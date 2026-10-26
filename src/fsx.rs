//! Paths and files: lenient resolution (like `Path.resolve()`), repo-relative names, atomic
//! replacement, an exclusive cross-platform lock, and hashing.

use std::cell::{Cell, RefCell};
use std::fs::{self, File, OpenOptions, TryLockError};
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};

use crate::error::{Fail, IoContext, Res};

pub fn sha(data: &[u8]) -> String {
    use std::fmt::Write as _;
    let d = Sha256::digest(data);
    let mut out = String::with_capacity(d.len() * 2);
    for b in d {
        let _ = write!(out, "{b:02x}");
    }
    out
}

/// The real path: symlinks followed as far as the path exists, `..` applied, never failing.
pub fn resolve(p: &Path) -> PathBuf {
    let abs = if p.is_absolute() { p.to_path_buf() } else { std::env::current_dir().unwrap_or_default().join(p) };
    let mut hops = 0;
    resolve_from(PathBuf::new(), &abs, &mut hops)
}

fn resolve_from(mut out: PathBuf, p: &Path, hops: &mut usize) -> PathBuf {
    for c in p.components() {
        match c {
            Component::Prefix(_) | Component::RootDir => out.push(c.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            Component::Normal(name) => {
                let next = out.join(name);
                match fs::read_link(&next) {
                    Ok(target) if *hops < 40 => {
                        *hops += 1;
                        let base = if target.is_absolute() { PathBuf::new() } else { out.clone() };
                        out = resolve_from(base, &target, hops);
                    }
                    _ => out = next,
                }
            }
        }
    }
    out
}

pub fn posix(p: &Path) -> String {
    p.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect::<Vec<_>>().join("/")
}

/// `p` relative to `base` with `/` separators, or `p` itself when it is outside.
pub fn rel(base: &Path, p: &Path) -> String {
    match resolve(p).strip_prefix(resolve(base)) {
        Ok(r) => posix(r),
        Err(_) => p.display().to_string(),
    }
}

pub fn file_name(p: &Path) -> String {
    p.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
}

pub fn stem(p: &Path) -> String {
    p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
}

/// A text file with CRLF line ends read as LF, invalid UTF-8 replaced.
pub fn read_text(p: &Path) -> Res<String> {
    let data = fs::read(p).at(p)?;
    Ok(String::from_utf8_lossy(&data).replace("\r\n", "\n"))
}

pub fn mtime_ns(md: &fs::Metadata) -> u128 {
    md.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map_or(0, |d| d.as_nanos())
}

pub fn now_secs() -> f64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64())
}

pub fn random_bytes<const N: usize>() -> [u8; N] {
    let mut b = [0u8; N];
    if getrandom::getrandom(&mut b).is_err() {
        let t = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos());
        for (i, x) in b.iter_mut().enumerate() {
            *x = (t >> ((i % 16) * 8)) as u8 ^ (std::process::id() as u8).wrapping_mul(i as u8 + 1);
        }
    }
    b
}

/// Write to a temporary file in the same directory, fsync, then rename over `path`.
pub fn atomic_write(path: &Path, data: &[u8]) -> Res<()> {
    let dir = path.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(dir).at(dir)?;
    let tag: String = random_bytes::<6>().iter().map(|b| format!("{b:02x}")).collect();
    let tmp = dir.join(format!(".{}.{tag}.tmp", file_name(path)));
    let result = (|| -> std::io::Result<()> {
        let mut fh = OpenOptions::new().write(true).create_new(true).open(&tmp)?;
        fh.write_all(data)?;
        fh.sync_all()?;
        drop(fh);
        fs::rename(&tmp, path)
    })();
    if let Err(e) = result {
        let _ = fs::remove_file(&tmp);
        return Err(Fail::new(format!("{}: {e}", path.display())));
    }
    sync_dir(dir)
}

