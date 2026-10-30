//! Checked writes to the canonical record (FORMAT.md §11).
//!
//! Every change names the revision it was made against. Under the store lock
//! the writer refuses, in order: a canonical checkout mid-merge/rebase/etc, on
//! a detached HEAD or the wrong branch; a differing version pin; a revision
//! that is not the expected one; a change that would add a validation error.
//! The changed records and the regenerated view are replaced as one
//! recoverable multi-file write (`fsx::commit_files`): a write interrupted by a
//! crash is finished or undone by the next one, or by `plied recover`.

use std::fmt;
use std::path::PathBuf;

use crate::access::Policy;
use crate::check::{self, Level};
use crate::doc::{Map, Node};
use crate::error::{Fail, Res, EXIT_BLOCKED, EXIT_CONFLICT, EXIT_INVALID, EXIT_USAGE};
use crate::fsx::{self, FileLock};
use crate::literature::Library;
use crate::records::{self, OwnKind, Record, Store};
use crate::repo::Repo;
use crate::views;
use crate::yamlish;

#[derive(Clone)]
pub struct Who {
    pub agent: String,
    pub session: Option<String>,
}

impl fmt::Display for Who {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.session {
            Some(s) => write!(f, "{} (session {})", self.agent, crate::doc::take(s, 8)),
            None => write!(f, "{}", self.agent),
        }
    }
}

pub struct Change {
    pub path: PathBuf,
    pub data: Vec<u8>,
    /// expected revision prefix, or "absent"
    pub expected: String,
    pub rid: String,
}

/// A mutation runs against the store loaded while the transaction owns the write lock.
pub type Mutate<'m> = dyn FnMut(&Store, &mut Map) -> Res<String> + 'm;

pub struct Writer<'a> {
    pub repo: &'a Repo,
    pub lock: &'a FileLock,
}

impl<'a> Writer<'a> {
    pub fn new(repo: &'a Repo, lock: &'a FileLock) -> Self {
        Writer { repo, lock }
    }

    pub fn remember(&self, data: &[u8], written: bool) -> Res<String> {
        fsx::remember(&self.repo.runtime, data, written)
    }

    pub fn written_revs(&self) -> std::collections::HashSet<String> {
        fsx::written(&self.repo.runtime)
    }

