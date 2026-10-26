//! Conformance fixtures for FORMAT.md: the behaviour a second implementation must match, each test
//! naming the section it pins down. Most pair a case that is accepted with one that is not, because
//! the negative cases are where two plausible readings of a field part ways.

mod common;

use common::*;

/// A store with the evidence source and two open threads, T001 and T002.
fn two_threads() -> Proj {
    let p = Proj::new();
    p.with_evidence();
    p.ok(&["new", "thread", "--title", "First"]);
    p.ok(&["new", "thread", "--title", "Second"]);
    p
}

fn commitment(p: &Proj, extra: &[&str]) {
    p.evidence("E001", "", "Review before every run.");
    let mut args = vec!["new", "commitment", "--title", "Review first", "wording=Review before every run.", "made=E001"];
    args.extend(extra);
    p.ok(&args);
}

fn gate(p: &Proj, node: &str, action: &str) -> Option<i32> {
    p.code(&["review", node, "--action", action, "--gate"])
}

fn dispose(p: &Proj, action: &str, disposition: &str) {
    let rev = p.rev("P01");
    p.ok(&["dispose", "P01", "--action", action, "--disposition", disposition, "--because", "fixture", "--expect", &rev]);
}

// ------------------------------------------------------------------- §12 applicability

#[test]
fn no_applicable_commitment_passes_the_gate() {
    let p = two_threads();
    assert_eq!(gate(&p, "T001", "run:1"), Some(0));
}

#[test]
fn attachment_by_nodes_by_links_and_through_dependencies() {
    let p = two_threads();
    commitment(&p, &["nodes=[T001]"]);
    assert_eq!(gate(&p, "T001", "run:1"), Some(6), "attached by nodes");
    assert_eq!(gate(&p, "T002", "run:1"), Some(0), "not attached, not reached");
    // a node that waits on (or uses) T001 reaches its commitment
    assert!(p.set("T002", &["status=waiting", "waiting_on=[T001]"]).status.success());
    assert_eq!(gate(&p, "T002", "run:1"), Some(6), "reached through waiting_on");

    let q = two_threads();
    commitment(&q, &["applies=project"]);
    assert_eq!(gate(&q, "T002", "run:1"), Some(6), "applies: project reaches every node");

    let r = two_threads();
    commitment(&r, &[]);
    assert!(r.set("T001", &["links=[P01]"]).status.success());
    assert_eq!(gate(&r, "T001", "run:1"), Some(6), "a node that links the commitment attaches it");
}

#[test]
fn a_parent_passes_on_commitments_only_with_applies_subtree() {
    let p = two_threads();
    assert!(p.set("T002", &["parent=T001"]).status.success());
    commitment(&p, &["nodes=[T001]"]);
    assert_eq!(gate(&p, "T002", "run:1"), Some(0), "applies: linked stops at the parent");
    assert!(p.set("P01", &["applies=subtree"]).status.success());
    assert_eq!(gate(&p, "T002", "run:1"), Some(6), "applies: subtree reaches the child");
}

#[test]
fn only_a_resolved_commitment_stops_applying() {
    let p = two_threads();
    commitment(&p, &["nodes=[T001]"]);
    // a malformed state is an error in check, and the gate still holds (fails closed)
    p.edit("plied/commitments/P01-review-first.md", "state: open", "state: opne");
    assert_eq!(gate(&p, "T001", "run:1"), Some(6));
    p.edit("plied/commitments/P01-review-first.md", "state: opne", "state: open");
    p.evidence("E002", "", "Reviews are done.");
    assert!(p.set("P01", &["state=fulfilled", "resolved_by=E002"]).status.success());
    assert_eq!(gate(&p, "T001", "run:1"), Some(0));
}

#[test]
fn the_latest_disposition_for_the_exact_action_decides() {
    let p = two_threads();
    commitment(&p, &["nodes=[T001]"]);
    dispose(&p, "run:1", "acting");
    assert_eq!(gate(&p, "T001", "run:1"), Some(0), "any disposition but blocks passes, acting included");
    assert_eq!(gate(&p, "T001", "run:2"), Some(6), "a disposition for another action does not count");
    assert_eq!(gate(&p, "T001", "Run:1"), Some(6), "action names are compared exactly");
    dispose(&p, "run:1", "blocks");
    assert_eq!(gate(&p, "T001", "run:1"), Some(6), "the last in list order wins");
    dispose(&p, "run:1", "satisfied");
    assert_eq!(gate(&p, "T001", "run:1"), Some(0));
}

// ------------------------------------------------------------------------- §3.3 fields

#[test]
fn an_empty_value_reads_as_absent() {
    let p = two_threads();
    commitment(&p, &["nodes=[T001]"]);
    p.edit("plied/commitments/P01-review-first.md", "state: open", "state: open\napplies: \"\"");
    let out = p.ok(&["check", "--errors-only"]);
    assert!(out.contains("0 error(s)"), "an empty applies is the default, linked:\n{out}");
}