/// fsync a directory, so the entries created or renamed in it are durable. A filesystem that cannot
/// sync directories (EINVAL, unsupported) is accepted; any other failure is an error.
pub fn sync_dir(dir: &Path) -> Res<()> {
    #[cfg(unix)]
    {
        let r = File::open(dir).and_then(|d| d.sync_all());
        if let Err(e) = r
            && !matches!(e.kind(), std::io::ErrorKind::InvalidInput | std::io::ErrorKind::Unsupported)
        {
            return Err(Fail::new(format!("{}: {e}", dir.display())));
        }
    }
    #[cfg(not(unix))]
    let _ = dir;
    Ok(())
}

/// Exclusive lock on `path` (created if missing). Re-entrant within one object.
pub struct FileLock {
    pub path: PathBuf,
    timeout: Duration,
    fh: RefCell<Option<File>>,
    depth: Cell<usize>,
}

pub struct LockGuard<'a>(&'a FileLock);

impl FileLock {
    pub fn new(path: PathBuf, timeout_secs: f64) -> Self {
        FileLock { path, timeout: Duration::from_secs_f64(timeout_secs), fh: RefCell::new(None), depth: Cell::new(0) }
    }

    pub fn lock(&self) -> Res<LockGuard<'_>> {
        if self.depth.get() > 0 {
            self.depth.set(self.depth.get() + 1);
            return Ok(LockGuard(self));
        }
        if let Some(d) = self.path.parent() {
            fs::create_dir_all(d).at(d)?;
        }
        let fh = OpenOptions::new().create(true).append(true).read(true).open(&self.path).at(&self.path)?;
        let deadline = Instant::now() + self.timeout;
        loop {
            match fh.try_lock() {
                Ok(()) => break,
                Err(TryLockError::WouldBlock) => {
                    if Instant::now() > deadline {
                        return Err(Fail::new(format!(
                            "could not lock {} within {:.0} s",
                            self.path.display(),
                            self.timeout.as_secs_f64()
                        )));
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
                Err(TryLockError::Error(e)) => return Err(Fail::new(format!("{}: {e}", self.path.display()))),
            }
        }
        *self.fh.borrow_mut() = Some(fh);
        self.depth.set(1);
        Ok(LockGuard(self))
    }
}

impl Drop for LockGuard<'_> {
    fn drop(&mut self) {
        let l = self.0;
        l.depth.set(l.depth.get() - 1);
        if l.depth.get() == 0
            && let Some(fh) = l.fh.borrow_mut().take()
        {
            let _ = fh.unlock();
        }
    }
}

// ------------------------------------------------------------------------- revision cache
//
// Every revision plied reads or writes is kept by its sha256, so a conflict can show what changed
// since the revision a writer saw. The content is kept for `runtime.revision_days` after it was
// last read or written (its mtime), then pruned. `written.txt` lists the revisions plied itself
// wrote, as hashes only; the commit guard accepts only those, and pruning never touches it.

pub fn remember(runtime: &Path, data: &[u8], written: bool) -> Res<String> {
    let rev = sha(data);
    let p = runtime.join("revs").join(&rev);
    if !p.exists() {
        atomic_write(&p, data)?;
    } else if let Ok(fh) = OpenOptions::new().append(true).open(&p) {
        let _ = fh.set_modified(SystemTime::now()); // seen again: its retention starts over
    }
    if written {
        fs::create_dir_all(runtime).at(runtime)?;
        let w = runtime.join("written.txt");
        let new = !w.exists();
        let mut fh = OpenOptions::new().create(true).append(true).open(&w).at(&w)?;
        fh.write_all(format!("{rev}\n").as_bytes()).at(&w)?;
        fh.sync_all().at(&w)?; // durable before the file it vouches for is replaced
        if new {
            sync_dir(runtime)?;
        }
    }
    Ok(rev)
}

