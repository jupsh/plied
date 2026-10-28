//! Keeping record files out of code branches (FORMAT.md §1, §6).
//! Managed paths change only through plied, in the canonical checkout.

use std::collections::HashSet;
use std::path::PathBuf;
use std::process::Command;

use crate::config::Config;
use crate::fsx;
use crate::repo::Repo;

pub const MARK: &str = "# installed by plied";
pub const EMPTY_TREE: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";

fn git_text(cwd: &std::path::Path, args: &[&str]) -> Result<String, String> {
    git_bytes(cwd, args).map(|b| String::from_utf8_lossy(&b).into_owned())
}

fn git_bytes(cwd: &std::path::Path, args: &[&str]) -> Result<Vec<u8>, String> {
    let out = Command::new("git").arg("-C").arg(cwd).args(args).output().map_err(|e| format!("git {}: {e}", args.join(" ")))?;
    if out.status.success() {
        Ok(out.stdout)
    } else {
        Err(format!("git {}: {}", args.join(" "), String::from_utf8_lossy(&out.stderr).trim()))
    }
}

pub fn managed(cfg: &Config, path: &str) -> bool {
    cfg.managed_paths().iter().any(|m| path == m || (m.ends_with('/') && path.starts_with(m.as_str())))
}

pub fn check_staged(repo: &Repo, written: &HashSet<String>, merging: bool) -> Result<Vec<String>, String> {
    let against = if git_text(&repo.here, &["rev-parse", "-q", "--verify", "HEAD"]).is_ok() {
        "HEAD".to_string()
    } else {
        EMPTY_TREE.to_string()
    };
    let out = git_bytes(&repo.here, &["diff", "--cached", "--name-status", "-z", "--no-renames", &against])?;
    let fields = nul_fields(&out)?;
    let (pairs, remainder) = fields.as_chunks::<2>();
    if !remainder.is_empty() {
        return Err("git diff returned an incomplete status/path pair".into());
    }
    let mut problems = vec![];
    for pair in pairs {
        let (status, path) = (pair[0], pair[1]);
        if !managed(&repo.cfg, path) {
            continue;
        }
        if !repo.in_canonical() && !merging {
            problems.push(format!(
                "{path}: record files change only through plied in the canonical checkout ({}), not in this worktree",
                repo.canonical.display()
            ));
            continue;
        }
        if status.starts_with('D') {
            problems.push(format!("{path}: plied never deletes records (supersede or close them instead)"));
            continue;
        }
        let blob = git_bytes(&repo.here, &["show", &format!(":{path}")])?;
        if !written.contains(&fsx::sha(&blob)) {
            let what = if merging { "the merge brings in" } else { "the staged content is" };
            problems.push(format!("{path}: {what} content plied did not write (a hand edit, or a code branch's change?)"));
        }
    }
    Ok(problems)
}

pub fn check_range(repo: &Repo, base: &str, head: &str) -> Result<Vec<String>, String> {
    let mb = git_text(&repo.here, &["merge-base", base, head])?.trim().to_string();
    let out = git_bytes(&repo.here, &["diff", "--name-only", "-z", "--no-renames", &mb, head])?;
    Ok(nul_fields(&out)?
        .into_iter()
        .filter(|p| managed(&repo.cfg, p))
        .map(|p| format!("{p}: changed on {head} since {}; code branches must not change record files", &mb[..mb.len().min(10)]))
        .collect())
}

/// `-z` disables Git's quoting, including octal escapes and embedded tabs/newlines.
/// An undecodable filename fails the guard rather than silently skipping a managed path.
fn nul_fields(bytes: &[u8]) -> Result<Vec<&str>, String> {
    bytes.split(|b| *b == 0).filter(|b| !b.is_empty())
        .map(|b| std::str::from_utf8(b).map_err(|_| "git diff returned a non-UTF-8 filename; cannot validate managed paths".into()))
        .collect()
}

pub fn check_merge(repo: &Repo, written: &HashSet<String>) -> Result<Vec<String>, String> {
    check_staged(repo, written, true)
}

pub fn install(repo: &Repo, force: bool) -> Result<Vec<String>, String> {
    let mut hooks_dir: Option<PathBuf> = None;
    if let Ok(hp) = git_text(&repo.here, &["config", "--get", "core.hooksPath"]) {
        let hp = hp.trim();
        if !hp.is_empty() {
            let p = PathBuf::from(hp);
            hooks_dir = Some(if p.is_absolute() { p } else { repo.canonical.join(p) });
        }
    }
    let hooks_dir = match hooks_dir {
        Some(d) => d,
        None => repo.common_dir.clone().ok_or("not a git repository")?.join("hooks"),
    };
    std::fs::create_dir_all(&hooks_dir).map_err(|e| e.to_string())?;
    let mut done = vec![];
    for (name, sub) in [("pre-commit", "pre-commit"), ("pre-merge-commit", "pre-merge-commit")] {
        let p = hooks_dir.join(name);
        if p.exists()
            && !std::fs::read_to_string(&p).unwrap_or_default().contains(MARK)
            && !force
        {
            done.push(format!("{}: exists and is not plied's; add `plied guard {sub}` to it yourself (or --force)", p.display()));
            continue;
        }
        std::fs::write(&p, format!("#!/bin/sh\n{MARK}\nexec plied guard {sub}\n")).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perm = std::fs::metadata(&p).map_err(|e| e.to_string())?.permissions();
            perm.set_mode(perm.mode() | 0o111);
            std::fs::set_permissions(&p, perm).map_err(|e| e.to_string())?;
        }
        done.push(format!("{}: installed", p.display()));
    }
    Ok(done)
}
