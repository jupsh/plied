//! Finding the canonical record store (FORMAT.md §1).
//!
//! Every worktree shares one git common directory; the main worktree holds the
//! record. Outside git, the canonical checkout is the nearest directory upwards
//! containing `plied.toml`, and the runtime directory is `.plied-runtime/` in it.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::rc::Rc;

use crate::config::{self, Config};
use crate::error::{Fail, Res};
use crate::fsx;

pub struct Repo {
    pub canonical: PathBuf,
    pub here: PathBuf,
    pub common_dir: Option<PathBuf>,
    pub runtime: PathBuf,
    pub cfg: Rc<Config>,
}

pub fn git(cwd: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git").arg("-C").arg(cwd).args(args).output().ok()?;
    if out.status.success() { Some(String::from_utf8_lossy(&out.stdout).trim().to_string()) } else { None }
}

fn runtime(default: PathBuf) -> PathBuf {
    match std::env::var_os("PLIED_RUNTIME") {
        Some(v) => fsx::resolve(&PathBuf::from(v)),
        None => default,
    }
}

impl Repo {
    pub fn in_canonical(&self) -> bool {
        fsx::resolve(&self.here) == fsx::resolve(&self.canonical)
    }

    pub fn rel(&self, p: &Path) -> String {
        fsx::rel(&self.canonical, p)
    }

    pub fn branch(&self) -> Option<String> {
        self.common_dir.as_ref()?;
        git(&self.canonical, &["symbolic-ref", "-q", "--short", "HEAD"])
    }

    /// Reasons the canonical checkout must not be written now (empty = ok). §11.
    pub fn write_blockers(&self) -> Vec<String> {
        let Some(_common) = &self.common_dir else { return vec![] };
        let mut out = vec![];
        let gd = git(&self.canonical, &["rev-parse", "--absolute-git-dir"]).map(PathBuf::from).unwrap_or_else(|| _common.clone());
        for (marker, what) in [
            ("MERGE_HEAD", "a merge"),
            ("CHERRY_PICK_HEAD", "a cherry-pick"),
            ("REVERT_HEAD", "a revert"),
            ("rebase-merge", "a rebase"),
            ("rebase-apply", "a rebase or am"),
            ("BISECT_LOG", "a bisect"),
        ] {
            if gd.join(marker).exists() {
                out.push(format!("the canonical checkout is in the middle of {what}"));
            }
        }
        match self.branch() {
            None => out.push("the canonical checkout has a detached HEAD".into()),
            Some(br) => {
                if let Some(want) = &self.cfg.canonical_branch
                    && br != *want
                {
                    out.push(format!("the canonical checkout is on branch {br:?}, not the configured {want:?}"));
                }
            }
        }
        out
    }
}

pub fn locate(start: Option<&Path>, config_path: Option<&Path>) -> Res<Repo> {
    let cwd = std::env::current_dir().map_err(|e| Fail::new(e.to_string()))?;
    let start = fsx::resolve(start.unwrap_or(&cwd));
    let config_path: Option<PathBuf> = config_path.map(|p| fsx::resolve(&start.join(p)));
    let top = git(&start, &["rev-parse", "--show-toplevel"]);
    let Some(top) = top else {
        // outside git: nearest directory upwards holding plied.toml
        if config_path.is_none() {
            let base = find_upwards(&start);
            return match base {
                Some(base) => {
                    let cfgp = base.join(config::CONFIG_NAME);
                    let cfg = Rc::new(config::load(&cfgp, Some(&base))?);
                    Ok(Repo { canonical: base.clone(), here: base.clone(), common_dir: None, runtime: runtime(base.join(".plied-runtime")), cfg })
                }
                None => Err(Fail::new(format!("no {} found above {} (and not a git repository)", config::CONFIG_NAME, start.display()))),
            };
        }
        let cp = config_path.clone().unwrap();
        let canonical = cp.parent().map(Path::to_path_buf).ok_or_else(|| Fail::new("bad --config path"))?;
        let cfg = Rc::new(config::load(&cp, Some(&canonical))?);
        return Ok(Repo { canonical: canonical.clone(), here: canonical.clone(), common_dir: None, runtime: runtime(canonical.join(".plied-runtime")), cfg });
    };
    let here = PathBuf::from(&top);
    let common = git(&start, &["rev-parse", "--path-format=absolute", "--git-common-dir"])
        .ok_or_else(|| Fail::new("could not read the git common directory"))?;
    let common_dir = PathBuf::from(&common);
    let canonical = canonical_worktree(&start, &common_dir)?;
    let cfg_file = config_path.clone().unwrap_or_else(|| canonical.join(config::CONFIG_NAME));
    if !cfg_file.exists() {
        return Err(Fail::new(format!("no {} in the canonical checkout {} (run `plied init` there)", config::CONFIG_NAME, canonical.display())));
    }
    let cfg = Rc::new(config::load(&cfg_file, Some(&canonical))?);
    Ok(Repo { canonical, here, runtime: runtime(common_dir.join("plied")), common_dir: Some(common_dir), cfg })
}