    /// Run `build` against the current store and write what it returns.
    pub fn transaction(
        &self,
        policy: &Policy,
        library: Option<&Library>,
        build: &mut dyn FnMut(&Store) -> Res<Vec<Change>>,
    ) -> Res<Vec<(String, String)>> {
        let cfg = &self.repo.cfg;
        let _g = self.lock.lock()?;
        self.refuse_blocked()?;
        self.recover_first()?;
        let store = Store::load(cfg, None, policy)?;
        let changes = build(&store)?;
        for ch in &changes {
            self.check_destination(policy, &ch.path, false)?;
            self.check_expected(&ch.path, &ch.expected, &ch.rid, &store)?;
        }
        let inputs = check::Inputs { library, messages: None, view: None, warnings: false };
        // an error is "added" when its identity (code, location, subject) is new; wording never counts (§8)
        let before = check::run(&store, &inputs);
        let before: std::collections::HashSet<_> = before.iter().filter(|p| p.level == Level::Error).map(|p| p.identity()).collect();
        let overrides: records::Overrides = changes.iter().map(|c| (c.path.clone(), Some(c.data.clone()))).collect();
        let proposed = Store::load(cfg, Some(&overrides), policy)?;
        for ch in &changes {
            if proposed.get(&ch.rid).is_none_or(|r| fsx::resolve(&r.path) != fsx::resolve(&ch.path) || r.rev != fsx::sha(&ch.data)) {
                return Err(Fail::policy(format!("{} did not enter the proposed store for validation", ch.rid)));
            }
        }
        let after = check::run(&proposed, &inputs);
        let mut new: Vec<check::Problem> = after.iter().filter(|p| p.level == Level::Error && !before.contains(&p.identity())).cloned().collect();
        for ch in &changes {
            if let Some(updated) = proposed.get(&ch.rid).filter(|r| r.is(OwnKind::Commitment)) {
                let previous = store.get(&ch.rid).and_then(|r| r.meta.get("reviews")).map(Node::items).unwrap_or_default();
                let next = updated.meta.get("reviews").map(Node::items).unwrap_or_default();
                // Every newly appended evidence claim has the same under-lock read preconditions,
                // including claims supplied through generic set/replace rather than dispose.
                if next.starts_with(previous) {
                    for review in &next[previous.len()..] {
                        for input in review.map().and_then(|rv| rv.get("inputs")).map(Node::items).unwrap_or_default() {
                            if let Some(pin) = input.map().and_then(crate::review::ReadInput::from_map)
                                && crate::review::full_revision(&pin.rev) && crate::review::input_id(&store, &pin.record) && pin.record != ch.rid
                            {
                                pin.check(&store)?;
                            }
                        }
                    }
                }
            }
            if let (Some(old), Some(updated)) = (store.get(&ch.rid), proposed.get(&ch.rid)) {
                if let Some(problem) = outcome_dropped(old, updated) {
                    new.push(check::Problem::error_on("outcome-kept", &self.repo.rel(&ch.path), "outcome", problem));
                }
                for field in ["reviews", "scanned", "prior_outcomes"] {
                    let previous = old.meta.get(field).map(Node::items).unwrap_or_default();
                    let next = updated.meta.get(field).map(Node::items).unwrap_or_default();
                    if !next.starts_with(previous) {
                        new.push(check::Problem::error_on("append-only", &self.repo.rel(&ch.path), field,
                            format!("{field} is append-only: preserve existing entries and their order")));
                    }
                }
            }
        }
        new.sort();
        if !new.is_empty() {
            return Err(Fail::code(
                EXIT_INVALID,
                format!("the change would add validation errors:\n{}", new.iter().map(|p| format!("  {p}")).collect::<Vec<_>>().join("\n")),
            )
            .with_detail(serde_json::json!({ "diagnostics": new })));
        }
        let mut files: Vec<(PathBuf, Vec<u8>)> = changes.iter().map(|c| (c.path.clone(), c.data.clone())).collect();
        self.check_destination(policy, &cfg.view_path(), true)?;
        files.extend(self.view_file(&proposed));
        fsx::commit_files(&self.repo.runtime, &files)?;
        // the write is done: failing to prune the cache is worth a note, never a failure
        if let Err(e) = fsx::prune_revisions_daily(&self.repo.runtime, cfg.revision_days) {
            crate::notes::note(format!("could not prune the revision cache: {}", e.msg));
        }
        Ok(changes.iter().map(|c| (c.rid.clone(), fsx::sha(&c.data))).collect())
    }

    /// Check both the alias policy and resolved destination before reading or committing bytes.
    fn check_destination(&self, policy: &Policy, path: &std::path::Path, view: bool) -> Res<()> {
        let real = policy.check_readable(path)?;
        let cfg = &self.repo.cfg;
        let root = fsx::resolve(&cfg.base.join(&cfg.root));
        let allowed = real.starts_with(&root) && if view {
            real.parent() == Some(root.as_path())
        } else {
            OwnKind::KINDS.iter().any(|kind| real.parent() == Some(fsx::resolve(&cfg.kind_dir(*kind)).as_path()))
        };
        if !allowed {
            return Err(Fail::policy("write destination resolves outside its configured record directory"));
        }
        Ok(())
    }

    /// Remove cached revision content past its retention, or all of it (`plied gc`).
    pub fn prune(&self, all: bool) -> Res<usize> {
        let _g = self.lock.lock()?;
        fsx::prune_revisions(&self.repo.runtime, if all { 0 } else { self.repo.cfg.revision_days })
    }

    /// The refusals every write shares: branch and merge state, then the version pin.
    fn refuse_blocked(&self) -> Res<()> {
        let blockers = self.repo.write_blockers();
        if !blockers.is_empty() {
            return Err(Fail::code(EXIT_BLOCKED, format!("writes are refused: {}", blockers.join("; "))));
        }
        if let Some((_, msg)) = self.repo.cfg.compat_problems().into_iter().next() {
            return Err(Fail::code(EXIT_BLOCKED, format!("writes are refused: {msg}")));
        }
        Ok(())
    }

