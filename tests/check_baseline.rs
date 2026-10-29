//! §8/§11 conformance: comparisons describe changes without changing check's verdict.
mod common;

use common::*;
use serde_json::Value;

fn report(p: &Proj, args: &[&str]) -> Value {
    let out = p.run(args);
    assert!(out.stderr.is_empty(), "{}", text(&out));
    serde_json::from_slice(&out.stdout).unwrap()
}

fn project() -> Proj {
    let p = Proj::new();
    p.append_config("[[sources]]\nname = \"log\"\nglob = \"log/*.md\"\nid = \"^D[0-9]{3}$\"\ndate = \"date\"\n");
    std::fs::create_dir(p.path().join("log")).unwrap();
    p.ok(&["new", "thread", "--title", "topic"]);
    std::fs::write(p.path().join("log/D001.md"), "---\nid: D001\ntitle: first\ndate: 2999-01-01\n---\nT001 changes.\n").unwrap();
    p
}

fn save(p: &Proj, value: &Value) -> String {
    let path = p.path().join("baseline.json");
    std::fs::write(&path, serde_json::to_vec(value).unwrap()).unwrap();
    path.to_string_lossy().into_owned()
}

#[test]
fn comparisons_distinguish_added_sources_and_changed_bytes_without_acknowledging_them() {
    let p = project();
    let before = report(&p, &["check", "--json"]);
    let path = save(&p, &before);
    let rev = p.rev("T001");
    std::fs::write(p.path().join("log/D002.md"), "---\nid: D002\ntitle: second\ndate: 2999-01-02\n---\nT001 changes again.\n").unwrap();
    let after = report(&p, &["check", "--baseline", &path, "--json"]);
    assert_eq!(after["exit"], 0);
    assert_eq!(after["result"]["comparison"]["added"].as_array().unwrap().len(), 1);
    assert_eq!(after["result"]["comparison"]["persisting"].as_array().unwrap().len(), 1);
    assert_eq!(after["result"]["comparison"]["added"][0]["source"]["record"], "D002");
    // The same aggregated warning still exists, and comparison performed no record write.
    assert_eq!(after["result"]["warnings"], before["result"]["warnings"]);
    assert_eq!(p.rev("T001"), rev);
    p.edit("log/D001.md", "changes.", "changed bytes with the same date.");
    let changed = report(&p, &["check", "--baseline", &path, "--json"]);
    let c = &changed["result"]["comparison"];
    assert_eq!(c["added"].as_array().unwrap().len(), 2);
    assert_eq!(c["resolved"].as_array().unwrap().len(), 1);
    assert_eq!(c["resolved"][0]["source"]["record"], "D001");
    assert_ne!(c["resolved"][0]["source"]["rev"], c["added"][0]["source"]["rev"]);
    assert_eq!(p.rev("T001"), rev);
}

#[test]
fn comparison_never_hides_current_errors_and_ignores_message_wording() {
    let p = project();
    let mut before = report(&p, &["check", "--json"]);
    before["result"]["diagnostics"][0]["message"] = Value::String("different wording".into());
    let path = save(&p, &before);
    let same = report(&p, &["check", "--baseline", &path, "--json"]);
    assert!(same["result"]["comparison"]["added"].as_array().unwrap().is_empty());
    assert_eq!(same["result"]["comparison"]["persisting"].as_array().unwrap().len(), 1);
    p.edit("plied/threads/T001-topic.md", "status: open", "status: broken");
    let invalid = report(&p, &["check", "--baseline", &path, "--json"]);
    assert_eq!(invalid["exit"], 1);
    assert!(invalid["result"]["errors"].as_u64().unwrap() > 0);
    assert!(invalid["result"]["comparison"]["added"].as_array().unwrap().iter().any(|e| e["level"] == "error"));
}

#[test]
fn comparison_refuses_unsupported_incomplete_or_differently_scoped_snapshots() {
    let p = project();
    let before = report(&p, &["check", "--json"]);
    let path = save(&p, &before);
    assert_eq!(report(&p, &["check", "--baseline", &path, "--errors-only", "--json"])["exit"], 2);
    let mut old = before.clone();
    old["result"].as_object_mut().unwrap().remove("comparison_basis");
    save(&p, &old);
    assert_eq!(report(&p, &["check", "--baseline", &path, "--json"])["exit"], 2);
    save(&p, &before);
    // A policy change cannot echo identities or source hashes saved before a seal.
    p.edit("plied.toml", "sealed = []", "sealed = [\"D001.md\"]");
    let sealed = report(&p, &["check", "--baseline", &path, "--json"]);
    assert_eq!(sealed["exit"], 2);
    assert!(!sealed.to_string().contains("D001"));
}

#[test]
fn moving_evidence_behind_an_existing_seal_cannot_echo_saved_hashes() {
    let p = project();
    p.edit("plied.toml", "sealed = []", "sealed = [\"private/\"]");
    p.edit("plied.toml", "log/*.md", "log/**/*.md");
    let before = report(&p, &["check", "--json"]);
    let path = save(&p, &before);
    let old_hash = before["result"]["comparison_basis"]["entries"][0]["source"]["rev"].as_str().unwrap();
    std::fs::create_dir(p.path().join("log/private")).unwrap();
    std::fs::rename(p.path().join("log/D001.md"), p.path().join("log/private/D001.md")).unwrap();
    let hidden = report(&p, &["check", "--baseline", &path, "--json"]);
    assert_eq!(hidden["exit"], 2);
    assert!(!hidden.to_string().contains("D001"));
    assert!(!hidden.to_string().contains(old_hash));
}
