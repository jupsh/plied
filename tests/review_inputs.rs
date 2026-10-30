//! FORMAT §§3.3, 8, 11 and 12: explicit evidence revisions never alter the action gate.
mod common;

use std::process::{Command, Output, Stdio};
use std::time::Duration;
use serde_json::Value;
use common::{Proj, bin, text};

fn fixture() -> Proj {
    let p = Proj::new();
    p.with_evidence();
    p.evidence("E001", "", "Before a run, check the evidence.");
    p.evidence("E002", "", "A known positive was detected.");
    p.evidence("E003", "", "The dose was delivered.");
    p.ok(&["new", "thread", "--title", "Run"]);
    p.ok(&["new", "commitment", "--title", "Check evidence", "made=E001", "nodes=T001", "wording=Before a run, check the evidence."]);
    p
}

fn result(out: &Output) -> Value {
    serde_json::from_slice::<Value>(&out.stdout).unwrap_or_else(|e| panic!("{e}: {}", text(out)))["result"].clone()
}

fn dispose(p: &Proj, action: &str, inputs: &[String]) -> Output {
    let rev = p.rev("P01");
    let mut args = vec!["dispose", "P01", "--action", action, "--disposition", "satisfied", "--because", "Inputs reviewed", "--expect", &rev, "--json"];
    for input in inputs { args.extend(["--read", input]); }
    p.run(&args)
}

fn warnings(p: &Proj) -> Vec<Value> {
    result(&p.run(&["check", "--json"]))["diagnostics"].as_array().unwrap().iter().filter(|d| d["code"] == "review-input").cloned().collect()
}

fn gate(p: &Proj, action: &str) -> Value {
    let out = p.run(&["review", "T001", "--action", action, "--gate", "--json"]);
    assert!(out.status.success(), "{}", text(&out));
    result(&out)
}

#[test]
fn an_old_unpinned_store_remains_valid_and_explicitly_unbound() {
    let p = fixture();
    // The pre-0.11 representation has no inputs. Keep a handwritten old record in this fixture.
    let path = p.path().join("plied/commitments/P01-check-evidence.md");
    let old = std::fs::read_to_string(&path).unwrap().replacen("\n---\n", "\nreviews:\n  - action: run:old\n    disposition: satisfied\n    ref: E002\n---\n", 1);
    std::fs::write(path, old).unwrap();
    p.ok(&["view"]);
    let checked = p.run(&["check", "--json"]);
    assert!(checked.status.success(), "{}", text(&checked));
    assert_eq!(result(&checked)["warnings"], 0);
    let review = gate(&p, "run:old");
    assert_eq!(review["commitments"][0]["input_reviews"][0]["status"], "unbound");
    assert!(p.ok(&["review", "T001", "--action", "run:old"]).contains("unbound"));
}

#[test]
fn changed_inputs_warn_until_that_action_is_reviewed_again() {
    let p = fixture();
    let pins = [format!("E002@{}", p.rev("E002")), format!("E003@{}", p.rev("E003"))];
    assert!(dispose(&p, "run:first", &pins).status.success());
    assert!(warnings(&p).is_empty());
    assert_eq!(gate(&p, "run:first")["commitments"][0]["input_reviews"][0]["status"], "current");
    // A byte edit with no date change is visible; this is independent of mention-based staleness.
    p.edit("evidence/E002.md", "A known positive was detected.", "A known positive was missed.");
    let warning = warnings(&p);
    assert_eq!(warning.len(), 1);
    assert_eq!(warning[0]["subject"], "reviews[0].inputs[0]");
    assert!(gate(&p, "run:first")["gate"]["passed"].as_bool().unwrap());
    let rev = p.rev("P01");
    p.ok(&["note", "P01", "Unrelated progress", "--expect", &rev]);
    assert_eq!(warnings(&p).len(), 1, "a fresh history line does not review evidence");
    p.ok(&["gc", "--all"]);
    assert_eq!(warnings(&p).len(), 1, "no revision cache is needed to compare hashes");
    let current = [format!("E002@{}", p.rev("E002"))];
    assert!(dispose(&p, "run:other", &current).status.success());
    assert_eq!(warnings(&p).len(), 1, "a different action cannot replace this claim");
    assert!(dispose(&p, "run:first", &current).status.success());
    assert!(warnings(&p).is_empty());
    let shown = result(&p.run(&["show", "P01", "--json"]));
    assert_eq!(shown["reviews"].as_array().unwrap().len(), 3, "old pins remain in append-only history");
    assert_eq!(gate(&p, "run:first")["commitments"][0]["input_reviews"][0]["review_index"], 2);
}

