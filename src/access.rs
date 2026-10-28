//! What plied may read: sealed paths, the search allowlist, and the policy hash.
//!
//! Sealed entries are substrings: a path containing any of them is never read, indexed or shown,
//! whether it is reached directly or through a symlink. Record loading, search, literature and the
//! index all go through this policy. The policy hash covers everything that decides access; an
//! index built under a different hash is refused, never served.

use std::path::{Path, PathBuf};

use serde_json::json;

use crate::config::Config;
use crate::error::{Fail, Res};
use crate::fsx::{self, posix, resolve};
use crate::globpat::glob_match;

pub const INDEX_FORMAT: i64 = 1;

pub struct Policy {
    base: PathBuf,
    pub sealed: Vec<String>,
    sealed_bytes: Vec<u8>,
    manifest_bytes: Vec<u8>,
    include: Vec<String>,
    exclude: Vec<String>,
    hard_exclude: Vec<String>,
    literature: serde_json::Value,
}

impl Policy {
    pub fn new(cfg: &Config) -> Res<Policy> {
        let base = resolve(&cfg.base);
        let mut sealed = cfg.sealed.clone();
        let mut sealed_bytes = vec![];
        if let Some(sf) = &cfg.sealed_file {
            let p = base.join(sf);
            if !p.exists() {
                return Err(Fail::policy(format!("access.sealed_file {sf} is configured but missing; refusing to read anything")));
            }
            sealed_bytes = std::fs::read(&p)?;
            for line in String::from_utf8_lossy(&sealed_bytes).lines() {
                let line = line.trim();
                if !line.is_empty() && !line.starts_with('#') {
                    sealed.push(line.to_string());
                }
            }
        }
        sealed.sort();
        sealed.dedup();
        let mut manifest_bytes = vec![];
        if let Some(l) = &cfg.literature {
            manifest_bytes = std::fs::read(base.join(&l.manifest)).unwrap_or_default();
        }
        let literature = match &cfg.literature {
            Some(l) => json!({
                "manifest": l.manifest, "list_key": l.list_key, "file_field": l.file_field, "hash_field": l.hash_field,
                "id_field": l.id_field, "id_from_file": l.id_from_file, "pdf_dir": l.pdf_dir, "text_dir": l.text_dir,
                "page_file": l.page_file, "ocr_marker": l.ocr_marker, "image_command": l.image_command, "image_path": l.image_path,
            }),
            None => serde_json::Value::Null,
        };
        Ok(Policy {
            base,
            sealed,
            sealed_bytes,
            manifest_bytes,
            include: cfg.search_include.clone(),
            exclude: cfg.search_exclude.clone(),
            hard_exclude: cfg.hard_exclude.clone(),
            literature,
        })
    }

    // -------------------------------------------------------------------------- checks

    pub fn is_sealed(&self, p: &Path) -> bool {
        let forms = [p.display().to_string(), posix_form(p), resolve(p).display().to_string()];
        self.sealed.iter().any(|s| forms.iter().any(|f| f.contains(s.as_str())))
    }

    /// The real path if plied may read it, else a policy error.
    pub fn check_readable(&self, path: &Path) -> Res<PathBuf> {
        if self.is_sealed(path) {
            return Err(Fail::policy(format!("{} is sealed", path.display())));
        }
        let real = resolve(path);
        if self.is_sealed(&real) {
            return Err(Fail::policy(format!("{} resolves to a sealed path", path.display())));
        }
        if real.strip_prefix(&self.base).is_err() {
            return Err(Fail::policy(format!("{} resolves outside the repository ({})", path.display(), real.display())));
        }
        Ok(real)
    }

    pub fn readable(&self, path: &Path) -> bool {
        self.check_readable(path).is_ok()
    }

    pub fn searchable(&self, path: &Path) -> bool {
        let Ok(real) = self.check_readable(path) else { return false };
        let relp = posix(real.strip_prefix(&self.base).unwrap_or(&real));
        let given = if path.is_absolute() { path.to_path_buf() } else { std::env::current_dir().unwrap_or_default().join(path) };
        let relg = given.strip_prefix(&self.base).map(posix).unwrap_or_else(|_| relp.clone());
        for r in [&relp, &relg] {
            if glob_match(r, &self.hard_exclude) || glob_match(r, &self.exclude) {
                return false;
            }
        }
        glob_match(&relp, &self.include) && glob_match(&relg, &self.include)
    }

    pub fn literature_readable(&self, path: &Path) -> bool {
        let Ok(real) = self.check_readable(path) else { return false };
        let relp = posix(real.strip_prefix(&self.base).unwrap_or(&real));
        !(glob_match(&relp, &self.hard_exclude) || glob_match(&relp, &self.exclude))
    }

    /// Only the generated page cache may live under .git or an external PLIED_RUNTIME.
    /// Symlinks cannot turn that exception into permission to read another directory.
    pub fn generated_page_readable(&self, path: &Path, runtime: &Path) -> bool {
        let cache = resolve(runtime).join("papers");
        let real = resolve(path);
        if resolve(&cache) != cache || !real.starts_with(&cache) || self.is_sealed(path) || self.is_sealed(&real) {
            return false;
        }
        let given = if path.is_absolute() { path.to_path_buf() } else { std::env::current_dir().unwrap_or_default().join(path) };
        if !given.starts_with(&cache) {
            return false;
        }
        [given, real].iter().all(|p| !glob_match(&fsx::rel(&self.base, p), &self.exclude))
    }

    pub fn digest(&self) -> String {
        let blob = json!({
            "format": INDEX_FORMAT,
            "sealed": self.sealed,
            "sealed_file": fsx::sha(&self.sealed_bytes),
            "manifest": fsx::sha(&self.manifest_bytes),
            "include": self.include,
            "exclude": self.exclude,
            "hard_exclude": self.hard_exclude,
            "literature": self.literature,
            "generated_pages": "contained-runtime-papers-with-source-policy-v1",
        });
        fsx::sha(blob.to_string().as_bytes())
    }
}

fn posix_form(p: &Path) -> String {
    let s = p.display().to_string();
    if cfg!(windows) { s.replace('\\', "/") } else { s }
}
