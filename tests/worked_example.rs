//! The tracked research example must remain readable and honest about gate limits.

mod common;

use common::*;
use std::path::Path;

fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

#[test]
fn research_snapshot_checks_cleanly_but_reviews_do_not_pin_evidence_or_identity() {
    let p = Proj::new();
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/research/store");
    copy_tree(&fixture, p.path());
    // View provenance includes the generating binary's version; regenerate on upgrades.
    p.ok(&["view"]);
    let checked: serde_json::Value = serde_json::from_str(&p.ok(&["check", "--json"])).unwrap();
    assert_eq!(checked["result"]["errors"], 0);
    assert_eq!(checked["result"]["warnings"], 0);
    assert_eq!(checked["result"]["records"], 6);
    assert_eq!(checked["result"]["linked"], 6);
    let gate = |node, action| p.code(&["review", node, "--action", action, "--gate"]);
    assert_eq!(gate("T002", "interpret:run-01"), Some(6));
    assert_eq!(gate("T002", "interpret:run-02"), Some(0));
    assert_eq!(gate("T003", "interpret:run-03"), Some(0));
    assert_eq!(gate("T001", "close:study-01"), Some(0));
    assert_eq!(gate("T002", "interpret:run-04"), Some(6));

    // A review's ref is not revision-bound. Changing the cited measurements does not
    // invalidate the recorded disposition, and check cannot evaluate the science.
    p.edit("evidence/E003-recovery.md", "6 units", "0 units");
    assert_eq!(gate("T002", "interpret:run-02"), Some(0));
    assert_eq!(p.code(&["check"]), Some(0));

    let rev = p.rev("P01");
    p.ok(&[
        "dispose", "P01", "--action", "interpret:run-04", "--disposition", "satisfied",
        "--because", "A claimed identity and disposition, not authenticated evidence",
        "--as", "another-claimed-reviewer", "--expect", &rev,
    ]);
    assert_eq!(gate("T002", "interpret:run-04"), Some(0));
    assert!(p.ok(&["show", "P01"]).contains("another-claimed-reviewer"));
}
