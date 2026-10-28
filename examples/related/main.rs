//! Experimental read-only retrieval companion; not part of plied's CLI contract.
use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use clap::{Parser, ValueEnum};
use plied::access::Policy;
use plied::error::{Fail, Res};
use plied::fsx;
use plied::index::tokens;
use plied::records::Store;
use plied::{outln, repo};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Copy, Debug, ValueEnum)]
enum Mode {
    Lexical,
    Semantic,
    Hybrid,
}

#[derive(Parser)]
#[command(about = "Find candidate prior records; an agent must judge applicability")]
struct Cli {
    #[arg(short = 'C', default_value = ".")]
    directory: PathBuf,
    #[arg(short = 'n', default_value_t = 5)]
    limit: usize,
    #[arg(long, value_enum, default_value = "hybrid")]
    mode: Mode,
    /// Explicitly trusted local program; no shell is used.
    #[arg(long)]
    embedder: Option<PathBuf>,
    #[arg(long, allow_hyphen_values = true)]
    embed_arg: Vec<String>,
    /// Fail rather than silently omit passages when the inference budget is exceeded.
    #[arg(long, default_value_t = 256)]
    max_passages: usize,
    query: String,
}

#[derive(Clone, Serialize)]
struct Passage {
    id: String,
    path: String,
    revision: String,
    section: String,
    excerpt: String,
    body_summary: Option<String>,
    #[serde(skip)]
    text: String,
}

// Keep excerpts as exact substrings. Headings and technical identifiers survive;
// generated History is excluded before splitting. Model token limits still apply.
fn split(text: &str) -> Vec<String> {
    let mut rest = text.trim();
    let mut out = vec![];
    while !rest.is_empty() {
        let boundary = rest.char_indices().nth(800).map_or(rest.len(), |(b, _)| b);
        let mut end = boundary;
        if boundary < rest.len() {
            let prefix = &rest[..boundary];
            if let Some(b) = prefix
                .rfind(char::is_whitespace)
                .filter(|b| *b > boundary / 2)
            {
                end = b;
            }
        }
        out.push(rest[..end].to_string());
        rest = rest[end..].trim_start();
    }
    out
}

fn corpus(store: &Store, policy: &Policy) -> Vec<Passage> {
    let mut out = vec![];
    for r in &store.records {
        if r.parse_error.is_some() || !policy.searchable(&r.path) {
            continue;
        }
        let body = r
            .body
            .lines()
            .take_while(|l| l.trim() != "## History")
            .collect::<Vec<_>>()
            .join("\n");
        let mut parts = vec![("body", body)];
        if let Some(summary) = r.body_summary() {
            parts.push(("body_summary", summary));
        }
        for name in ["outcome.quote", "wording", "summary", "revisit", "answer"] {
            if let Some(value) = r.get(name).and_then(|n| n.scalar()) {
                parts.push((name, value));
            }
        }
        // A title-only record remains discoverable.
        if parts.iter().all(|(_, s)| s.trim().is_empty()) {
            parts.push(("title", r.title()));
        }
        for (section, text) in parts {
            for excerpt in split(&text) {
                out.push(Passage {
                    id: r.id.clone(),
                    path: fsx::rel(&store.cfg.base, &r.path),
                    revision: r.rev.clone(),
                    section: section.into(),
                    body_summary: r.body_summary(),
                    text: format!("{}\n{excerpt}", r.title()),
                    excerpt,
                });
            }
        }
    }
    out.sort_by(|a, b| (&a.path, &a.section, &a.excerpt).cmp(&(&b.path, &b.section, &b.excerpt)));
    out
}

/// BM25 with the core search's tokenizer, K1 and B, evaluated over passages.
fn lexical(passages: &[Passage], query: &str) -> Vec<(usize, f64)> {
    let terms: HashSet<String> = tokens(query).into_iter().collect();
    let docs: Vec<Vec<String>> = passages.iter().map(|p| tokens(&p.text)).collect();
    let n = docs.len() as f64;
    let avg = (docs.iter().map(Vec::len).sum::<usize>() as f64 / n.max(1.0)).max(1e-9);
    let mut scores = vec![0.0; docs.len()];
    for term in terms {
        let df = docs.iter().filter(|d| d.contains(&term)).count() as f64;
        let idf = (1.0 + (n - df + 0.5) / (df + 0.5)).ln();
        for (i, d) in docs.iter().enumerate() {
            let tf = d.iter().filter(|t| **t == term).count() as f64;
            scores[i] += idf * tf * 2.2 / (tf + 1.2 * (0.25 + 0.75 * d.len() as f64 / avg));
        }
    }
    ranking(
        scores
            .into_iter()
            .enumerate()
            .filter(|(_, s)| *s > 0.0)
            .collect(),
    )
}