    /// Finish or undo an earlier write a crash interrupted, before this one. That is a separate
    /// outcome from this write's, so it is reported on its own line even if this write is refused.
    fn recover_first(&self) -> Res<()> {
        if let Some(r) = fsx::recover(&self.repo.runtime)? {
            crate::notes::note(format!("before this command: {r}"));
        }
        Ok(())
    }

    /// Finish or undo an interrupted write and nothing else (`plied recover`).
    pub fn recover(&self) -> Res<Option<fsx::Recovery>> {
        let _g = self.lock.lock()?;
        self.refuse_blocked()?;
        fsx::recover(&self.repo.runtime)
    }

    fn check_expected(&self, path: &std::path::Path, expected: &str, rid: &str, store: &Store) -> Res<()> {
        if expected == "absent" {
            if path.exists() {
                let cur = store.get(rid);
                return Err(conflict(rid, expected, cur, None));
            }
            return Ok(());
        }
        if expected.len() < 8 || !expected.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(Fail::code(
                EXIT_USAGE,
                "--expect needs the revision you read (at least 8 hex characters; `plied show` prints it)",
            ));
        }
        let Ok(current) = std::fs::read(path) else {
            return Err(conflict(rid, expected, None, None));
        };
        let rev = fsx::sha(&current);
        if rev.starts_with(expected) {
            return Ok(());
        }
        let diff = fsx::recall(&self.repo.runtime, expected).map(|old| {
            unified_diff(
                &String::from_utf8_lossy(&old),
                &String::from_utf8_lossy(&current),
                &format!("{rid}@{}", &expected[..expected.len().min(12)]),
                &format!("{rid}@{}", &rev[..12]),
            )
        });
        Err(conflict(rid, expected, store.get(rid), diff))
    }

    pub fn regenerate(&self, store: Option<&Store>) -> Res<()> {
        let Some(store) = store else {
            return Err(Fail::new("internal: regenerate needs a store"));
        };
        self.check_destination(&Policy::new(&self.repo.cfg)?, &self.repo.cfg.view_path(), true)?;
        let files: Vec<_> = self.view_file(store).into_iter().collect();
        fsx::commit_files(&self.repo.runtime, &files)
    }

    /// The view's path and new text, when it differs from what is on disk.
    fn view_file(&self, store: &Store) -> Option<(PathBuf, Vec<u8>)> {
        let text = views::render_view(store);
        let p = self.repo.cfg.view_path();
        let current = if p.exists() { fsx::read_text(&p).ok() } else { None };
        (current.as_deref() != Some(text.as_str())).then(|| (p, text.into_bytes()))
    }

    /// Regenerate only (the `plied view` write path).
    pub fn regenerate_view(&self, policy: &Policy) -> Res<()> {
        let _g = self.lock.lock()?;
        self.refuse_blocked()?;
        self.recover_first()?;
        let store = Store::load(&self.repo.cfg, None, policy)?;
        self.regenerate(Some(&store))
    }

    #[allow(clippy::too_many_arguments)]
    pub fn create(&self, policy: &Policy, library: Option<&Library>, kind: OwnKind, mut meta: Map, who: &Who, why: Option<&str>, body: Option<String>) -> Res<(String, String)> {
        let cfg = self.repo.cfg.clone();
        let mut build = |store: &Store| -> Res<Vec<Change>> {
            let rid = store.next_id(kind);
            let mut m = Map::new();
            m.insert("id".into(), Node::Str(rid.clone()));
            for (k, v) in meta.iter() {
                m.insert(k.clone(), v.clone());
            }
            meta = m.clone();
            let text = append_history(body.as_deref().unwrap_or(""), &format!("{who}: created{}", why_suffix(why)));
            let title = crate::doc::text(&m, "title").unwrap_or_default();
            let path = cfg.kind_dir(kind).join(format!("{rid}-{}.md", records::slugify(&title)));
            let data = records::render(kind, &m, &text).into_bytes();
            Ok(vec![Change { path, data, expected: "absent".into(), rid }])
        };
        let out = self.transaction(policy, library, &mut build)?;
        out.into_iter().next().ok_or_else(|| Fail::new("nothing written"))
    }

    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &self,
        policy: &Policy,
        library: Option<&Library>,
        rid: &str,
        expected: &str,
        who: &Who,
        why: Option<&str>,
        assignments: &[String],
        note: Option<&str>,
        replace_text: Option<&str>,
        mutate: Option<&mut Mutate>,
    ) -> Res<String> {
        let mut mutate = mutate;
        let mut build = |store: &Store| -> Res<Vec<Change>> {
            let r = store.get(rid).filter(|r| r.is_own()).ok_or_else(|| Fail::new(format!("{rid} is not a plied record")))?;
            self.check_expected(&r.path, expected, rid, store)?;
            let (mut meta, mut body, mut what);
            if let Some(replace_text) = replace_text {
                let (fm, b) = yamlish::split_frontmatter(replace_text).map_err(|e| Fail::new(e.0))?;
                let fm = fm.ok_or_else(|| Fail::new("the replacement has no frontmatter"))?;
                meta = records::coerce(yamlish::parse(&fm).map_err(|e| Fail::new(e.0))?);
                if crate::doc::text(&meta, "id").as_deref() != Some(r.id.as_str()) {
                    return Err(Fail::new(format!("the replacement must keep id {}", r.id)));
                }
                body = b;
                what = "replaced the record".to_string();
            } else {
                meta = r.meta.clone();
                body = r.body.clone();
                what = if assignments.is_empty() { String::new() } else { apply_assignments(&mut meta, assignments)? };
            }
            if let Some(m) = mutate.as_deref_mut() {
                let w = m(store, &mut meta)?;
                if !w.is_empty() {
                    what = w;
                }
            }
            if let Some(note) = note {
                what = if what.is_empty() { note.to_string() } else { format!("{what}; {note}") };
            }
            if what.is_empty() {
                return Err(Fail::new("nothing to change"));
            }
            body = append_history(&body, &format!("{who}: {what}{}", why_suffix(why)));
            let kind = r.own().unwrap();
            let data = records::render(kind, &meta, &body).into_bytes();
            Ok(vec![Change { path: r.path.clone(), data, expected: expected.to_string(), rid: r.id.clone() }])
        };
        let out = self.transaction(policy, library, &mut build)?;
        out.into_iter().next().map(|(_, rev)| rev).ok_or_else(|| Fail::new("nothing written"))
    }
}