/// Remove cached revision content last read or written more than `days` days ago (all of it when
/// `days` is 0). Returns how many were removed.
pub fn prune_revisions(runtime: &Path, days: i64) -> Res<usize> {
    let d = runtime.join("revs");
    let Ok(rd) = fs::read_dir(&d) else { return Ok(0) };
    let cut = SystemTime::now() - Duration::from_secs(days.max(0) as u64 * 86_400);
    let mut n = 0;
    for e in rd.filter_map(|e| e.ok()) {
        let old = e.metadata().ok().and_then(|m| m.modified().ok()).is_none_or(|t| days == 0 || t < cut);
        if old {
            fs::remove_file(e.path()).at(&e.path())?;
            n += 1;
        }
    }
    Ok(n)
}

/// `prune_revisions`, at most once a day: the time of the last run is the mtime of `pruned`.
pub fn prune_revisions_daily(runtime: &Path, days: i64) -> Res<usize> {
    let stamp = runtime.join("pruned");
    let due = fs::metadata(&stamp).and_then(|m| m.modified()).map_or(true, |t| t.elapsed().map_or(true, |e| e > Duration::from_secs(86_400)));
    if !due {
        return Ok(0);
    }
    let n = prune_revisions(runtime, days)?;
    fs::write(&stamp, b"").at(&stamp)?;
    Ok(n)
}

pub fn recall(runtime: &Path, rev_prefix: &str) -> Option<Vec<u8>> {
    let d = runtime.join("revs");
    if rev_prefix.chars().count() < 8 {
        return None;
    }
    let hits: Vec<PathBuf> = fs::read_dir(&d)
        .ok()?
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().starts_with(rev_prefix))
        .map(|e| e.path())
        .collect();
    if hits.len() == 1 { fs::read(&hits[0]).ok() } else { None }
}

pub fn written(runtime: &Path) -> std::collections::HashSet<String> {
    fs::read_to_string(runtime.join("written.txt"))
        .map(|t| t.split_whitespace().map(String::from).collect())
        .unwrap_or_default()
}

// ------------------------------------------------------------------------- multi-file commits
//
// A write that replaces several files (a record and the view, say) goes through `intent.json` in
// the runtime directory, so a crash leaves a state the next write can finish or undo. The files
// still become visible one rename at a time: this is recoverable, not atomic to readers.
//
//   1. intent `prepare`: the files, their temporary names, and the revisions before and after;
//   2. every temporary file written and fsynced, every new revision added to `written.txt` and
//      fsynced, the temporary files' directories fsynced;
//   3. intent `commit`;
//   4. every temporary file renamed over its target, the targets' directories fsynced; the intent
//      removed.
//
// `recover` runs under the lock before the next write. A `prepare` intent is rolled back (its
// temporary files deleted, targets untouched). A `commit` intent is rolled forward, but only if
// each target still holds its revision from before or already holds the new one; anything else
// (a hand edit since, a lost temporary file, a file that cannot be read) refuses writes until
// someone looks.

const INTENT: &str = "intent.json";

struct Entry {
    path: PathBuf,
    tmp: PathBuf,
    prev: Option<String>,
    sha: String,
}

