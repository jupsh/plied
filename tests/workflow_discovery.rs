//! §11 conformance: orientation is bounded and confers no record authority.
mod common;
use common::*;
use serde_json::Value;
use std::process::Command;

fn result(p: &Proj, args: &[&str]) -> Value {
    serde_json::from_str::<Value>(&p.ok(args)).unwrap()["result"].clone()
}

#[test]
fn context_reports_unverified_runtime_access_without_probing_or_creating_it() {
    let p = Proj::new();
    let runtime = p.path().join("never-created-runtime");
    let out = Command::new(bin()).arg("-C").arg(p.path()).args(["context", "--json"]).env("PLIED_RUNTIME", &runtime).output().unwrap();
    assert!(out.status.success(), "{}", text(&out));
    let context: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(context["result"]["runtime_access"]["writability"], "unverified");
    assert!(context["result"]["write_blockers"].as_array().unwrap().is_empty());
    assert!(!runtime.exists(), "ordinary discovery must not probe with a write");
    let empty = tempfile::tempdir().unwrap();
    let out = Command::new(bin()).current_dir(empty.path()).args(["context", "--json"]).env_remove("PLIED_CANONICAL").output().unwrap();
    assert!(out.status.success());
    let unconfigured: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(unconfigured["result"]["configured"], false);
    assert!(unconfigured["result"]["runtime_access"].is_null());
}

#[test]
fn compact_show_preserves_full_json_and_accepts_preexisting_store_bytes() {
    let p = Proj::new();
    p.with_evidence();
    p.evidence("E001", "", "Before acting, read the evidence.");
    std::fs::create_dir_all(p.path().join("plied/threads")).unwrap();
    // Pre-0.11 format-1 record: no new stored field is necessary for either orientation flag.
    let path = p.path().join("plied/threads/T001.md");
    let history = (1..=20).map(|n| format!("- 2020-01-{n:02} 00:00 UTC alice: entry {n:02}\n")).collect::<String>();
    let body = format!("---\nid: T001\ntitle: Established task\nstatus: open\n---\n## Summary\n{}\n\n## Details\nFULL_EVIDENCE_REQUIRED\n\n## History\n{history}", "Author synopsis. ".repeat(100));
    std::fs::write(&path, body).unwrap();
    p.ok(&["new", "commitment", "--title", "Read original", "made=E001", "nodes=T001", "wording=Before acting, read the evidence."]);
    let before = std::fs::read(&path).unwrap();
    assert_eq!(result(&p, &["check", "--json"])["errors"], 0);
    let full = result(&p, &["show", "T001", "--json"]);
    let compact = result(&p, &["show", "T001", "--summary", "--freshness", "--json"]);
    for (field, value) in full.as_object().unwrap() { assert_eq!(&compact[field], value, "full field {field} was lost"); }
    assert_eq!(compact["summary_view"]["omitted"], true);
    assert_eq!(compact["summary_view"]["show_args"], serde_json::json!(["show", "T001"]));
    let preview = compact["summary_view"]["text"].as_str().unwrap();
    assert!(preview.contains("author-written"));
    assert!(preview.contains("entry 20"));
    assert!(!preview.contains("entry 01"));
    assert!(!preview.contains("FULL_EVIDENCE_REQUIRED"));
    assert!(preview.contains("wording omitted"));
    assert!(preview.contains("plied review T001"));
    assert!(preview.chars().count() < 3000);
    assert!(full["text"].as_str().unwrap().contains("FULL_EVIDENCE_REQUIRED"));
    assert!(full.get("summary_view").is_none());
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert_eq!(result(&p, &["check", "--json"])["errors"], 0);
}

#[test]
fn compact_fallback_help_and_participant_guidance_are_discoverable() {
    let p = Proj::new();
    p.ok(&["new", "thread", "--title", "No body"]);
    assert!(p.ok(&["show", "T001", "--summary"]).contains("No author-written summary or body preview"));
    p.with_evidence();
    p.evidence("E001", "", &format!("{}\n\nLATER_DETAIL", "First paragraph. ".repeat(100)));
    let preview = p.ok(&["show", "E001", "--summary"]);
    assert!(preview.contains("no author-written summary"));
    assert!(!preview.contains("LATER_DETAIL"));
    assert!(preview.contains("plied show E001"));
    let help = p.ok(&["show", "--help"]);
    assert!(help.contains("configured [[sources]]"));
    assert!(help.contains("plied show D123"));
    let skill = p.ok(&["skill"]);
    assert!(skill.contains("Participants with limited record authority"));
    assert!(skill.contains("confer no approval or record-edit authority"));
    assert!(skill.contains("release --this-session"));
    assert!(skill.contains("--baseline"));
    assert!(skill.contains("--read ID@FULL_SHA"));
}
