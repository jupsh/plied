mod common;

use common::{Proj, sha, text};
use serde_json::{Value, json};

fn result(p: &Proj, args: &[&str]) -> Value {
    let out = p.run(args);
    assert!(out.status.success(), "{args:?}: {}", text(&out));
    serde_json::from_slice::<Value>(&out.stdout).unwrap()["result"].clone()
}

/// These are format-1 bytes accepted before freshness presentation was added. In particular,
/// there is no new field on disk and check must continue to accept the store as it stands.
fn legacy_store() -> Proj {
    let p = Proj::new();
    std::fs::write(p.path().join("plied.toml"), "[plied]\nformat = 1\nrequires = \">=0.10.0\"\n\n[[sources]]\nname = \"decisions\"\nglob = \"decisions/*.md\"\nid = \"^D[0-9]{3}$\"\ndate = \"date\"\n").unwrap();
    std::fs::create_dir_all(p.path().join("decisions")).unwrap();
    std::fs::create_dir_all(p.path().join("plied/threads")).unwrap();
    std::fs::create_dir_all(p.path().join("plied/commitments")).unwrap();
    std::fs::create_dir_all(p.path().join("plied/questions")).unwrap();
    for (file, meta) in [
        ("threads/T001.md", "id: T001\ntitle: Scheduled\nstatus: open\nnext: 1\nactor: alice\nuses: D000\n"),
        ("threads/T002.md", "id: T002\ntitle: User\nstatus: open\nuses: T001\n"),
        ("threads/T003.md", "id: T003\ntitle: Child\nstatus: open\nparent: T001\n"),
        ("threads/T004.md", "id: T004\ntitle: Closed\nstatus: closed\nuses: T001\noutcome:\n  ref: D000\n  quote: Done.\n"),
        ("commitments/P01.md", "id: P01\ntitle: Attached\nwording: Done.\nmade: D000\nstate: open\nnodes: T001\n"),
        ("questions/Q01.md", "id: Q01\ntitle: Question\nraised: D000\nstate: open\nnodes: T001\n"),
    ] {
        std::fs::write(p.path().join("plied").join(file), format!("---\n{meta}---\n## History\n- 2000-01-01 00:00 UTC alice: created\n")).unwrap();
    }
    source(&p, "D000", "1999-01-01", "Done.");
    source(&p, "D001", "2001-01-01", "T001 changes method.");
    source(&p, "D002", "2002-01-01", "D000 is mentioned as context.");
    p
}

fn source(p: &Proj, id: &str, date: &str, body: &str) {
    std::fs::write(p.path().join(format!("decisions/{id}.md")), format!("---\nid: {id}\ntitle: {id}\ndate: {date}\n---\n{body}\n")).unwrap();
}

fn freshness(p: &Proj, id: &str) -> Value {
    result(p, &["show", id, "--freshness", "--json"])["freshness"].clone()
}

#[test]
fn freshness_is_opt_in_and_a_legacy_store_keeps_its_warnings() {
    let p = legacy_store();
    let before = result(&p, &["check", "--json"]);
    assert_eq!(before["errors"], 0);
    assert!(before["warnings"].as_u64().unwrap() > 0);
    assert!(result(&p, &["show", "T001", "--json"]).get("freshness").is_none());
    let before_bytes = std::fs::read(p.path().join("plied/threads/T001.md")).unwrap();
    let actual = freshness(&p, "T001");
    assert_eq!(actual[0]["record"], "D001");
    assert_eq!(actual[0]["subject"], "named");
    assert_eq!(actual[0]["mentions"], json!(["T001"]));
    assert_eq!(actual[0]["via"], json!([]));
    assert_eq!(actual[0]["rev"], sha(&std::fs::read(p.path().join("decisions/D001.md")).unwrap()));
    assert_eq!(actual[1]["record"], "D002");
    assert_eq!(actual[1]["subject"], "links");
    assert_eq!(actual[1]["mentions"], json!(["D000"]));
    assert_eq!(actual[1]["via"], json!([{"field": "uses", "id": "D000"}]));
    assert_eq!(freshness(&p, "T003"), json!([]), "parent is organisational");
    assert_eq!(freshness(&p, "T004"), json!([]), "resolved references are history");
    assert_eq!(freshness(&p, "D001"), json!([]), "linked records have no own stale check");
    // Attachment/made warnings are still individually reviewable: orientation clears nothing.
    assert_eq!(freshness(&p, "P01")[0]["via"], json!([{"field": "nodes", "id": "T001"}]));
    assert_eq!(freshness(&p, "P01")[1]["via"], json!([{"field": "made", "id": "D000"}]));
    assert_eq!(std::fs::read(p.path().join("plied/threads/T001.md")).unwrap(), before_bytes);
    let after = result(&p, &["check", "--json"]);
    assert_eq!(after["diagnostics"], before["diagnostics"]);
    assert_eq!(after["errors"], before["errors"]);
    assert_eq!(after["warnings"], before["warnings"]);
}

