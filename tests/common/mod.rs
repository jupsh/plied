//! The harness shared by the end-to-end tests: a throwaway git repository with plied initialised.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

pub fn bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_plied"))
}

pub struct Proj {
    pub dir: tempfile::TempDir,
}

impl Proj {
    pub fn new() -> Proj {
        let dir = tempfile::tempdir().unwrap();
        let p = Proj { dir };
        p.git(&["init", "-q", "-b", "main", "."]);
        p.git(&["config", "user.email", "t@example.com"]);
        p.git(&["config", "user.name", "t"]);
        let out = p.run(&["init"]);
        assert!(out.status.success(), "init: {}", text(&out));
        p
    }

    pub fn path(&self) -> &Path {
        self.dir.path()
    }

    pub fn git(&self, args: &[&str]) {
        let out = Command::new("git").arg("-C").arg(self.path()).args(args).output().unwrap();
        assert!(out.status.success(), "git {args:?}: {}", text(&out));
    }

    pub fn run(&self, args: &[&str]) -> Output {
        self.run_as("alice", args)
    }

    pub fn run_as(&self, agent: &str, args: &[&str]) -> Output {
        Command::new(bin())
            .arg("-C")
            .arg(self.path())
            .args(args)
            .env("PLIED_AGENT", agent)
            .env("PLIED_SESSION", "sess12345678")
            .output()
            .unwrap()
    }

    pub fn ok(&self, args: &[&str]) -> String {
        let out = self.run(args);
        assert!(out.status.success(), "{args:?}: {}", text(&out));
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    pub fn rev(&self, id: &str) -> String {
        let out = self.ok(&["show", id, "--json"]);
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        v["result"]["rev"].as_str().unwrap().to_string()
    }
}

pub fn text(out: &Output) -> String {
    format!("{}\n{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr))
}


pub fn sha(data: &[u8]) -> String {
    use sha2::Digest;
    sha2::Sha256::digest(data).iter().map(|b| format!("{b:02x}")).collect()
}


impl Proj {
    /// Replace `from` with `to` in a file by hand, as an editor or another tool would.
    pub fn edit(&self, rel: &str, from: &str, to: &str) {
        let f = self.path().join(rel);
        let t = std::fs::read_to_string(&f).unwrap();
        assert!(t.contains(from), "{rel} has no {from:?}:\n{t}");
        std::fs::write(&f, t.replacen(from, to, 1)).unwrap();
    }

    pub fn append_config(&self, extra: &str) {
        let p = self.path().join("plied.toml");
        let t = std::fs::read_to_string(&p).unwrap();
        std::fs::write(p, t + "\n" + extra).unwrap();
    }

    /// Link `evidence/*.md` as decisions and observations with authority and scope fields.
    pub fn with_evidence(&self) -> &Self {
        self.append_config(
            "[[sources]]\nname = \"evidence\"\nglob = \"evidence/*.md\"\nid = \"^[ED][0-9]{3}$\"\nauthority = \"authority\"\nscope = \"scope\"\n",
        );
        std::fs::create_dir_all(self.path().join("evidence")).unwrap();
        self
    }

    /// A linked evidence record; `meta` is extra frontmatter lines.
    pub fn evidence(&self, id: &str, meta: &str, body: &str) {
        std::fs::write(self.path().join(format!("evidence/{id}.md")), format!("---\nid: {id}\ntitle: {id}\n{meta}---\n{body}\n")).unwrap();
    }

    pub fn code(&self, args: &[&str]) -> Option<i32> {
        self.run(args).status.code()
    }

    /// `plied set ID fields… --expect <current rev>`
    pub fn set(&self, id: &str, fields: &[&str]) -> std::process::Output {
        let rev = self.rev(id);
        let mut args = vec!["set", id];
        args.extend(fields);
        args.extend(["--expect", &rev]);
        self.run(&args)
    }
}