/// The revision of `p`, None when it does not exist. Any other read failure is an error.
fn current_rev(p: &Path) -> Res<Option<String>> {
    match fs::read(p) {
        Ok(d) => Ok(Some(sha(&d))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(Fail::new(format!("{}: {e}", p.display()))),
    }
}

/// Whether an intent is pending: written by a write in progress, or left by one that was interrupted.
pub fn pending_intent(runtime: &Path) -> Res<bool> {
    let p = runtime.join(INTENT);
    match fs::symlink_metadata(&p) {
        Ok(_) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(Fail::code(crate::error::EXIT_BLOCKED, format!("{}: {e}", p.display()))),
    }
}

fn path_str(p: &Path) -> Res<String> {
    p.to_str().map(String::from).ok_or_else(|| Fail::new(format!("{}: path is not UTF-8", p.display())))
}

fn write_intent(runtime: &Path, state: &str, entries: &[Entry]) -> Res<()> {
    let mut files = vec![];
    for e in entries {
        files.push(serde_json::json!({
            "path": path_str(&e.path)?,
            "tmp": path_str(&e.tmp)?,
            "prev": e.prev,
            "sha": e.sha,
        }));
    }
    let v = serde_json::json!({ "version": 1, "state": state, "files": files });
    atomic_write(&runtime.join(INTENT), (v.to_string() + "\n").as_bytes())
}

fn read_intent(runtime: &Path) -> Res<Option<(String, Vec<Entry>)>> {
    let p = runtime.join(INTENT);
    let raw = match fs::read(&p) {
        Ok(raw) => raw,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(Fail::code(crate::error::EXIT_BLOCKED, format!("writes are refused: {}: {e}", p.display()))),
    };
    let bad = || Fail::code(crate::error::EXIT_BLOCKED, format!("writes are refused: {} is not a plied intent; inspect it, then remove it", p.display()));
    let v: serde_json::Value = serde_json::from_slice(&raw).map_err(|_| bad())?;
    if v["version"] != 1 {
        return Err(bad());
    }
    let state = v["state"].as_str().ok_or_else(bad)?.to_string();
    let mut entries = vec![];
    for f in v["files"].as_array().ok_or_else(bad)? {
        let s = |k: &str| f[k].as_str().map(String::from);
        entries.push(Entry {
            path: PathBuf::from(s("path").ok_or_else(bad)?),
            tmp: PathBuf::from(s("tmp").ok_or_else(bad)?),
            prev: s("prev"),
            sha: s("sha").ok_or_else(bad)?,
        });
    }
    Ok(Some((state, entries)))
}

fn sync_dirs(entries: &[Entry]) -> Res<()> {
    for d in entries.iter().filter_map(|e| e.path.parent()).collect::<std::collections::BTreeSet<_>>() {
        sync_dir(d)?;
    }
    Ok(())
}

/// Replace every file in `files` or, after a crash, none of them until `recover` finishes the job.
/// The caller holds the lock. Each new revision is in `written.txt` before any file is replaced.
pub fn commit_files(runtime: &Path, files: &[(PathBuf, Vec<u8>)]) -> Res<()> {
    if files.is_empty() {
        return Ok(());
    }
    let mut entries = vec![];
    for (path, data) in files {
        if entries.iter().any(|e: &Entry| e.path == *path) {
            return Err(Fail::new(format!("internal: two changes to {}", path.display())));
        }
        let dir = path.parent().unwrap_or(Path::new("."));
        let tag: String = random_bytes::<6>().iter().map(|b| format!("{b:02x}")).collect();
        entries.push(Entry {
            path: path.clone(),
            tmp: dir.join(format!(".{}.{tag}.tmp", file_name(path))),
            prev: current_rev(path)?,
            sha: sha(data),
        });
    }
    fs::create_dir_all(runtime).at(runtime)?;
    write_intent(runtime, "prepare", &entries)?;
    let staged = (|| -> Res<()> {
        for (e, (_, data)) in entries.iter().zip(files) {
            let dir = e.tmp.parent().unwrap_or(Path::new("."));
            fs::create_dir_all(dir).at(dir)?;
            let mut fh = OpenOptions::new().write(true).create_new(true).open(&e.tmp).at(&e.tmp)?;
            fh.write_all(data).at(&e.tmp)?;
            fh.sync_all().at(&e.tmp)?;
            remember(runtime, data, true)?;
        }
        sync_dirs(&entries) // temporary files sit beside their targets
    })();
    if let Err(err) = staged.and_then(|()| write_intent(runtime, "commit", &entries)) {
        for e in &entries {
            let _ = fs::remove_file(&e.tmp);
        }
        let _ = fs::remove_file(runtime.join(INTENT));
        return Err(err);
    }
    let half_done = |err: Fail| {
        Fail::new(format!("{}; the write is half done: `plied recover` (or the next plied write) finishes it", err.msg))
    };
    for e in &entries {
        fs::rename(&e.tmp, &e.path).at(&e.path).map_err(half_done)?;
    }
    sync_dirs(&entries).map_err(half_done)?;
    let intent = runtime.join(INTENT);
    fs::remove_file(&intent).map_err(|err| {
        Fail::new(format!("{}: {err}; every file was replaced, and `plied recover` removes the intent", intent.display()))
    })?;
    Ok(())
}

/// What `recover` did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Recovery {
    /// true when an interrupted write was finished, false when it was undone
    pub finished: bool,
    pub files: usize,
}