#[test]
fn closing_records_an_outcome_not_success_and_carries_does_not_block() {
    let p = two_threads();
    p.evidence("E001", "", "The loan was declined.");
    p.ok(&["new", "question", "--title", "Which substitute?", "raised=T001"]);
    assert!(p.set("T002", &["status=waiting", "waiting_on=[T001]"]).status.success());
    let closed = p.set("T001", &["status=closed", "outcome.ref=E001", "outcome.quote=The loan was declined.", "carries=[Q01]"]);
    assert!(closed.status.success(), "{}", text(&closed));
    // Q01 is still open, but only waiting_on blocks
    assert!(p.set("T002", &["status=open"]).status.success());
    // naming Q01 in waiting_on does block
    assert_eq!(p.set("T002", &["status=waiting", "waiting_on+=Q01"]).status.code(), Some(0));
    assert_eq!(p.set("T002", &["status=open"]).status.code(), Some(4));
}

fn with_paper(p: &Proj) {
    std::fs::create_dir_all(p.path().join("papers")).unwrap();
    std::fs::create_dir_all(p.path().join("cache/Smith2020")).unwrap();
    std::fs::write(p.path().join("papers/Smith2020.pdf"), b"%PDF-fake").unwrap();
    std::fs::write(p.path().join("cache/Smith2020/p01.txt"), "Table 2. Bout lengths.\n").unwrap();
    let sha = sha(b"%PDF-fake");
    std::fs::write(
        p.path().join("literature.json"),
        format!("{{\"papers\": [{{\"id\": \"Smith2020\", \"file\": \"Smith2020.pdf\", \"sha256\": \"{sha}\"}}]}}"),
    )
    .unwrap();
    p.append_config("[literature]\nmanifest = \"literature.json\"\npdf_dir = \"papers\"\ntext_dir = \"cache/{id}\"\n");
}

#[test]
fn image_checked_is_a_boolean() {
    let p = Proj::new();
    with_paper(&p);
    p.ok(&["new", "thread", "--title", "Citing"]);
    let f = "plied/threads/T001-citing.md";
    let cite = |v: &str| format!("status: open\ncites:\n  - paper: Smith2020\n    page: 1\n    kind: table\n    image_checked: {v}");
    p.edit(f, "status: open", &cite("\"true\""));
    let out = p.ok(&["check"]);
    assert!(!out.contains("image_checked"), "true is checked:\n{out}");
    p.edit(f, "image_checked: \"true\"", "image_checked: \"false\"");
    let out = p.ok(&["check"]);
    assert!(out.contains("warning") && out.contains("image_checked"), "false is not checked, a warning:\n{out}");
    p.edit(f, "image_checked: \"false\"", "image_checked: 2026-10-24");
    let out = p.run(&["check"]);
    assert_eq!(out.status.code(), Some(1), "anything else is an error:\n{}", text(&out));
    assert!(text(&out).contains("cites.image_checked must be one of true, false"), "{}", text(&out));
}

// ---------------------------------------------------------------------- §4 authority

#[test]
fn authority_decision_table() {
    let p = two_threads();
    let approve = |by: &str| p.set("T001", &["approval.state=approved", &format!("approval.by={by}")]).status.code();
    p.evidence("D001", "authority: reviewer\nscope: [T001]\n", "Approved.");
    assert_eq!(approve("D001"), Some(4), "an authority not in approve_authorities");
    p.evidence("D002", "authority: user\nscope: [T002]\n", "Approved.");
    assert_eq!(approve("D002"), Some(4), "a scope without the node");
    p.evidence("D003", "authority: user\n", "Approved.");
    assert_eq!(approve("D003"), Some(4), "no scope at all");
    p.evidence("D004", "authority: delegated by D999\nscope: [T001]\n", "Approved.");
    assert_eq!(approve("D004"), Some(4), "delegated by a record that does not exist");
    assert_eq!(approve("T002"), Some(4), "a plied record cannot approve");

    // accepted: the delegating record need only exist; its own authority is not examined
    p.evidence("E009", "", "Anything at all.");
    p.evidence("D005", "authority: delegated by E009\nscope: [T001]\n", "Approved.");
    assert_eq!(approve("D005"), Some(0));
    // accepted with a warning: no authority field means it cannot be verified, not that it fails
    let q = two_threads();
    q.evidence("D006", "scope: [T001]\n", "Approved.");
    assert!(q.set("T001", &["approval.state=approved", "approval.by=D006"]).status.success());
    let out = q.ok(&["check"]);
    assert!(out.contains("warning") && out.contains("cannot be verified"), "{out}");
}

#[test]
fn an_approval_names_a_node_not_a_revision() {
    let p = two_threads();
    p.evidence("D001", "authority: user\nscope: [T001]\n", "Approved.");
    assert!(p.set("T001", &["approval.state=approved", "approval.by=D001"]).status.success());
    assert!(p.set("T001", &["title=First, rewritten after approval"]).status.success());
    assert!(p.ok(&["check", "--errors-only"]).contains("0 error(s)"), "the approval survives the edit");
}

// --------------------------------------------------------------- §8 diagnostic identity