/// `locate` without running git, for hooks (§6 sessions). None if unsure.
pub fn locate_fast(start: &Path) -> Option<Repo> {
    let start = fsx::resolve(start);
    let mut dir = Some(start.as_path());
    let (common, here);
    loop {
        let d = dir?;
        let g = d.join(".git");
        if g.is_dir() {
            common = g;
            here = d.to_path_buf();
            break;
        }
        if g.is_file() {
            let text = std::fs::read_to_string(&g).ok()?;
            let text = text.trim();
            let rest = text.strip_prefix("gitdir:")?.trim();
            let gitdir = fsx::resolve(&d.join(rest));
            let cd = gitdir.join("commondir");
            let cdtext = std::fs::read_to_string(&cd).ok()?;
            common = fsx::resolve(&gitdir.join(cdtext.trim()));
            here = d.to_path_buf();
            break;
        }
        dir = d.parent();
    }
    if common.file_name().map(|n| n != ".git").unwrap_or(true) || std::env::var_os("PLIED_CANONICAL").is_some() {
        return None;
    }
    let canonical = common.parent()?.to_path_buf();
    let cfg_file = canonical.join(config::CONFIG_NAME);
    if !cfg_file.exists() {
        return None;
    }
    let cfg = Rc::new(config::load(&cfg_file, Some(&canonical)).ok()?);
    Some(Repo { canonical, here, runtime: runtime(common.join("plied")), common_dir: Some(common), cfg })
}

/// Find the initialization destination without requiring a configuration to exist yet.
pub fn init_base(start: &Path) -> Res<PathBuf> {
    let start = fsx::resolve(start);
    if git(&start, &["rev-parse", "--show-toplevel"]).is_none() {
        return Ok(start);
    }
    let common = git(&start, &["rev-parse", "--path-format=absolute", "--git-common-dir"])
        .ok_or_else(|| Fail::new("could not read the git common directory"))?;
    canonical_worktree(&start, Path::new(&common))
}

fn canonical_worktree(start: &Path, common_dir: &Path) -> Res<PathBuf> {
    if let Ok(v) = std::env::var("PLIED_CANONICAL")
        && !v.is_empty()
    {
        return Ok(fsx::resolve(&PathBuf::from(v)));
    }
    main_worktree(start, common_dir)
}

fn main_worktree(start: &Path, common_dir: &Path) -> Res<PathBuf> {
    if common_dir.file_name().map(|n| n == ".git").unwrap_or(false) {
        return common_dir.parent().map(fsx::resolve).ok_or_else(|| Fail::new("could not find the main worktree"));
    }
    let listing = git(start, &["worktree", "list", "--porcelain"]).unwrap_or_default();
    for line in listing.lines() {
        if let Some(p) = line.strip_prefix("worktree ") {
            return Ok(fsx::resolve(&PathBuf::from(p)));
        }
    }
    Err(Fail::new("could not find the main worktree (bare repositories need PLIED_CANONICAL)"))
}

fn find_upwards(start: &Path) -> Option<PathBuf> {
    let mut d = Some(start);
    while let Some(p) = d {
        if p.join(config::CONFIG_NAME).exists() {
            return Some(p.to_path_buf());
        }
        d = p.parent();
    }
    None
}
