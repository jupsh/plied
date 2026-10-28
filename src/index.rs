//! Keyword search (BM25) over allowed records and cleared paper pages (§9, §10).
//! The index lives in the runtime directory and stores the access-policy digest
//! it was built under; searching under a different policy is refused.

use std::collections::HashMap;
use std::path::PathBuf;

use regex::Regex;
use serde_json::{Value, json};
use std::sync::LazyLock;

use crate::access::Policy;
use crate::config::Config;
use crate::error::{Fail, Res};
use crate::fsx;
use crate::globpat;
use crate::literature::Library;
use crate::records::Store;

static TOKEN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\w+(?:\.\d+)?").unwrap());
const K1: f64 = 1.2;
const B: f64 = 0.75;

pub fn tokens(text: &str) -> Vec<String> {
    TOKEN.find_iter(text).map(|m| m.as_str().to_lowercase()).collect()
}

#[derive(Clone)]
pub struct Hit {
    pub key: String,
    pub kind: String,
    pub path: PathBuf,
    pub score: f64,
    pub snippet: String,
    pub stale: bool,
    pub ocr: bool,
}

fn sources(cfg: &Config, policy: &Policy) -> Vec<PathBuf> {
    let mut seen = std::collections::HashSet::new();
    let mut out = vec![];
    for pat in &cfg.search_include {
        for name in globpat::glob(&cfg.base, pat) {
            let p = cfg.base.join(&name);
            if p.is_file() && policy.searchable(&p) {
                let r = fsx::resolve(&p);
                if seen.insert(r) {
                    out.push(p);
                }
            }
        }
    }
    out.sort();
    out
}

pub fn build(cfg: &Config, policy: &Policy, runtime: &std::path::Path, store: Option<&Store>) -> Res<Value> {
    let mut docs: Vec<Value> = vec![];
    let mut postings: HashMap<String, Vec<Value>> = HashMap::new();
    let ids_by_path: HashMap<PathBuf, String> = store
        .map(|s| s.records.iter().map(|r| (fsx::resolve(&r.path), r.id.clone())).collect())
        .unwrap_or_default();

    #[allow(clippy::too_many_arguments)]
    fn add(
        docs: &mut Vec<Value>,
        postings: &mut HashMap<String, Vec<Value>>,
        cfg: &Config,
        key: String,
        kind: &str,
        path: &std::path::Path,
        text: &str,
        data: &[u8],
        extra: Value,
    ) -> Res<()> {
        let md = std::fs::metadata(path)?;
        let toks = tokens(text);
        let idx = docs.len();
        let mut doc = json!({
            "key": key, "kind": kind, "path": fsx::rel(&cfg.base, path),
            "size": md.len(), "mtime_ns": fsx::mtime_ns(&md) as u64,
            "sha": fsx::sha(data), "len": toks.len(),
        });
        if let (Some(o), Some(e)) = (doc.as_object_mut(), extra.as_object()) {
            for (k, v) in e {
                o.insert(k.clone(), v.clone());
            }
        }
        docs.push(doc);
        let mut counts: HashMap<String, usize> = HashMap::new();
        for t in toks {
            *counts.entry(t).or_default() += 1;
        }
        for (t, n) in counts {
            postings.entry(t).or_default().push(json!([idx, n]));
        }
        Ok(())
    }

    for p in sources(cfg, policy) {
        let data = std::fs::read(&p)?;
        let text = String::from_utf8_lossy(&data).into_owned();
        let key = ids_by_path.get(&fsx::resolve(&p)).cloned().unwrap_or_else(|| fsx::rel(&cfg.base, &p));
        add(&mut docs, &mut postings, cfg, key, "record", &p, &text, &data, Value::Null)?;
    }
    if cfg.literature.is_some()
        && let Ok(lib) = Library::new(cfg, policy, runtime)
    {
        for paper in lib.papers.values() {
            for page in lib.pages(paper) {
                let pp = lib.page_path(paper, page);
                if !policy.literature_readable(&pp) {
                    continue;
                }
                let Ok(data) = std::fs::read(&pp) else { continue };
                let text = String::from_utf8_lossy(&data).into_owned();
                let ocr = lib.is_ocr(&text);
                add(
                    &mut docs,
                    &mut postings,
                    cfg,
                    format!("{} p{page}", paper.id),
                    "page",
                    &pp,
                    &text,
                    &data,
                    json!({"paper": paper.id, "page": page, "ocr": ocr}),
                )?;
            }
        }
    }
    let total: usize = docs.iter().map(|d| d["len"].as_u64().unwrap_or(0) as usize).sum();
    let avg = if docs.is_empty() { 0.0 } else { total as f64 / docs.len() as f64 };
    let ix = json!({
        "format": 1, "policy": policy.digest(),
        "built": crate::messages::iso(),
        "avgdl": avg, "docs": docs, "postings": postings,
    });
    let out = runtime.join("index.json");
    fsx::atomic_write(&out, serde_json::to_string(&ix)?.as_bytes())?;
    Ok(ix)
}

pub fn load(runtime: &std::path::Path) -> Res<Value> {
    let p = runtime.join("index.json");
    if !p.exists() {
        return Err(Fail::new("no search index yet: run `plied index`"));
    }
    let text = fsx::read_text(&p)?;
    serde_json::from_str(&text).map_err(|e| Fail::new(format!("{}: {e}", p.display())))
}