#[test]
fn resolving_one_of_several_dependencies_adds_no_error() {
    let p = two_threads();
    p.ok(&["new", "thread", "--title", "Third"]);
    // an invalid state made by hand: open, with two unresolved dependencies
    p.edit("plied/threads/T003-third.md", "status: open", "status: open\nwaiting_on: [T001, T002]");
    p.evidence("E001", "", "Done.");
    let out = p.set("T001", &["status=closed", "outcome.ref=E001", "outcome.quote=Done."]);
    assert!(out.status.success(), "the remaining error on T003 is the same error:\n{}", text(&out));
    // a genuinely new error is still refused: a third unresolved dependency
    p.ok(&["new", "thread", "--title", "Fourth"]);
    let out = p.set("T003", &["waiting_on+=T004"]);
    assert_eq!(out.status.code(), Some(4), "{}", text(&out));
    assert!(text(&out).contains("T004"), "{}", text(&out));
}

#[test]
fn check_json_carries_the_identity() {
    let p = two_threads();
    p.edit("plied/threads/T002-second.md", "status: open", "status: open\nwaiting_on: [T001]");
    let v: serde_json::Value = serde_json::from_slice(&p.run(&["check", "--json"]).stdout).unwrap();
    let e = v["result"]["diagnostics"].as_array().unwrap().iter().find(|e| e["code"] == "state").unwrap();
    assert_eq!(e["where"], "plied/threads/T002-second.md");
    assert_eq!(e["subject"], "waiting_on:T001");
}

// ---------------------------------------------------------------------- §5 messages

#[test]
fn a_reused_message_id_must_repeat_the_original() {
    let p = two_threads();
    p.ok(&["send", "T001", "hold the rerun", "--id", "M1"]);
    assert!(p.ok(&["send", "T001", "hold the rerun", "--id", "M1"]).contains("already recorded"));
    let changed = p.run(&["send", "T001", "go ahead", "--id", "M1"]);
    assert_eq!(changed.status.code(), Some(3), "{}", text(&changed));
    let elsewhere = p.run(&["send", "T002", "hold the rerun", "--id", "M1"]);
    assert_eq!(elsewhere.status.code(), Some(3), "ids are unique across logs:\n{}", text(&elsewhere));
}