#[test]
fn missing_and_sealed_inputs_are_advisory_without_leaking_bytes() {
    for sealed in [false, true] {
        let p = fixture();
        let pin = format!("E002@{}", p.rev("E002"));
        assert!(dispose(&p, "run:one", std::slice::from_ref(&pin)).status.success());
        if sealed {
            p.edit("plied.toml", "sealed = []", "sealed = [\"evidence/E002.md\"]");
        } else {
            std::fs::remove_file(p.path().join("evidence/E002.md")).unwrap();
        }
        assert_eq!(warnings(&p).len(), 1);
        let review = gate(&p, "run:one");
        let input = &review["commitments"][0]["input_reviews"][0]["inputs"][0];
        assert_eq!(input["status"], "unavailable");
        assert!(input["current"].is_null());
        let failed = dispose(&p, "run:two", &[pin]);
        assert_eq!(failed.status.code(), Some(3), "{}", text(&failed));
        let envelope: Value = serde_json::from_slice(&failed.stdout).unwrap();
        assert!(envelope["error"]["detail"]["current"].is_null());
        let output = format!("{review} {}", text(&failed));
        assert!(!output.contains("known positive"));
        assert!(!output.contains("evidence/E002.md"));
    }
}

#[test]
fn invalid_duplicate_self_and_changed_read_sets_write_nothing() {
    let p = fixture();
    // Prefix-based sources still require exact read-set ids, not ordinary reference aliases.
    p.edit("plied.toml", "^[ED][0-9]{3}$", "^[ED][0-9]{3}");
    let good = format!("E002@{}", p.rev("E002"));
    let before = p.rev("P01");
    for pins in [vec!["E002@12345678".into()], vec![format!("P01@{before}")], vec![format!("evidence/E002.md@{}", p.rev("E002"))], vec![format!("E002.@{}", p.rev("E002"))], vec![format!("E002,locator@{}", p.rev("E002"))],
        vec![good.clone(), format!("E002@{}", "0".repeat(64))]] {
        let out = dispose(&p, "run:bad", &pins);
        assert_eq!(out.status.code(), Some(2), "{}", text(&out));
        assert_eq!(p.rev("P01"), before);
    }
    let other = format!("E003@{}", p.rev("E003"));
    p.edit("evidence/E003.md", "The dose was delivered.", "The dose was not delivered.");
    let failed = dispose(&p, "run:changed", &[good.clone(), other]);
    assert_eq!(failed.status.code(), Some(3), "{}", text(&failed));
    assert_eq!(p.rev("P01"), before);
    assert!(dispose(&p, "run:deduplicated", &[good.clone(), good]).status.success());
    assert_eq!(result(&p.run(&["show", "P01", "--json"]))["reviews"][0]["inputs"].as_array().unwrap().len(), 1);
}