impl std::fmt::Display for Recovery {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let verb = if self.finished { "finished" } else { "undid" };
        write!(f, "{verb} an interrupted write of {} file(s)", self.files)
    }
}

/// Finish or undo a write interrupted by a crash (see `commit_files`). The caller holds the lock.
/// Returns what was done, if anything.
pub fn recover(runtime: &Path) -> Res<Option<Recovery>> {
    let Some((state, entries)) = read_intent(runtime)? else { return Ok(None) };
    let intent = runtime.join(INTENT);
    if state == "prepare" {
        for e in &entries {
            let _ = fs::remove_file(&e.tmp);
        }
        fs::remove_file(&intent).at(&intent)?;
        return Ok(Some(Recovery { finished: false, files: entries.len() }));
    }
    if state != "commit" {
        return Err(Fail::code(crate::error::EXIT_BLOCKED, format!("writes are refused: {} has unknown state {state:?}", intent.display())));
    }
    let mut pending = vec![];
    let mut problems = vec![];
    for e in &entries {
        let cur = match current_rev(&e.path) {
            Ok(cur) => cur,
            Err(err) => {
                problems.push(err.msg);
                continue;
            }
        };
        if cur.as_deref() == Some(e.sha.as_str()) {
            continue;
        }
        if cur != e.prev {
            problems.push(format!("{} changed since the interrupted write began", e.path.display()));
        } else if current_rev(&e.tmp).ok().flatten().as_deref() != Some(e.sha.as_str()) {
            problems.push(format!("{} is missing, unreadable or damaged", e.tmp.display()));
        } else {
            pending.push(e);
        }
    }
    if !problems.is_empty() {
        return Err(Fail::code(
            crate::error::EXIT_BLOCKED,
            format!(
                "writes are refused: an interrupted write cannot be finished safely:\n  {}\nInspect {}, put the files right, then remove it.",
                problems.join("\n  "),
                intent.display()
            ),
        ));
    }
    for e in &pending {
        fs::rename(&e.tmp, &e.path).at(&e.path)?;
    }
    sync_dirs(&entries)?;
    fs::remove_file(&intent).at(&intent)?;
    Ok(Some(Recovery { finished: true, files: entries.len() }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An intent as `commit_files` would leave it, with the temporary files written.
    fn interrupted(rt: &Path, state: &str, files: &[(&Path, &[u8])]) -> Vec<Entry> {
        let entries: Vec<Entry> = files
            .iter()
            .map(|(p, data)| Entry {
                path: p.to_path_buf(),
                tmp: p.with_file_name(format!(".{}.test.tmp", file_name(p))),
                prev: current_rev(p).unwrap(),
                sha: sha(data),
            })
            .collect();
        fs::create_dir_all(rt).unwrap();
        write_intent(rt, state, &entries).unwrap();
        for (e, (_, data)) in entries.iter().zip(files) {
            fs::write(&e.tmp, data).unwrap();
        }
        entries
    }

    #[test]
    fn commit_replaces_all_and_lists_revisions_first() {
        let d = tempfile::tempdir().unwrap();
        let (rt, a, b) = (d.path().join("rt"), d.path().join("a.md"), d.path().join("b.md"));
        fs::write(&a, "old a").unwrap();
        commit_files(&rt, &[(a.clone(), b"new a".to_vec()), (b.clone(), b"new b".to_vec())]).unwrap();
        assert_eq!(fs::read(&a).unwrap(), b"new a");
        assert_eq!(fs::read(&b).unwrap(), b"new b");
        assert!(!rt.join(INTENT).exists());
        let w = written(&rt);
        assert!(w.contains(&sha(b"new a")) && w.contains(&sha(b"new b")));
        assert!(commit_files(&rt, &[(a.clone(), b"x".to_vec()), (a, b"y".to_vec())]).is_err());
    }

    #[test]
    fn pruning_goes_by_when_a_revision_was_last_seen() {
        let d = tempfile::tempdir().unwrap();
        let rt = d.path().join("rt");
        let (old, fresh) = (remember(&rt, b"old", true).unwrap(), remember(&rt, b"fresh", false).unwrap());
        let month_ago = SystemTime::now() - Duration::from_secs(30 * 86_400);
        OpenOptions::new().append(true).open(rt.join("revs").join(&old)).unwrap().set_modified(month_ago).unwrap();
        assert_eq!(prune_revisions(&rt, 14).unwrap(), 1);
        assert!(!rt.join("revs").join(&old).exists() && rt.join("revs").join(&fresh).exists());
        assert!(written(&rt).contains(&old), "written.txt is never pruned");
        // seeing a revision again starts its retention over
        OpenOptions::new().append(true).open(rt.join("revs").join(&fresh)).unwrap().set_modified(month_ago).unwrap();
        remember(&rt, b"fresh", false).unwrap();
        assert_eq!(prune_revisions(&rt, 14).unwrap(), 0);
        assert_eq!(prune_revisions(&rt, 0).unwrap(), 1, "0 days keeps nothing");
    }

    #[test]
    fn prepare_is_undone() {
        let d = tempfile::tempdir().unwrap();
        let (rt, a) = (d.path().join("rt"), d.path().join("a.md"));
        fs::write(&a, "old").unwrap();
        let e = interrupted(&rt, "prepare", &[(&a, b"new")]);
        assert_eq!(recover(&rt).unwrap(), Some(Recovery { finished: false, files: 1 }));
        assert_eq!(fs::read(&a).unwrap(), b"old");
        assert!(!e[0].tmp.exists() && !rt.join(INTENT).exists());
        assert_eq!(recover(&rt).unwrap(), None);
    }

    #[test]
    fn commit_is_finished_even_when_half_renamed() {
        let d = tempfile::tempdir().unwrap();
        let (rt, a, b) = (d.path().join("rt"), d.path().join("a.md"), d.path().join("b.md"));
        fs::write(&a, "old a").unwrap();
        fs::write(&b, "old b").unwrap();
        let e = interrupted(&rt, "commit", &[(&a, b"new a"), (&b, b"new b")]);
        fs::rename(&e[0].tmp, &a).unwrap(); // the crash came after the first rename
        assert_eq!(recover(&rt).unwrap(), Some(Recovery { finished: true, files: 2 }));
        assert_eq!(fs::read(&a).unwrap(), b"new a");
        assert_eq!(fs::read(&b).unwrap(), b"new b");
        assert!(!rt.join(INTENT).exists());
    }

    #[test]
    fn commit_over_a_hand_edit_is_refused() {
        let d = tempfile::tempdir().unwrap();
        let (rt, a) = (d.path().join("rt"), d.path().join("a.md"));
        fs::write(&a, "old").unwrap();
        interrupted(&rt, "commit", &[(&a, b"new")]);
        fs::write(&a, "edited by hand").unwrap();
        let err = recover(&rt).unwrap_err();
        assert_eq!(err.code, crate::error::EXIT_BLOCKED);
        assert_eq!(fs::read(&a).unwrap(), b"edited by hand");
        assert!(rt.join(INTENT).exists(), "the intent stays for someone to inspect");
    }
}