/// A write that clears or changes a thread's outcome must keep the old one in `prior_outcomes`, appended
/// in the same write (§11), so a conclusion is never lost from the record that reached it.
fn outcome_dropped(old: &Record, new: &Record) -> Option<String> {
    let pair = |r: &Record| match r.meta.get("outcome") {
        Some(Node::Map(o)) => Some((crate::doc::text(o, "quote").unwrap_or_default(), crate::doc::text(o, "ref").unwrap_or_default())),
        _ => None,
    };
    let before = pair(old).filter(|(q, f)| !q.is_empty() || !f.is_empty())?;
    if pair(new).as_ref() == Some(&before) {
        return None;
    }
    let kept = |r: &Record| r.meta.get("prior_outcomes").map(Node::items).unwrap_or_default().to_vec();
    let (was, now) = (kept(old), kept(new));
    let appended = now.get(was.len()..).unwrap_or_default();
    let keeps = appended.iter().filter_map(Node::map).any(|m| {
        (crate::doc::text(m, "quote").unwrap_or_default(), crate::doc::text(m, "ref").unwrap_or_default()) == before
    });
    (!keeps).then(|| "this write would clear or change outcome without keeping it in prior_outcomes; use `plied reopen`".to_string())
}

fn why_suffix(why: Option<&str>) -> String {
    why.filter(|w| !w.is_empty()).map(|w| format!(" — {w}")).unwrap_or_default()
}

fn conflict(rid: &str, expected: &str, current: Option<&Record>, diff: Option<String>) -> Fail {
    let cur = current.map(|c| &c.rev[..12]).unwrap_or("absent");
    let shown = if expected.len() > 12 { &expected[..12] } else { expected };
    let mut msg = format!("conflict: {rid} is at revision {cur}, not {shown}. Reconcile against the current record and retry.");
    match diff {
        Some(d) => msg += &format!("\n\nChanges since {shown}:\n{d}"),
        None if current.is_some() => {
            msg += "\n\n(the revision you read is not in the cache, so no diff; the current record follows)"
        }
        None => {}
    }
    if let Some(c) = current {
        msg += &format!("\n\nCurrent record ({}):\n{}", &c.rev[..12], c.text);
    }
    let detail = serde_json::json!({ "id": rid, "expected": expected, "current": current.map(|c| c.rev.clone()) });
    Fail::code(EXIT_CONFLICT, msg).with_detail(detail)
}