fn ranking(mut values: Vec<(usize, f64)>) -> Vec<(usize, f64)> {
    values.sort_by(|(ai, a), (bi, b)| b.total_cmp(a).then(ai.cmp(bi)));
    values
}

fn fused(lists: &[&[(usize, f64)]]) -> Vec<(usize, f64)> {
    let mut scores: HashMap<usize, f64> = HashMap::new();
    for list in lists {
        for (rank, (i, _)) in list.iter().enumerate() {
            *scores.entry(*i).or_default() += 1.0 / (60.0 + rank as f64 + 1.0);
        }
    }
    ranking(scores.into_iter().collect())
}

#[derive(Deserialize)]
struct Embeddings {
    model: Value,
    query: Vec<f64>,
    documents: Vec<Vec<f64>>,
}

fn semantic_scores(v: &Embeddings, count: usize) -> Res<Vec<(usize, f64)>> {
    let dim = v.query.len();
    if dim == 0 || v.documents.len() != count || !v.model.is_object() {
        return Err(Fail::new("invalid embedder response shape"));
    }
    let norm = |a: &[f64]| a.iter().map(|x| x * x).sum::<f64>().sqrt();
    let qnorm = norm(&v.query);
    if !qnorm.is_finite() || qnorm <= 0.0 || v.query.iter().any(|x| !x.is_finite()) {
        return Err(Fail::new("invalid query vector"));
    }
    let mut scores = vec![];
    for (i, d) in v.documents.iter().enumerate() {
        let dn = norm(d);
        if d.len() != dim || !dn.is_finite() || dn <= 0.0 || d.iter().any(|x| !x.is_finite()) {
            return Err(Fail::new("invalid document vector"));
        }
        let score = v
            .query
            .iter()
            .zip(d)
            .map(|(a, b)| (a / qnorm) * (b / dn))
            .sum::<f64>();
        scores.push((i, score.clamp(-1.0, 1.0)));
    }
    Ok(ranking(scores))
}

fn embed(cli: &Cli, passages: &[Passage]) -> Res<Embeddings> {
    let program = cli.embedder.as_ref().ok_or_else(|| {
        Fail::new(
            "semantic/hybrid mode needs --embedder; use --mode lexical for keyword-only search",
        )
    })?;
    let mut child = Command::new(program)
        .args(&cli.embed_arg)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let request = json!({"v": 1, "query": cli.query, "documents": passages.iter().map(|p| &p.text).collect::<Vec<_>>()});
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| Fail::new("embedder stdin unavailable"))?;
    // Write concurrently with reading the output to avoid full-pipe deadlocks.
    let bytes = serde_json::to_vec(&request)?;
    let writer = std::thread::spawn(move || stdin.write_all(&bytes));
    let output = child.wait_with_output()?;
    writer
        .join()
        .map_err(|_| Fail::new("embedder input writer failed"))??;
    if !output.status.success() {
        return Err(Fail::new(
            "embedder failed; check the local model setup separately (worker stderr is suppressed to avoid echoing record text)",
        ));
    }
    serde_json::from_slice(&output.stdout)
        .map_err(|_| Fail::new("embedder did not return valid vector JSON"))
}