#[test]
fn unsupported_event_versions_are_skipped_and_reported() {
    let p = two_threads();
    p.ok(&["send", "T001", "hello", "--to", "bob"]);
    let log = p.path().join("plied/messages/T001.jsonl");
    let mut t = std::fs::read_to_string(&log).unwrap();
    t += "{\"v\": 2, \"id\": \"FUTURE\", \"type\": \"message\", \"node\": \"T001\", \"text\": \"from the future\"}\n";
    std::fs::write(&log, t).unwrap();
    assert!(!p.ok(&["messages", "T001"]).contains("from the future"));
    let out = p.run(&["check"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(text(&out).contains("event version 2 is not supported"), "{}", text(&out));
}

#[test]
fn recipients_are_fixed_when_a_message_is_sent() {
    let p = two_threads();
    p.run_as("alice", &["claim", "T001"]);
    p.run_as("bob", &["claim", "T001"]);
    let who = p.ok(&["who", "T001"]);
    assert!(who.contains("alice") && who.contains("bob"), "claims are presence, not exclusive:\n{who}");
    p.ok(&["send", "T001", "reread the intake note"]);
    p.run_as("cara", &["claim", "T001"]);
    assert!(!p.run_as("cara", &["inbox", "--peek"]).stdout.windows(6).any(|w| w == b"reread"));
    assert!(p.ok(&["messages", "T001"]).contains("reread the intake note"));
}

// ------------------------------------------------------------------------ §9 access

#[test]
fn a_sealed_message_log_is_neither_read_nor_appended_to() {
    let p = two_threads();
    p.ok(&["send", "T001", "the source is K", "--to", "bob"]);
    let log = p.path().join("plied/messages/T001.jsonl");
    let mut t = std::fs::read_to_string(&log).unwrap();
    t += "not json at all\n";
    std::fs::write(&log, t).unwrap();
    p.edit("plied.toml", "sealed = []", "sealed = [\"plied/messages/T001\"]");
    assert!(!p.ok(&["messages", "T001"]).contains("the source is K"));
    assert!(!String::from_utf8_lossy(&p.run_as("bob", &["inbox", "--peek"]).stdout).contains("the source is K"));
    assert!(p.ok(&["check", "--errors-only"]).contains("0 error(s)"), "a sealed log is not read, so its bad line is not seen");
    let send = p.run(&["send", "T001", "more"]);
    assert_ne!(send.status.code(), Some(0));
    assert!(text(&send).contains("sealed"), "{}", text(&send));
}

// ------------------------------------------------------------------ §11 JSON results

fn envelope(out: &std::process::Output) -> serde_json::Value {
    assert!(out.stderr.is_empty(), "--json writes nothing to stderr:\n{}", text(out));
    serde_json::from_slice(&out.stdout).unwrap_or_else(|e| panic!("not one JSON document ({e}):\n{}", text(out)))
}

#[test]
fn every_command_answers_json_in_one_envelope() {
    let p = two_threads();
    commitment(&p, &["nodes=[T001]"]);
    p.ok(&["index"]);
    let commands: &[&[&str]] = &[
        &["show", "T001"],
        &["list"],
        &["find", "{\"kind\": \"thread\"}"],
        &["backlinks", "T001"],
        &["tree"],
        &["frontier"],
        &["view", "--print"],
        &["view", "--check"],
        &["check"],
        &["claim", "T001"],
        &["who"],
        &["send", "T001", "hello", "--to", "bob"],
        &["subscribe", "T002"],
        &["unsubscribe", "T002"],
        &["inbox", "--peek"],
        &["messages", "T001"],
        &["review", "T001", "--action", "run:1"],
        &["recover"],
        &["release"],
        &["index"],
        &["search", "First"],
        &["view"],
    ];
    for c in commands {
        let mut args = c.to_vec();
        args.push("--json");
        let out = p.run(&args);
        let v = envelope(&out);
        assert_eq!(v["plied"], 1, "{c:?}");
        assert_eq!(v["exit"], out.status.code().unwrap(), "{c:?}");
        assert_eq!(v["ok"], out.status.success(), "{c:?}");
        assert!(v.get("result").is_some() && v["notes"].is_array(), "{c:?}: {v}");
    }
}

#[test]
fn json_failures_carry_their_class_and_detail() {
    let p = two_threads();
    let stale = p.rev("T001");
    p.set("T001", &["actor=bob"]);
    let v = envelope(&p.run(&["set", "T001", "actor=carol", "--expect", &stale, "--json"]));
    assert_eq!((v["exit"].as_i64(), v["status"].as_str()), (Some(3), Some("conflict")));
    assert_eq!(v["error"]["detail"]["current"], p.rev("T001"));

    let rev = p.rev("T001");
    let v = envelope(&p.run(&["set", "T001", "waiting_on=[T999]", "--expect", &rev, "--json"]));
    assert_eq!(v["status"], "invalid");
    assert_eq!(v["error"]["detail"]["diagnostics"][0]["subject"], "waiting_on:T999");

    let v = envelope(&p.run(&["show", "T001", "--json", "--no-such-flag"]));
    assert_eq!((v["exit"].as_i64(), v["status"].as_str()), (Some(2), Some("usage")));

    commitment(&p, &["nodes=[T001]"]);
    let out = p.run(&["review", "T001", "--action", "run:1", "--gate", "--json"]);
    assert_eq!(out.status.code(), Some(6));
    let v = envelope(&out);
    assert_eq!((v["status"].as_str(), v["result"]["gate"]["passed"].as_bool()), (Some("gate"), Some(false)));
    assert!(v["result"]["gate"]["reasons"][0].as_str().unwrap().contains("P01"));
}

#[test]
fn notes_go_into_the_envelope() {
    let p = two_threads();
    // a prepare intent left by a crash: the next write undoes it and says so, apart from its own result
    std::fs::write(p.path().join(".git/plied/intent.json"), r#"{"version": 1, "state": "prepare", "files": []}"#).unwrap();
    let v = envelope(&p.run(&["new", "thread", "--title", "Third", "--json"]));
    assert_eq!(v["result"]["id"], "T003");
    assert!(v["notes"][0].as_str().unwrap().contains("before this command: undid an interrupted write"), "{v}");
}

#[test]
fn expect_takes_a_revision_prefix_of_eight_or_more() {
    let p = two_threads();
    let rev = p.rev("T001");
    assert_eq!(p.code(&["note", "T001", "short", "--expect", &rev[..7]]), Some(2), "too short to name a revision");
    assert_eq!(p.code(&["note", "T001", "short", "--expect", &rev[..8]]), Some(0));
    // the prefix `show` prints above the record is enough
    let rev = p.rev("T001");
    assert!(p.ok(&["show", "T001"]).contains(&format!("rev: {}", &rev[..12])));
    assert_eq!(p.code(&["note", "T001", "shown", "--expect", &rev[..12]]), Some(0));
}

#[test]
fn messages_names_a_record_or_fails() {
    let p = two_threads();
    // `messages` shows a node's conversation: a node that names no record is an
    // error (exit 1), as `send` and `subscribe` already report, never an empty log
    assert_eq!(p.code(&["messages", "T999"]), Some(1));
    assert_eq!(p.code(&["messages", "T001"]), Some(0), "a real node with no messages is fine");
}

#[test]
fn frontier_with_an_actor_scopes_every_section_in_json() {
    let p = two_threads();
    p.ok(&["new", "question", "--title", "Whose?", "raised=T001"]);
    assert!(p.set("T001", &["actor=bob", "due=2000-01-01"]).status.success());
    assert!(p.set("T002", &["actor=alice", "next=1"]).status.success());
    // text frontier with --actor already shows only that actor's work; --json must agree:
    // open questions and dates are no one actor's, so they are empty, not everyone's
    let v = envelope(&p.run(&["frontier", "--actor", "bob", "--json"]));
    assert_eq!(v["result"]["open_questions"], serde_json::json!([]), "{v}");
    assert_eq!(v["result"]["dates"], serde_json::json!([]), "{v}");
    let unscheduled = v["result"]["unscheduled"].as_array().unwrap();
    assert!(unscheduled.iter().all(|r| r["actor"] == "bob"), "{v}");
    // without --actor both sections are present as before
    let v = envelope(&p.run(&["frontier", "--json"]));
    assert_eq!(v["result"]["open_questions"].as_array().unwrap().len(), 1, "{v}");
    assert!(!v["result"]["dates"].as_array().unwrap().is_empty(), "{v}");
}

#[test]
fn frontier_lists_open_threads_in_no_queue() {
    let p = two_threads();
    p.ok(&["new", "thread", "--title", "Third"]);
    assert!(p.set("T001", &["actor=bob", "next=1"]).status.success());
    assert!(p.set("T002", &["actor=bob"]).status.success());
    let v = envelope(&p.run(&["frontier", "--json"]));
    let ids = |v: &serde_json::Value| -> Vec<(String, serde_json::Value)> {
        v["result"]["unscheduled"].as_array().unwrap().iter().map(|r| (r["id"].as_str().unwrap().to_string(), r["actor"].clone())).collect()
    };
    assert_eq!(ids(&v), [("T002".to_string(), serde_json::json!("bob")), ("T003".to_string(), serde_json::Value::Null)]);
    // --actor narrows them to the threads that actor holds
    let v = envelope(&p.run(&["frontier", "--actor", "bob", "--json"]));
    assert_eq!(ids(&v), [("T002".to_string(), serde_json::json!("bob"))]);
}

// --------------------------------------------------------------------------- §8 stale

#[test]
fn a_record_about_a_group_makes_the_group_stale_not_its_children() {
    let p = Proj::new();
    p.append_config("[[sources]]\nname = \"log\"\nglob = \"log/*.md\"\nid = \"^D[0-9]{3}$\"\ndate = \"date\"\n");
    std::fs::create_dir_all(p.path().join("log")).unwrap();
    p.ok(&["new", "thread", "--title", "Group", "status=group"]);
    p.ok(&["new", "thread", "--title", "Child", "parent=T001"]);
    p.ok(&["new", "thread", "--title", "User", "uses=T001"]);
    std::fs::write(p.path().join("log/D001.md"), "---\nid: D001\ntitle: news\ndate: 2999-01-01\n---\nT001 changes method.\n").unwrap();
    let stale: Vec<String> = envelope(&p.run(&["check", "--json"]))["result"]["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|d| d["code"] == "stale")
        .map(|d| d["where"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(stale.len(), 2, "{stale:?}");
    assert!(stale[0].contains("T001-") && stale[1].contains("T003-"), "the group and its user, not the child by parent: {stale:?}");
}

#[test]
fn a_resolved_record_goes_stale_only_when_named() {
    let p = Proj::new();
    p.append_config("[[sources]]\nname = \"log\"\nglob = \"log/*.md\"\nid = \"^D[0-9]{3}$\"\ndate = \"date\"\n");
    std::fs::create_dir_all(p.path().join("log")).unwrap();
    std::fs::write(p.path().join("log/D000.md"), "---\nid: D000\ntitle: old\ndate: 2000-01-01\n---\nDone.\n").unwrap();
    p.ok(&["new", "thread", "--title", "Open"]);
    for title in ["Closed user", "Closed named"] {
        p.ok(&["new", "thread", "--title", title, "uses=T001", "status=closed", "outcome.ref=D000", "outcome.quote=Done."]);
    }
    // names T001, which both closed threads use, and T003 itself
    std::fs::write(p.path().join("log/D001.md"), "---\nid: D001\ntitle: news\ndate: 2999-01-01\n---\nT001 changes; T003 is reopened.\n").unwrap();
    let stale: Vec<String> = envelope(&p.run(&["check", "--json"]))["result"]["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|d| d["code"] == "stale")
        .map(|d| d["where"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(stale.len(), 2, "{stale:?}");
    assert!(stale[0].contains("T001-") && stale[1].contains("T003-"), "the open node, and the closed one named, not the closed one by its link: {stale:?}");
}

// ------------------------------------------------------------------------ §7 the view

#[test]
fn the_view_does_not_depend_on_linked_records() {
    let p = Proj::new();
    p.append_config("[[sources]]\nname = \"backlog\"\nglob = \"backlog/*.md\"\nid = \"^B[0-9]{3}$\"\nstatus = \"status\"\n");
    std::fs::create_dir_all(p.path().join("backlog")).unwrap();
    std::fs::write(p.path().join("backlog/B001.md"), "---\nid: B001\ntitle: item\nstatus: open\n---\n").unwrap();
    p.ok(&["new", "thread", "--title", "First", "backlog=B001"]);
    p.edit("backlog/B001.md", "status: open", "status: done");
    // no plied write happened, so the view a write generated must still be current
    assert_eq!(p.code(&["view", "--check"]), Some(0), "{}", text(&p.run(&["view", "--check"])));
    // `tree` reads the linked state live
    assert!(p.ok(&["tree"]).contains("[B001: done]"));
}

// ------------------------------------------------------------------- §10 citation results

#[test]
fn a_citation_always_gets_five_results_in_order() {
    let p = Proj::new();
    with_paper(&p);
    p.ok(&["new", "thread", "--title", "Citing"]);
    p.edit("plied/threads/T001-citing.md", "status: open", "status: open\ncites:\n  - paper: Smith2020\n    page: 9\n    quote: anything");
    let out = p.run(&["lit", "check", "--json"]);
    assert_eq!(out.status.code(), Some(1));
    let v = envelope(&out);
    let results = v["result"]["citations"][0]["results"].as_array().unwrap();
    let got: Vec<(&str, &str)> = results.iter().map(|r| (r["aspect"].as_str().unwrap(), r["status"].as_str().unwrap())).collect();
    // page 9 does not exist: quote cannot be reached, so it is unchecked, never missing
    assert_eq!(got, [("clearance", "ok"), ("hash", "ok"), ("page", "fail"), ("quote", "unchecked"), ("image", "unchecked")]);
}

// ---------------------------------------------------------------------- §3 encodings

#[test]
fn encodings_bom_crlf_and_invalid_utf8() {
    let p = two_threads();
    let f = p.path().join("plied/threads/T001-first.md");
    let lf = std::fs::read(&f).unwrap();
    let crlf: Vec<u8> = [b"\xef\xbb\xbf".as_slice(), &String::from_utf8(lf.clone()).unwrap().replace('\n', "\r\n").into_bytes()].concat();
    std::fs::write(&f, &crlf).unwrap();
    assert!(p.ok(&["check", "--errors-only"]).contains("0 error(s)"), "a BOM and CRLF read like the plain file");
    assert_eq!(p.rev("T001"), sha(&crlf), "the revision is the hash of the raw bytes");
    assert!(p.set("T001", &["actor=bob"]).status.success());
    let rewritten = std::fs::read(&f).unwrap();
    assert!(!rewritten.starts_with(b"\xef\xbb\xbf") && !rewritten.contains(&b'\r'), "plied writes no BOM and LF line ends");

    let mut bad = std::fs::read(&f).unwrap();
    bad.extend(b"\xff\xfe");
    std::fs::write(&f, bad).unwrap();
    let out = p.run(&["check"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(text(&out).contains("not valid UTF-8"), "{}", text(&out));
}

#[test]
fn an_assignment_operator_is_at_the_first_equals_sign() {
    let p = two_threads();
    assert!(p.set("T001", &["closes_when=a += b, then c -= d"]).status.success());
    let v: serde_json::Value = serde_json::from_slice(&p.run(&["show", "T001", "--json"]).stdout).unwrap();
    assert_eq!(v["result"]["closes_when"], "a += b, then c -= d");
    assert!(p.set("T001", &["links+=T002"]).status.success());
    assert!(p.set("T001", &["links+=[T002, E001]", "links-=E001"]).status.success(), "+= skips items already present");
    let v: serde_json::Value = serde_json::from_slice(&p.run(&["show", "T001", "--json"]).stdout).unwrap();
    assert_eq!(v["result"]["links"], serde_json::json!(["T002"]));
}

#[test]
fn a_quote_opens_a_flow_item_only_at_its_start() {
    let p = two_threads();
    let options = |p: &Proj| envelope(&p.run(&["show", "Q01", "--json"]))["result"]["options"].clone();
    p.ok(&["new", "question", "--title", "Q", "raised=T001", "options=[a, it's b, c]"]);
    assert_eq!(options(&p), serde_json::json!(["a", "it's b", "c"]), "assigned");
    p.edit("plied/questions/Q01-q.md", "options: [a, it's b, c]", "options: [a, it's b, 'c, d']");
    assert_eq!(options(&p), serde_json::json!(["a", "it's b", "c, d"]), "read from a hand-written file");
}

// ------------------------------------------------------------ versions and configuration

#[test]
fn init_writes_a_format_and_a_minimum_version() {
    let p = Proj::new();
    let toml = std::fs::read_to_string(p.path().join("plied.toml")).unwrap();
    assert!(toml.contains("format = 1"), "{toml}");
    assert!(toml.contains(&format!("requires = \">={}\"", env!("CARGO_PKG_VERSION"))), "{toml}");
}

#[test]
fn an_unknown_format_or_an_excluded_version_refuses_writes() {
    let p = two_threads();
    p.edit("plied.toml", "format = 1", "format = 2");
    let out = p.run(&["new", "thread", "--title", "Third"]);
    assert_eq!(out.status.code(), Some(5), "{}", text(&out));
    let out = p.run(&["check", "--json"]);
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(v["result"]["diagnostics"].as_array().unwrap().iter().any(|d| d["code"] == "version" && d["subject"] == "format"), "{v}");
    assert!(v["notes"][0].as_str().unwrap().contains("format 2"), "reads say they may misread: {v}");
    p.edit("plied.toml", "format = 2", "format = 1");

    let req = format!("requires = \">={}\"", env!("CARGO_PKG_VERSION"));
    p.edit("plied.toml", &req, "requires = \">=99.0.0\"");
    assert_eq!(p.code(&["new", "thread", "--title", "Third"]), Some(5));
    p.edit("plied.toml", "requires = \">=99.0.0\"", &format!("requires = \"{}\"", env!("CARGO_PKG_VERSION")));
    assert_eq!(p.code(&["new", "thread", "--title", "Third"]), Some(0), "an exact pin to this version is allowed");
    p.edit("plied.toml", &format!("requires = \"{}\"", env!("CARGO_PKG_VERSION")), "requires = \"^0.1\"");
    let out = p.run(&["list"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(text(&out).contains("plied.requires"), "{}", text(&out));
}

#[test]
fn a_misspelt_setting_is_an_error_not_a_default() {
    let p = Proj::new();
    p.edit("plied.toml", "sealed = []", "seald = [\"secret/\"]");
    let out = p.run(&["list"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(text(&out).contains("access.seald: not a plied.toml setting"), "{}", text(&out));
}

#[test]
fn action_kinds_restrict_new_reviews_and_warn_about_old_ones() {
    let p = two_threads();
    commitment(&p, &["nodes=[T001]"]);
    dispose(&p, "deploy-friday", "satisfied");
    p.append_config("[checks]\naction_kinds = [\"run\", \"publish\"]\n");
    assert_eq!(gate(&p, "T001", "rollback:inc42"), Some(2), "an unknown kind is a usage error");
    let rev = p.rev("P01");
    assert_eq!(p.code(&["dispose", "P01", "--action", "rollback:1", "--disposition", "satisfied", "--because", "x", "--expect", &rev]), Some(2));
    assert_eq!(gate(&p, "T001", "run:fit-20261024"), Some(6), "a known kind gates as before");
    dispose(&p, "run:fit-20261024", "satisfied");
    assert_eq!(gate(&p, "T001", "run:fit-20261024"), Some(0));
    let out = p.ok(&["check"]);
    assert!(out.contains("warning") && out.contains("deploy-friday"), "the old review is a warning, not an error:\n{out}");
}

// ------------------------------------------------------------------------- dates

#[test]
fn dates_are_checked_for_form_and_warn_when_they_pass() {
    let p = two_threads();
    assert_eq!(p.set("T001", &["due=soon"]).status.code(), Some(4), "not YYYY-MM-DD");
    assert_eq!(p.set("T001", &["due=2026-02-30"]).status.code(), Some(4), "not a calendar date");
    assert!(p.set("T001", &["due=2000-01-01"]).status.success());
    assert!(p.set("T002", &["status=parked", "revisit_on=2000-01-01", "due=2999-12-31"]).status.success());
    let out = p.ok(&["check"]);
    assert!(out.contains("due 2000-01-01 has passed"), "{out}");
    assert!(out.contains("revisit_on 2000-01-01 has come"), "{out}");
    assert!(!out.contains("2999-12-31"), "{out}");
    assert!(out.contains("0 error(s)"), "dates never make errors:\n{out}");
    let v: serde_json::Value = serde_json::from_slice(&p.run(&["frontier", "--json"]).stdout).unwrap();
    let dates: Vec<(&str, &str, bool)> = v["result"]["dates"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| (d["id"].as_str().unwrap(), d["field"].as_str().unwrap(), d["passed"].as_bool().unwrap()))
        .collect();
    assert_eq!(dates, [("T001", "due", true), ("T002", "revisit_on", true), ("T002", "due", false)]);
    p.evidence("E001", "", "Done.");
    assert!(p.set("T001", &["status=closed", "outcome.ref=E001", "outcome.quote=Done."]).status.success());
    assert!(!p.ok(&["check"]).contains("2000-01-01 has passed"), "a resolved record's due date asks nothing");
}

// --------------------------------------------------------------- the revision cache

#[test]
fn pruning_the_cache_keeps_the_guard_evidence() {
    let p = two_threads();
    let stale = p.rev("T001");
    assert!(p.set("T001", &["actor=bob"]).status.success());
    let v: serde_json::Value = serde_json::from_slice(&p.run(&["gc", "--all", "--json"]).stdout).unwrap();
    assert!(v["result"]["removed"].as_u64().unwrap() > 0, "{v}");
    // conflicts are still caught, only without a diff
    let out = p.run(&["set", "T001", "actor=carol", "--expect", &stale]);
    assert_eq!(out.status.code(), Some(3));
    assert!(text(&out).contains("not in the cache"), "{}", text(&out));
    // the commit guard still recognises what plied wrote
    p.git(&["add", "-A"]);
    let out = p.run(&["guard", "pre-commit"]);
    assert!(out.status.success(), "{}", text(&out));
}

// ------------------------------------------------------------- §8 commitment scan reviews

/// `two_threads` plus a scanned source, `notes/N001.md`, whose body has a commitment phrase.
fn scanned_note() -> Proj {
    let p = two_threads();
    p.append_config("[[sources]]\nname = \"notes\"\nglob = \"notes/*.md\"\nid = \"^N[0-9]{3}$\"\nscan_commitments = true\n");
    std::fs::create_dir_all(p.path().join("notes")).unwrap();
    std::fs::write(p.path().join("notes/N001.md"), "---\nid: N001\ntitle: A note\n---\nThe rerun was deferred last week.\n").unwrap();
    p
}

/// The `check --json` diagnostics as (level, code, where, subject).
fn diagnostics(p: &Proj) -> Vec<(String, String, String, String)> {
    let v = envelope(&p.run(&["check", "--json"]));
    let s = |d: &serde_json::Value, k: &str| d[k].as_str().unwrap_or_default().to_string();
    v["result"]["diagnostics"].as_array().unwrap().iter().map(|d| (s(d, "level"), s(d, "code"), s(d, "where"), s(d, "subject"))).collect()
}

fn scan_warned(p: &Proj) -> bool {
    diagnostics(p).iter().any(|(l, c, w, _)| l == "warning" && c == "commitment?" && w == "N001")
}

fn scan(p: &Proj, rev: &str) -> std::process::Output {
    p.run(&["scanned", "N001", "--rev", rev, "--on", "T001", "--because", "describes the past", "--expect", &p.rev("T001")])
}

#[test]
fn a_scan_review_covers_the_revision_it_read() {
    let p = scanned_note();
    assert!(scan_warned(&p));
    let read = p.rev("N001");
    let out = scan(&p, &read[..8]);
    assert!(out.status.success(), "{}", text(&out));
    assert!(!scan_warned(&p), "a reviewed revision no longer warns");
    // the review is kept in full: who, when, why, and the whole revision
    let t = envelope(&p.run(&["show", "T001", "--json"]));
    let sc = &t["result"]["scanned"][0];
    assert_eq!((sc["record"].as_str(), sc["rev"].as_str()), (Some("N001"), Some(read.as_str())));
    assert!(sc["by"].as_str().unwrap().starts_with("alice") && sc["because"] == "describes the past" && sc["at"].is_string(), "{sc}");
    // backlinks say how the thread cites it
    assert!(p.ok(&["backlinks", "N001"]).contains("scanned"));

    // an edited record is scanned again; the warning keeps its identity and names the old review
    p.edit("notes/N001.md", "last week.", "last week, and is still pending.");
    assert!(scan_warned(&p));
    assert!(p.ok(&["check"]).contains("T001 records a scan of revision"));
    // a review of the revision read earlier is now a conflict
    let stale = scan(&p, &read);
    assert_eq!(stale.status.code(), Some(3), "{}", text(&stale));
    let mut args = vec!["scanned", "N001", "--rev", &read, "--on", "T001", "--because", "x"];
    let expect = p.rev("T001");
    args.extend(["--expect", &expect, "--json"]);
    let v = envelope(&p.run(&args));
    assert_eq!(v["error"]["detail"]["id"], "N001");
    assert_eq!(v["error"]["detail"]["current"], p.rev("N001").as_str());
    assert!(scan(&p, &p.rev("N001")).status.success());
    assert!(!scan_warned(&p));
}

#[test]
fn only_a_scanned_linked_record_is_reviewed_on_a_thread() {
    let p = scanned_note();
    let rev = p.rev("N001");
    p.evidence("E001", "", "Not scanned.");
    let e001 = p.rev("E001");
    let t002 = p.rev("T002");
    let cases: &[&[&str]] = &[
        // a plied record, and a source without scan_commitments
        &["scanned", "T002", "--rev", &t002, "--on", "T001"],
        &["scanned", "E001", "--rev", &e001, "--on", "T001"],
        // a record that does not exist, and somewhere that is not a thread
        &["scanned", "N999", "--rev", &rev, "--on", "T001"],
    ];
    for c in cases {
        let mut args = c.to_vec();
        let expect = p.rev("T001");
        args.extend(["--because", "x", "--expect", &expect]);
        assert_eq!(p.code(&args), Some(1), "{c:?}");
    }
    p.ok(&["new", "commitment", "--title", "Review first", "wording=Not scanned.", "made=E001"]);
    let p01 = p.rev("P01");
    assert_eq!(p.code(&["scanned", "N001", "--rev", &rev, "--on", "P01", "--because", "x", "--expect", &p01]), Some(1));
    // --rev is a revision
    assert_eq!(p.code(&["scanned", "N001", "--rev", "abc", "--on", "T001", "--because", "x", "--expect", &p.rev("T001")]), Some(2));
    assert!(scan_warned(&p));
}

#[test]
fn a_scan_review_by_hand_is_checked() {
    let p = scanned_note();
    let rev = p.rev("N001");
    let item = |record: &str, rev: &str| format!("status: open\nscanned:\n  - record: {record}\n    rev: {rev}\n    by: bob\n");
    let at = "plied/threads/T001-first.md";
    let has = |p: &Proj, code: &str, subject: &str| diagnostics(p).iter().any(|(l, c, w, s)| l == "error" && c == code && w == at && s == subject);

    p.edit(at, "status: open", &item("N001", &rev[..12]));
    assert!(!scan_warned(&p) && !diagnostics(&p).iter().any(|(l, ..)| l == "error"), "{:?}", diagnostics(&p));

    p.edit(at, &format!("rev: {}", &rev[..12]), "rev: ABCDEF12");
    assert!(has(&p, "field", "scanned[0].rev"), "{:?}", diagnostics(&p));
    assert!(scan_warned(&p), "an invalid revision covers nothing");

    p.edit(at, "record: N001\n    rev: ABCDEF12", &format!("record: N999\n    rev: {rev}"));
    assert!(has(&p, "ref", "scanned:N999"), "{:?}", diagnostics(&p));
    p.edit(at, "record: N999", "record: T002");
    assert!(has(&p, "ref", "scanned:T002"), "{:?}", diagnostics(&p));
    p.edit(at, "  - record: T002\n", "  - ");
    assert!(has(&p, "field", "scanned[0].record"), "{:?}", diagnostics(&p));
    p.edit(at, "    by: bob", "    by: bob\n    reason: typo");
    assert!(has(&p, "field", "scanned.reason"), "{:?}", diagnostics(&p));
}