/// Append a history line under `## History` (created at the end when missing). §3.5.
pub fn append_history(body: &str, line: &str) -> String {
    let entry = format!("- {} {line}", records::stamp());
    let trimmed = body.strip_suffix('\n').unwrap_or(body);
    let mut lines: Vec<String> = if trimmed.trim().is_empty() { vec![] } else { trimmed.split('\n').map(String::from).collect() };
    let idx = lines.iter().rposition(|l| l.trim().eq_ignore_ascii_case("## history"));
    match idx {
        None => {
            lines.push(String::new());
            lines.push("## History".into());
            lines.push(entry);
        }
        Some(idx) => {
            let mut j = idx + 1;
            while j < lines.len() && !lines[j].starts_with("## ") {
                j += 1;
            }
            let mut k = j;
            while k > idx + 1 && lines[k - 1].trim().is_empty() {
                k -= 1;
            }
            lines.insert(k, entry);
        }
    }
    lines.join("\n") + "\n"
}

fn parse_value(raw: &str) -> Res<Option<Node>> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Ok(None);
    }
    if raw.starts_with(['[', '"', '\'']) {
        return yamlish::scalar(raw, 0).map(Some).map_err(|e| Fail::new(e.0));
    }
    if raw.bytes().all(|b| b.is_ascii_digit())
        && let Ok(n) = raw.parse::<i64>() {
            return Ok(Some(Node::Int(n)));
        }
    Ok(Some(Node::Str(raw.to_string())))
}

fn show(v: &Node) -> String {
    let s = match v {
        Node::Str(s) => s.clone(),
        other => other.to_json().to_string(),
    };
    if crate::doc::clen(&s) <= 80 { s } else { format!("{}…", crate::doc::take(&s, 77)) }
}

/// Apply `key=value`, `key+=value`, `key-=value` (dotted keys nest). Returns a summary.
pub fn apply_assignments(meta: &mut Map, assignments: &[String]) -> Res<String> {
    let mut done: Vec<String> = vec![];
    for a in assignments {
        // the operator is at the first `=`, so a value may itself contain `+=` or `-=`
        let Some(eq) = a.find('=') else {
            return Err(Fail::new(format!("cannot read assignment {a:?}: use key=value, key+=value or key-=value")));
        };
        let (op, key_raw) = match (a[..eq].strip_suffix('+'), a[..eq].strip_suffix('-')) {
            (Some(k), _) => ("+=", k),
            (_, Some(k)) => ("-=", k),
            _ => ("=", &a[..eq]),
        };
        let raw = &a[eq + 1..];
        let key = key_raw.trim().to_string();
        let val = parse_value(raw)?;
        let parts: Vec<String> = key.split('.').map(String::from).collect();
        if parts.last().map(String::as_str) == Some("id") {
            return Err(Fail::new("ids never change"));
        }
        // walk to the parent map
        apply_at(meta, &parts, &key, op, val, &mut done)?;
    }
    Ok(done.join("; "))
}