pub fn search(
    cfg: &Config,
    policy: &Policy,
    runtime: &std::path::Path,
    query: &str,
    limit: usize,
    kind: Option<&str>,
    paper: Option<&str>,
) -> Res<(Vec<Hit>, Vec<String>)> {
    let ix = load(runtime)?;
    if ix.get("policy").and_then(Value::as_str) != Some(policy.digest().as_str()) {
        return Err(Fail::new(
            "the search index was built under a different access policy (sealed paths, manifest or allowlist changed); it will not be used. Rebuild it with `plied index`.",
        ));
    }
    let mut q = tokens(query);
    q.dedup();
    let docs = ix["docs"].as_array().cloned().unwrap_or_default();
    let n_docs = docs.len() as f64;
    let avg = ix["avgdl"].as_f64().unwrap_or(1.0).max(1e-9);
    let empty = HashMap::new();
    let postings = ix["postings"].as_object().map(|o| {
        o.iter().map(|(k, v)| (k.clone(), v.clone())).collect::<HashMap<_, _>>()
    }).unwrap_or(empty);
    let mut scores: HashMap<usize, f64> = HashMap::new();
    for t in &q {
        let Some(plist) = postings.get(t).and_then(Value::as_array) else { continue };
        let df = plist.len() as f64;
        let idf = (1.0 + (n_docs - df + 0.5) / (df + 0.5)).ln();
        for item in plist {
            let (Some(i), Some(tf)) = (item[0].as_u64(), item[1].as_f64()) else { continue };
            let i = i as usize;
            let Some(d) = docs.get(i) else { continue };
            if kind.is_some_and(|k| d["kind"].as_str() != Some(k)) {
                continue;
            }
            if paper.is_some_and(|p| d.get("paper").and_then(Value::as_str) != Some(p)) {
                continue;
            }
            let len = d["len"].as_f64().unwrap_or(0.0);
            *scores.entry(i).or_default() += idf * tf * (K1 + 1.0) / (tf + K1 * (1.0 - B + B * len / avg));
        }
    }
    let mut ranked: Vec<(usize, f64)> = scores.into_iter().collect();
    ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    let mut notes = vec![];
    let mut hits = vec![];
    for (i, score) in ranked {
        if hits.len() >= limit {
            break; // a limit of 0 returns no hits, not one
        }
        let d = &docs[i];
        let path = cfg.base.join(d["path"].as_str().unwrap_or_default());
        if policy.check_readable(&path).is_err() {
            continue;
        }
        if !path.exists() {
            notes.push(format!("{} was indexed but no longer exists", d["path"].as_str().unwrap_or_default()));
            continue;
        }
        let ok = if d["kind"].as_str() == Some("record") { policy.searchable(&path) } else { policy.literature_readable(&path) };
        if !ok {
            continue;
        }
        let data = std::fs::read(&path)?;
        let stale = fsx::sha(&data) != d["sha"].as_str().unwrap_or_default();
        hits.push(Hit {
            key: d["key"].as_str().unwrap_or_default().to_string(),
            kind: d["kind"].as_str().unwrap_or_default().to_string(),
            path,
            score,
            snippet: snippet(&String::from_utf8_lossy(&data), &q, 220),
            stale,
            ocr: d.get("ocr").and_then(Value::as_bool).unwrap_or(false),
        });
        if hits.len() >= limit {
            break;
        }
    }
    if hits.iter().any(|h| h.stale) {
        notes.push("some results changed since the index was built: rerun `plied index`".into());
    }
    Ok((hits, notes))
}

pub fn snippet(text: &str, q: &[String], width: usize) -> String {
    let flat: Vec<char> = crate::doc::squash(text).chars().collect();
    let flat_s: String = flat.iter().collect();
    let low = flat_s.to_lowercase();
    // char offset of each byte offset in `low`, so term positions found as bytes can be
    // counted and windowed in chars without scanning the text once per position
    let byte_to_char: Vec<usize> = low.char_indices().map(|(b, _)| b).collect();
    let char_of = |byte: usize| byte_to_char.partition_point(|&b| b < byte);
    let mut char_positions: Vec<usize> = vec![];
    for t in q {
        if t.is_empty() {
            continue;
        }
        for (b, _) in low.match_indices(t.as_str()) {
            char_positions.push(char_of(b));
        }
    }
    char_positions.sort_unstable();
    // the window of `width` chars holding the most term occurrences (two pointers)
    let (mut best, mut best_n, mut lo) = (0usize, -1i64, 0usize);
    for (hi, &p) in char_positions.iter().enumerate() {
        while p >= char_positions[lo] + width {
            lo += 1;
        }
        let n = (hi - lo + 1) as i64;
        if n > best_n {
            best = char_positions[lo];
            best_n = n;
        }
    }
    let start = best.saturating_sub(40);
    let s: String = flat.iter().skip(start).take(width).collect::<String>().trim().to_string();
    let pre = if start > 0 { "…" } else { "" };
    let post = if start + width < flat.len() { "…" } else { "" };
    format!("{pre}{s}{post}")
}