fn run(cli: &Cli) -> Res<Value> {
    if cli.query.trim().is_empty() {
        return Err(Fail::new("query must not be empty"));
    }
    if cli.max_passages == 0 {
        return Err(Fail::new("--max-passages must be positive"));
    }
    let repo = repo::locate(Some(&cli.directory), None)?;
    if fsx::pending_intent(&repo.runtime)? {
        return Err(Fail::new(
            "pending write: recover the store before retrieval",
        ));
    }
    let policy = Policy::new(&repo.cfg)?;
    let store = Store::load(&repo.cfg, None, &policy)?;
    if !store.load_problems.is_empty() {
        return Err(Fail::new(
            "record loading has errors; run plied check before retrieval",
        ));
    }
    let passages = corpus(&store, &policy);
    if passages.len() > cli.max_passages {
        return Err(Fail::new(
            "passage budget exceeded; narrow search.include or explicitly increase --max-passages",
        ));
    }
    let lex = lexical(&passages, &cli.query);
    let vectors = if cli.limit > 0 && !passages.is_empty() && !matches!(cli.mode, Mode::Lexical) {
        Some(embed(cli, &passages)?)
    } else {
        None
    };
    let sem = vectors
        .as_ref()
        .map(|v| semantic_scores(v, passages.len()))
        .transpose()?
        .unwrap_or_default();
    let ranked = match cli.mode {
        Mode::Lexical => lex.clone(),
        Mode::Semantic => sem.clone(),
        Mode::Hybrid => fused(&[&lex, &sem]),
    };
    // Reload the actual policy after embedding. Never return snapshot excerpts if
    // access changed during inference; a score is not permission to show a record.
    let current = repo::locate(Some(&cli.directory), None)?;
    let current_policy = Policy::new(&current.cfg)?;
    if fsx::pending_intent(&current.runtime)? {
        return Err(Fail::new(
            "pending write appeared during retrieval; recover before retrying",
        ));
    }
    if current.canonical != repo.canonical || current_policy.digest() != policy.digest() {
        return Err(Fail::new("access policy changed during retrieval; rerun"));
    }
    let mut seen = HashSet::new();
    let mut candidates = vec![];
    let mut changed = false;
    for (i, score) in ranked {
        if candidates.len() >= cli.limit {
            break;
        }
        let p = &passages[i];
        if !current_policy.searchable(&current.canonical.join(&p.path)) {
            continue;
        }
        let bytes = std::fs::read(current.canonical.join(&p.path));
        if bytes.as_ref().map_or(true, |b| fsx::sha(b) != p.revision) {
            changed = true;
            continue;
        }
        if !seen.insert(p.path.clone()) {
            continue;
        }
        let mut candidate = serde_json::to_value(p)?;
        candidate["score"] = json!(score);
        candidate["lexical_rank"] = json!(lex.iter().position(|(j, _)| *j == i).map(|r| r + 1));
        candidate["semantic_rank"] = json!(sem.iter().position(|(j, _)| *j == i).map(|r| r + 1));
        candidate["applicability"] = json!("unassessed");
        candidates.push(candidate);
    }
    Ok(
        json!({"v": 1, "ok": true, "mode": format!("{:?}", cli.mode).to_lowercase(),
        "policy": policy.digest(), "model": vectors.as_ref().map(|v| &v.model), "passages": passages.len(),
        "candidates": candidates, "changed_candidates_omitted": changed,
        "note": "Candidates require an explicit object/assumption comparison. Scores are not probabilities or gate verdicts. No hits do not establish novelty."}),
    )
}

fn main() {
    let cli = Cli::parse();
    match run(&cli) {
        Ok(result) => outln!("{result}"),
        Err(e) => {
            outln!("{}", json!({"v": 1, "ok": false, "error": e.msg}));
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splitting_preserves_unicode_and_exact_source_substrings() {
        let text = "αβγ integer premise removed\n".repeat(80);
        let parts = split(&text);
        assert!(parts.len() > 1);
        assert!(parts.iter().all(|s| text.contains(s)));
        assert_eq!(
            parts.join(" ").split_whitespace().collect::<Vec<_>>(),
            text.split_whitespace().collect::<Vec<_>>()
        );
    }

    #[test]
    fn fusion_retains_a_semantic_only_candidate_and_exact_keyword_match() {
        let out = fused(&[&[(0, 12.0), (1, 3.0)], &[(2, 0.9), (1, 0.8)]]);
        assert_eq!(out[0].0, 1);
        assert_eq!(
            out.iter().map(|x| x.0).collect::<HashSet<_>>(),
            HashSet::from([0, 1, 2])
        );
    }

    #[test]
    fn malformed_vectors_cannot_become_search_scores() {
        for docs in [
            vec![vec![1.0]],
            vec![vec![0.0, 0.0]],
            vec![vec![f64::NAN, 1.0]],
            vec![],
        ] {
            let v = Embeddings {
                model: json!({"name": "fixture"}),
                query: vec![1.0, 0.0],
                documents: docs,
            };
            assert!(semantic_scores(&v, 1).is_err());
        }
    }
}