fn apply_at(cur: &mut Map, parts: &[String], key: &str, op: &str, val: Option<Node>, done: &mut Vec<String>) -> Res<()> {
    if parts.len() > 1 {
        let head = parts[0].clone();
        let entry = cur.entry(head.clone()).or_insert_with(|| Node::Map(Map::new()));
        if !matches!(entry, Node::Map(_)) {
            *entry = Node::Map(Map::new());
        }
        let Node::Map(inner) = entry else { unreachable!() };
        apply_at(inner, &parts[1..], key, op, val, done)?;
        if inner.is_empty() {
            cur.shift_remove(&head);
        }
        return Ok(());
    }
    let last = parts[0].clone();
    match op {
        "=" => match val {
            None => {
                cur.shift_remove(&last);
                done.push(format!("cleared {key}"));
            }
            Some(mut v) => {
                if records::LIST_FIELDS.contains(&key) && !matches!(v, Node::List(_)) {
                    v = Node::List(vec![v]);
                }
                done.push(format!("set {key}={}", show(&v)));
                cur.insert(last, v);
            }
        },
        _ => {
            let items: Vec<Node> = match val {
                Some(Node::List(l)) => l,
                Some(v) => vec![v],
                None => vec![],
            };
            let mut lst: Vec<Node> = match cur.get(&last) {
                Some(Node::List(l)) => l.clone(),
                Some(other) if !other.is_empty() => vec![other.clone()],
                _ => vec![],
            };
            if op == "+=" {
                for x in &items {
                    if !lst.contains(x) {
                        lst.push(x.clone());
                    }
                }
                done.push(format!("added {} to {key}", items.iter().map(show).collect::<Vec<_>>().join(", ")));
            } else {
                lst.retain(|x| !items.contains(x));
                done.push(format!("removed {} from {key}", items.iter().map(show).collect::<Vec<_>>().join(", ")));
            }
            cur.insert(last, Node::List(lst));
        }
    }
    Ok(())
}

/// Minimal unified diff (line LCS), for conflict reports.
pub fn unified_diff(old: &str, new: &str, from: &str, to: &str) -> String {
    let a: Vec<&str> = old.lines().collect();
    let b: Vec<&str> = new.lines().collect();
    // The LCS table is quadratic in the line counts: two long records (a body can hold
    // tens of thousands of lines) would need gigabytes for a conflict message. Past a
    // generous size, show the common head and tail and the whole changed middle instead.
    if (a.len() as u128) * (b.len() as u128) > 4_000_000 {
        let mut out = format!("--- {from}\n+++ {to}\n");
        let head = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
        let tail = a[head..].iter().rev().zip(b[head..].iter().rev()).take_while(|(x, y)| x == y).count();
        for l in &a[..head] {
            out += &format!(" {l}\n");
        }
        for l in &a[head..a.len() - tail] {
            out += &format!("-{l}\n");
        }
        for l in &b[head..b.len() - tail] {
            out += &format!("+{l}\n");
        }
        for l in &a[a.len() - tail..] {
            out += &format!(" {l}\n");
        }
        return out;
    }
    // LCS table
    let (n, m) = (a.len(), b.len());
    let mut dp = vec![vec![0usize; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            dp[i][j] = if a[i] == b[j] { dp[i + 1][j + 1] + 1 } else { dp[i + 1][j].max(dp[i][j + 1]) };
        }
    }
    let mut out = format!("--- {from}\n+++ {to}\n");
    let (mut i, mut j) = (0, 0);
    while i < n && j < m {
        if a[i] == b[j] {
            out += &format!(" {}\n", a[i]);
            i += 1;
            j += 1;
        } else if dp[i + 1][j] >= dp[i][j + 1] {
            out += &format!("-{}\n", a[i]);
            i += 1;
        } else {
            out += &format!("+{}\n", b[j]);
            j += 1;
        }
    }
    while i < n {
        out += &format!("-{}\n", a[i]);
        i += 1;
    }
    while j < m {
        out += &format!("+{}\n", b[j]);
        j += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_small_diff_is_a_line_diff() {
        let d = unified_diff("a\nb\nc\n", "a\nx\nc\n", "old", "new");
        assert!(d.contains("-b\n") && d.contains("+x\n") && d.contains(" a\n"), "{d}");
    }

    #[test]
    fn a_huge_diff_does_not_build_the_quadratic_table() {
        // 3_000 x 3_000 lines is past the cap: the changed middle is shown whole,
        // the common head and tail as context, without an LCS table
        let old: String = (0..3_000).map(|i| format!("line {i}\n")).collect();
        let new = old.replace("line 1500\n", "changed 1500\n");
        let d = unified_diff(&old, &new, "old", "new");
        assert!(d.contains("-line 1500\n") && d.contains("+changed 1500\n"), "{}", &d[..d.len().min(200)]);
        assert!(d.contains(" line 0\n") && d.contains(" line 2999\n"), "common head and tail are context");
    }
}