#[test]
fn frontier_freshness_respects_actor_selection_and_includes_questions() {
    let p = legacy_store();
    let default = result(&p, &["frontier", "--json"]);
    assert!(default["queues"]["alice"][0].get("freshness").is_none());
    let fresh = result(&p, &["frontier", "--freshness", "--json"]);
    assert_eq!(fresh["queues"]["alice"][0]["freshness"], freshness(&p, "T001"));
    assert_eq!(fresh["unscheduled"][0]["freshness"], freshness(&p, "T002"));
    assert_eq!(fresh["open_questions"][0]["freshness"], freshness(&p, "Q01"));
    let alice = result(&p, &["frontier", "--freshness", "--actor", "alice", "--json"]);
    assert_eq!(alice["queues"]["alice"][0]["freshness"], freshness(&p, "T001"));
    assert_eq!(alice["unscheduled"], json!([]));
    assert_eq!(alice["open_questions"], json!([]));
    assert_eq!(alice["dates"], json!([]));
    let plain = p.ok(&["frontier", "--freshness"]);
    assert!(plain.contains("Potentially stale: D001"), "{plain}");
}

#[test]
fn grouped_stale_causes_preserve_individual_diagnostics_and_reference_edges() {
    let p = legacy_store();
    let normal = result(&p, &["check", "--json"]);
    assert!(normal.get("stale_groups").is_none());
    let grouped = result(&p, &["check", "--group-stale", "--json"]);
    assert_eq!(normal["diagnostics"], grouped["diagnostics"]);
    assert_eq!(normal["errors"], grouped["errors"]);
    assert_eq!(normal["warnings"], grouped["warnings"]);
    let groups = grouped["stale_groups"].as_array().unwrap();
    assert_eq!(groups.len(), 2);
    assert_eq!(groups[0]["record"], "D001");
    assert_eq!(groups[0]["named"].as_array().unwrap().iter().map(|v| v["id"].clone()).collect::<Vec<_>>(), vec![json!("T001")]);
    let linked = groups[0]["links"].as_array().unwrap();
    assert_eq!(linked.iter().map(|v| v["id"].clone()).collect::<Vec<_>>(), vec![json!("P01"), json!("Q01"), json!("T002")]);
    assert_eq!(linked[2]["via"], json!([{"field": "uses", "id": "T001"}]));
    assert_eq!(linked[2]["path"], "plied/threads/T002.md");
    assert_eq!(result(&p, &["check", "--group-stale", "--errors-only", "--json"])["stale_groups"], json!([]));
}

#[test]
fn freshness_reports_all_readable_causes_and_skips_unrelated_validation() {
    let p = legacy_store();
    for n in 3..=10 {
        source(&p, &format!("D{n:03}"), "2003-01-01", "T001 and D000 both change.");
    }
    // Simulate an unrelated broken record: orientation still needs no full-project validation.
    std::fs::write(p.path().join("plied/threads/T099.md"), "---\nid: T099\ntitle: Broken elsewhere\nstatus: unknown\n---\n").unwrap();
    let causes = freshness(&p, "T001");
    assert_eq!(causes.as_array().unwrap().len(), 10, "structured causes are never display-truncated");
    assert_eq!(causes[1]["subject"], "named", "direct mentions win over simultaneous reference mentions");
    assert_eq!(causes[1]["mentions"], json!(["D000", "T001"]));
    p.edit("plied.toml", "[[sources]]", "[access]\nsealed = [\"D003.md\"]\n\n[[sources]]");
    let sealed = freshness(&p, "T001");
    assert_eq!(sealed.as_array().unwrap().len(), 9);
    assert!(!sealed.as_array().unwrap().iter().any(|v| v["record"] == "D003"));
    assert!(!p.ok(&["frontier", "--freshness"]).contains("D003"));
    assert_eq!(p.code(&["check", "--json"]), Some(1));
}
