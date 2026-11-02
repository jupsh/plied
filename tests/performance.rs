//! Repeatable end-to-end benchmark, without timing assertions in CI:
//! `cargo test --release --test performance -- --ignored --nocapture`
//! Set PLIED_BENCH_BIN to compare a saved binary against the same fixture sizes.
//! Filter by `repeated_work` to benchmark review/show/claim/find without the staleness case.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

fn dated_store(count: usize) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::create_dir_all(root.join("plied/threads")).unwrap();
    std::fs::create_dir_all(root.join("log")).unwrap();
    std::fs::write(
        root.join("plied.toml"),
        "[plied]\nformat = 1\nroot = \"plied\"\n\n[[sources]]\nname = \"log\"\nglob = \"log/*.md\"\nid = \"^D[0-9]{4}$\"\ndate = \"date\"\n",
    ).unwrap();
    let filler = "The recorded experiment describes the current method and the next steps. ".repeat(16);
    for i in 1..=count {
        std::fs::write(
            root.join(format!("plied/threads/T{i:03}.md")),
            format!("---\nid: T{i:03}\ntitle: Thread {i}\nstatus: open\n---\n## History\n- 2000-01-01 00:00 UTC — Created.\n"),
        ).unwrap();
        std::fs::write(
            root.join(format!("log/D{i:04}.md")),
            format!("---\nid: D{i:04}\ntitle: Log {i}\ndate: 2999-01-01\n---\nT{i:03} changes. {filler}\n"),
        ).unwrap();
    }
    dir
}

fn check(binary: &Path, root: &Path) -> Duration {
    let (elapsed, v) = run(binary, root, &["check", "--json"]);
    assert_eq!(v["result"]["errors"], 0);
    assert_eq!(v["result"]["warnings"], v["result"]["records"]);
    elapsed
}

fn run(binary: &Path, root: &Path, args: &[&str]) -> (Duration, serde_json::Value) {
    let start = Instant::now();
    let out = Command::new(binary)
        .arg("-C").arg(root).args(args)
        .env("PLIED_AGENT", "benchmark").env("PLIED_SESSION", "benchmark-session")
        .output().unwrap();
    let elapsed = start.elapsed();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    (elapsed, v)
}

fn binary() -> PathBuf {
    std::env::var_os("PLIED_BENCH_BIN").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(env!("CARGO_BIN_EXE_plied")))
}

#[test]
#[ignore = "performance benchmark; use --release --ignored --nocapture"]
fn check_with_dated_linked_records() {
    let binary = binary();
    for count in [100, 1000] {
        let dir = dated_store(count);
        check(&binary, dir.path()); // warm up
        let mut samples: Vec<_> = (0..3).map(|_| check(&binary, dir.path())).collect();
        samples.sort_unstable();
        plied::outln!("{count} nodes + {count} dated linked records: median {:.3}s ({samples:?})", samples[1].as_secs_f64());
    }
}

fn dependency_store(count: usize) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    for name in ["plied/threads", "plied/commitments", "evidence"] {
        std::fs::create_dir_all(root.join(name)).unwrap();
    }
    std::fs::write(
        root.join("plied.toml"),
        "[plied]\nformat = 1\nroot = \"plied\"\n[[sources]]\nname = \"evidence\"\nglob = \"evidence/*.md\"\nid = \"^E[0-9]{3}$\"\n",
    ).unwrap();
    std::fs::write(root.join("evidence/E001.md"), "---\nid: E001\ntitle: Evidence\n---\nReview before every run.\n").unwrap();
    for i in 1..=count {
        let mut extra = if i == 1 {
            format!("uses: [{}]\n", (2..=count).map(|j| format!("T{j:03}")).collect::<Vec<_>>().join(", "))
        } else {
            String::new()
        };
        if i < 10 {
            extra.push_str(&format!("parent: T{:03}\n", i + 1));
        }
        std::fs::write(
            root.join(format!("plied/threads/T{i:03}.md")),
            format!("---\nid: T{i:03}\ntitle: Thread {i}\nstatus: open\n{extra}---\n"),
        ).unwrap();
        let scope = if i % 2 == 1 { "subtree" } else { "linked" };
        std::fs::write(
            root.join(format!("plied/commitments/P{i:02}.md")),
            format!("---\nid: P{i:02}\ntitle: Commitment {i}\nstate: open\nwording: Review before every run.\nmade: E001\napplies: {scope}\nnodes: [T{count:03}, T010]\n---\n"),
        ).unwrap();
    }
    dir
}

#[test]
#[ignore = "performance benchmark; use --release --ignored --nocapture"]
fn commands_with_repeated_work() {
    let binary = binary();
    let count = 1000;
    let dir = dependency_store(count);
    let query = r#"{"title":{"$regex":"\\bthread\\s+[0-9]+\\b","$options":"i"}}"#;
    for (command, target) in [("review", "T001"), ("show", "T001"), ("claim", "T001"), ("find", query)] {
        let mut samples = vec![];
        for i in 0..4 {
            let (elapsed, v) = run(&binary, dir.path(), &[command, target, "--json"]);
            let items = if command == "find" { &v["result"] } else { &v["result"]["commitments"] };
            assert_eq!(items.as_array().unwrap().len(), count);
            if i > 0 {
                samples.push(elapsed);
            }
        }
        samples.sort_unstable();
        plied::outln!("{command}: {count} nodes + {count} commitments: median {:.3}s ({samples:?})", samples[1].as_secs_f64());
    }
}