#[test]
fn pin_fields_validate_and_do_not_attach_commitments() {
    let p = fixture();
    p.ok(&["new", "thread", "--title", "Unattached"]);
    let pin = format!("T002@{}", p.rev("T002"));
    assert!(dispose(&p, "run:one", &[pin]).status.success());
    assert!(result(&p.run(&["review", "T002", "--json"]))["commitments"].as_array().unwrap().is_empty());
    let path = "plied/commitments/P01-check-evidence.md";
    p.edit(path, "        rev:", "        extra: bad\n        rev:");
    let out = p.run(&["check", "--json"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(result(&out)["diagnostics"].as_array().unwrap().iter().any(|d| d["subject"] == "reviews[0].inputs[0].extra"), "{}", text(&out));
    let schema = result(&p.run(&["schema", "commitment", "--json"]));
    let reviews = schema["fields"].as_array().unwrap().iter().find(|f| f["name"] == "reviews").unwrap();
    assert!(reviews["properties"].as_array().unwrap().iter().any(|f| f["name"] == "inputs" && f["type"] == "list"));
}

#[test]
fn generic_replacement_cannot_append_an_obsolete_evidence_claim() {
    let p = fixture();
    let before = p.rev("P01");
    let input = p.rev("E002");
    let original = std::fs::read_to_string(p.path().join("plied/commitments/P01-check-evidence.md")).unwrap();
    let review = format!("\nreviews:\n  - action: run:generic\n    disposition: satisfied\n    inputs:\n      - record: E002\n        rev: {input}\n---\n");
    let replacement = p.path().join("replacement.md");
    std::fs::write(&replacement, original.replacen("\n---\n", &review, 1)).unwrap();
    p.edit("evidence/E002.md", "A known positive was detected.", "A known positive was missed.");
    let out = p.run(&["replace", "P01", replacement.to_str().unwrap(), "--expect", &before, "--json"]);
    assert_eq!(out.status.code(), Some(3), "{}", text(&out));
    assert_eq!(p.rev("P01"), before);
}

fn wait_for_locked_command(p: &Proj, args: &[String], edit: impl FnOnce()) -> Output {
    let lock_path = p.path().join(".git/plied/lock");
    let lock = plied::fsx::FileLock::new(lock_path, 5.0);
    let held = lock.lock().unwrap();
    let mut child = Command::new(bin()).arg("-C").arg(p.path()).args(args)
        .env("PLIED_AGENT", "alice").env("PLIED_SESSION", "lock-test")
        .stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
    // The command must actually be blocked, not finish its input check before we alter the source.
    std::thread::sleep(Duration::from_millis(400));
    assert!(child.try_wait().unwrap().is_none(), "writer should be waiting for the held lock");
    edit();
    drop(held);
    child.wait_with_output().unwrap()
}

#[test]
fn dispose_compares_all_inputs_after_waiting_for_the_lock() {
    let p = fixture();
    let before = p.rev("P01");
    let inputs = [format!("E002@{}", p.rev("E002")), format!("E003@{}", p.rev("E003"))];
    let args: Vec<String> = ["dispose", "P01", "--action", "run:locked", "--disposition", "satisfied", "--because", "Read both inputs", "--expect", &before,
        "--read", &inputs[0], "--read", &inputs[1], "--json"].into_iter().map(String::from).collect();
    let out = wait_for_locked_command(&p, &args, || p.edit("evidence/E003.md", "The dose was delivered.", "The dose was not delivered."));
    assert_eq!(out.status.code(), Some(3), "{}", text(&out));
    assert_eq!(p.rev("P01"), before);
}

#[test]
fn scanned_also_compares_its_source_after_waiting_for_the_lock() {
    let p = fixture();
    p.append_config("[[sources]]\nname = \"scans\"\nglob = \"scans/*.md\"\nid = \"^S[0-9]{3}$\"\nscan_commitments = true\n");
    std::fs::create_dir(p.path().join("scans")).unwrap();
    std::fs::write(p.path().join("scans/S001.md"), "---\nid: S001\ntitle: Scan\n---\nDeferred historically.\n").unwrap();
    let source = p.rev("S001");
    let before = p.rev("T001");
    let args: Vec<String> = ["scanned", "S001", "--rev", &source[..12], "--on", "T001", "--because", "Historical", "--expect", &before, "--json"]
        .into_iter().map(String::from).collect();
    let out = wait_for_locked_command(&p, &args, || p.edit("scans/S001.md", "Deferred historically.", "Deferred pending new work."));
    assert_eq!(out.status.code(), Some(3), "{}", text(&out));
    assert_eq!(p.rev("T001"), before);
}
