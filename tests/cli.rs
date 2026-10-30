//! End-to-end tests of the `plied` binary against FORMAT.md.

mod common;

use std::path::PathBuf;

use common::*;

#[test]
fn quickstart_conflict_and_view() {
    let p = Proj::new();
    let out = p.ok(&["new", "thread", "--title", "Calibrate the walking model", "next=1", "actor=alice"]);
    assert!(out.contains("created T001"), "{out}");
    p.ok(&["new", "thread", "--title", "Held-out test", "status=held"]);
    let tree = p.ok(&["tree"]);
    assert!(tree.contains("T001"), "{tree}");
    assert!(p.ok(&["check"]).contains("0 error(s)"));
    assert!(p.run(&["view", "--check"]).status.success());

    let rev = p.rev("T001");
    p.ok(&["set", "T001", "actor=bob", "--expect", &rev, "--why", "handoff"]);
    // the same rev again is a conflict (exit 3)
    let stale = p.run(&["set", "T001", "actor=carol", "--expect", &rev]);
    assert_eq!(stale.status.code(), Some(3), "{}", text(&stale));
    assert!(text(&stale).contains("conflict"));
    // an invalid change is refused (exit 4) and changes nothing
    let rev = p.rev("T001");
    let bad = p.run(&["set", "T001", "status=bogus", "--expect", &rev]);
    assert_eq!(bad.status.code(), Some(4), "{}", text(&bad));
    let show = p.ok(&["show", "T001"]);
    assert!(show.contains("actor: bob"), "{show}");
}

#[test]
fn messages_review_gate_and_search() {
    let p = Proj::new();
    p.ok(&["new", "thread", "--title", "Calibrate the walking model", "next=1", "actor=alice"]);
    p.ok(&["claim", "T001"]);
    let sent = p.run_as("bob", &["send", "T001", "please hold", "--id", "MSG1"]);
    assert!(sent.status.success(), "{}", text(&sent));
    // a retry with the same id is recorded once
    let again = p.run_as("bob", &["send", "T001", "please hold", "--id", "MSG1"]);
    assert!(text(&again).contains("already recorded"), "{}", text(&again));
    let inbox = p.ok(&["inbox"]);
    assert!(inbox.contains("please hold"), "{inbox}");
    p.ok(&["ack", "MSG1"]);
    assert!(p.ok(&["inbox"]).contains("inbox empty"));

    p.ok(&["new", "commitment", "--title", "Check first", "wording=Calibrate the walking model", "made=T001", "nodes=[T001]"]);
    let gate = p.run(&["review", "T001", "--action", "run:1", "--gate"]);
    assert_eq!(gate.status.code(), Some(6), "{}", text(&gate));
    let rev = p.rev("P01");
    p.ok(&["dispose", "P01", "--action", "run:1", "--disposition", "satisfied", "--because", "checked", "--expect", &rev]);
    assert!(p.run(&["review", "T001", "--action", "run:1", "--gate"]).status.success());
    // --gate without --action is a usage error
    assert_eq!(p.run(&["review", "T001", "--gate"]).status.code(), Some(2));

    p.ok(&["index"]);
    let hits = p.ok(&["search", "walking"]);
    assert!(hits.contains("T001"), "{hits}");
    let found = p.ok(&["find", "{\"kind\": \"thread\", \"status\": \"open\"}"]);
    assert!(found.contains("T001"), "{found}");
    assert_eq!(p.run(&["find", "not json"]).status.code(), Some(2));
}

#[test]
fn claim_and_show_list_the_commitments_that_apply() {
    let p = Proj::new();
    p.ok(&["new", "thread", "--title", "Calibrate the walking model"]);
    p.ok(&["new", "thread", "--title", "Fit", "uses=[T001]"]);
    p.ok(&["new", "thread", "--title", "Unrelated"]);
    p.ok(&["new", "commitment", "--title", "Check first", "wording=Calibrate the walking model", "made=T001", "nodes=[T001]"]);

    // a dependency's commitment applies (§12 rule 2), and claiming says so before any work starts
    let out = p.ok(&["claim", "T002"]);
    assert!(out.contains("Commitments that apply") && out.contains("P01 [open] Check first"), "{out}");
    assert!(out.contains("plied review T002"), "{out}");
    let v: serde_json::Value = serde_json::from_str(&p.ok(&["claim", "T002", "--json"])).unwrap();
    let c = &v["result"]["commitments"][0];
    assert_eq!((c["id"].as_str(), c["state"].as_str(), c["title"].as_str()), (Some("P01"), Some("open"), Some("Check first")));
    assert_eq!(c["rev"], p.rev("P01"));
    assert_eq!(c["chains"][0], "T002 uses T001 ← P01");
    assert_eq!(v["result"]["node"], "T002", "the claim's own fields are kept");

    // nothing applies: the list is empty, and the text says nothing about commitments
    assert!(!p.ok(&["claim", "T003"]).contains("Commitments"));
    let v: serde_json::Value = serde_json::from_str(&p.ok(&["claim", "T003", "--json"])).unwrap();
    assert_eq!(v["result"]["commitments"], serde_json::json!([]));
    // a commitment is not a node commitments apply to
    let v: serde_json::Value = serde_json::from_str(&p.ok(&["claim", "P01", "--json"])).unwrap();
    assert_eq!(v["result"]["commitments"], serde_json::json!([]));

    // `show --json` carries the same list that `show` prints, for threads and questions only
    let v: serde_json::Value = serde_json::from_str(&p.ok(&["show", "T002", "--json"])).unwrap();
    assert_eq!(v["result"]["commitments"][0]["id"], "P01");
    let v: serde_json::Value = serde_json::from_str(&p.ok(&["show", "P01", "--json"])).unwrap();
    assert!(v["result"].get("commitments").is_none(), "{v}");
    // a resolved commitment no longer applies
    let rev = p.rev("P01");
    p.ok(&["set", "P01", "state=not-triggered", "resolved_by=T001", "--expect", &rev]);
    let v: serde_json::Value = serde_json::from_str(&p.ok(&["show", "T002", "--json"])).unwrap();
    assert_eq!(v["result"]["commitments"], serde_json::json!([]));
}

#[test]
fn claim_says_who_else_is_on_the_node() {
    let p = Proj::new();
    p.ok(&["new", "thread", "--title", "Shared work"]);
    let v: serde_json::Value = serde_json::from_str(&p.ok(&["claim", "T001", "--json"])).unwrap();
    assert_eq!(v["result"]["others"], serde_json::json!([]), "nobody else yet");
    let out = p.run_as("bob", &["claim", "T001"]);
    let said = text(&out);
    assert!(said.contains("Also on T001") && said.contains("alice (session sess1234)"), "{said}");
    assert!(said.contains(&format!("lease until {}", chrono::Utc::now().format("%Y-%m-%d"))), "a lease shows its date:\n{said}");
    let v: serde_json::Value = serde_json::from_str(&p.ok(&["claim", "T001", "--json"])).unwrap();
    let others = v["result"]["others"].as_array().unwrap();
    assert_eq!(others.len(), 1, "renewing your own claim does not list you: {v}");
    assert_eq!((others[0]["agent"].as_str(), others[0]["node"].as_str()), (Some("bob"), Some("T001")));
}

#[test]
fn next_write_undoes_an_interrupted_one() {
    let p = Proj::new();
    p.ok(&["new", "thread", "--title", "First"]);
    // what a crash before the commit point leaves: an intent in `prepare` and a temporary file
    let tmp = p.path().join("plied/threads/.T001-first.md.dead.tmp");
    std::fs::write(&tmp, "half-written").unwrap();
    let intent = p.path().join(".git/plied/intent.json");
    let entry = serde_json::json!({"path": p.path().join("plied/threads/T001-first.md"), "tmp": tmp, "prev": null, "sha": "0"});
    std::fs::write(&intent, serde_json::json!({"version": 1, "state": "prepare", "files": [entry]}).to_string()).unwrap();
    let out = p.run(&["new", "thread", "--title", "Second"]);
    assert!(out.status.success(), "{}", text(&out));
    assert!(text(&out).contains("undid an interrupted write"), "{}", text(&out));
    assert!(!tmp.exists() && !intent.exists());
    assert!(p.ok(&["check"]).contains("0 error(s)"));
}

/// Leave the state a crash after the commit point would: an intent in `commit`, the temporary
/// file written, the target not yet replaced.
fn interrupted_commit(p: &Proj, rel: &str, new: &str) -> (PathBuf, PathBuf) {
    let target = p.path().join(rel);
    let tmp = target.with_file_name(".interrupted.tmp");
    std::fs::write(&tmp, new).unwrap();
    let prev = sha(&std::fs::read(&target).unwrap());
    let entry = serde_json::json!({"path": target, "tmp": tmp, "prev": prev, "sha": sha(new.as_bytes())});
    let intent = p.path().join(".git/plied/intent.json");
    std::fs::write(&intent, serde_json::json!({"version": 1, "state": "commit", "files": [entry]}).to_string()).unwrap();
    (target, intent)
}

#[test]
fn a_pending_intent_is_reported_refused_and_recovered() {
    let p = Proj::new();
    p.ok(&["new", "thread", "--title", "First"]);
    let rel = "plied/threads/T001-first.md";
    let old = std::fs::read_to_string(p.path().join(rel)).unwrap();
    let new = old.replace("title: First", "title: Renamed");
    let (target, intent) = interrupted_commit(&p, rel, &new);

    // reads say the state may be mixed; check fails; the commit guard refuses
    assert!(text(&p.run(&["show", "T001"])).contains("may mix old and new"));
    let check = p.run(&["check"]);
    assert_eq!(check.status.code(), Some(1), "{}", text(&check));
    assert!(text(&check).contains("plied recover"), "{}", text(&check));
    assert_eq!(p.run(&["guard", "pre-commit"]).status.code(), Some(5));
    // message appends don't cover record files, so they neither recover nor wait (FORMAT.md §11)
    assert!(p.run(&["send", "T001", "hello", "--to", "bob"]).status.success());
    assert!(intent.exists());

    let out = p.ok(&["recover"]);
    assert!(out.contains("finished an interrupted write"), "{out}");
    assert_eq!(std::fs::read_to_string(&target).unwrap(), new);
    assert!(!intent.exists());
    assert!(p.ok(&["recover"]).contains("nothing to recover"));
    assert!(p.ok(&["check"]).contains("0 error(s)"));
    assert!(p.run(&["guard", "pre-commit"]).status.success());
}

#[test]
fn recovery_is_reported_apart_from_a_refused_write() {
    let p = Proj::new();
    p.ok(&["new", "thread", "--title", "First"]);
    let stale = p.rev("T001");
    let rel = "plied/threads/T001-first.md";
    let new = std::fs::read_to_string(p.path().join(rel)).unwrap().replace("title: First", "title: Renamed");
    let (target, _) = interrupted_commit(&p, rel, &new);
    // the earlier write is finished, then this one is refused as stale: two outcomes, both said
    let out = p.run(&["set", "T001", "actor=bob", "--expect", &stale]);
    assert_eq!(out.status.code(), Some(3), "{}", text(&out));
    assert!(text(&out).contains("before this command: finished an interrupted write"), "{}", text(&out));
    assert_eq!(std::fs::read_to_string(&target).unwrap(), new);
}

#[test]
fn an_unreadable_intent_refuses_writes() {
    let p = Proj::new();
    p.ok(&["new", "thread", "--title", "First"]);
    // a directory where the intent should be: it exists but cannot be read as one
    std::fs::create_dir_all(p.path().join(".git/plied/intent.json")).unwrap();
    let out = p.run(&["new", "thread", "--title", "Second"]);
    assert_eq!(out.status.code(), Some(5), "{}", text(&out));
    assert!(!p.path().join("plied/threads/T002-second.md").exists());
}

#[test]
fn check_reports_broken_records_and_literature() {
    let p = Proj::new();
    p.ok(&["new", "thread", "--title", "A thread"]);
    // break the record by hand: check must fail (exit 1)
    let f = p.path().join("plied/threads/T001-a-thread.md");
    let mut t = std::fs::read_to_string(&f).unwrap();
    t = t.replace("status: open", "status: open\nwaiting_on: [T999]\nunknownfield: x");
    std::fs::write(&f, t).unwrap();
    let out = p.run(&["check"]);
    assert_eq!(out.status.code(), Some(1), "{}", text(&out));
    assert!(text(&out).contains("T999"), "{}", text(&out));

    // literature: a citation whose quote is not on the page fails lit check
    let p2 = Proj::new();
    std::fs::create_dir_all(p2.path().join("papers")).unwrap();
    std::fs::create_dir_all(p2.path().join("cache/Smith2020")).unwrap();
    std::fs::write(p2.path().join("papers/Smith2020.pdf"), b"%PDF-fake").unwrap();
    std::fs::write(p2.path().join("cache/Smith2020/p01.txt"), "The mean bout lasted 12.5 s.\n").unwrap();
    let sha = sha(b"%PDF-fake");
    std::fs::write(
        p2.path().join("literature.json"),
        format!("{{\"papers\": [{{\"id\": \"Smith2020\", \"file\": \"Smith2020.pdf\", \"sha256\": \"{sha}\"}}]}}"),
    )
    .unwrap();
    let mut toml = std::fs::read_to_string(p2.path().join("plied.toml")).unwrap();
    toml += "\n[literature]\nmanifest = \"literature.json\"\npdf_dir = \"papers\"\ntext_dir = \"cache/{id}\"\n";
    std::fs::write(p2.path().join("plied.toml"), toml).unwrap();
    p2.ok(&["new", "thread", "--title", "Citing thread"]);
    let f = p2.path().join("plied/threads/T001-citing-thread.md");
    let mut t = std::fs::read_to_string(&f).unwrap();
    t = t.replace(
        "status: open",
        "status: open\ncites:\n  - paper: Smith2020\n    page: 1\n    quote: \"The mean bout lasted 12.5 s\"",
    );
    std::fs::write(&f, &t).unwrap();
    // a hand edit makes the view stale; regenerate through plied by a note instead
    let rev = p2.rev("T001");
    p2.ok(&["note", "T001", "cite added", "--expect", &rev]);
    assert!(p2.run(&["lit", "check"]).status.success(), "{}", text(&p2.run(&["lit", "check"])));
    assert!(p2.run(&["check"]).status.success(), "{}", text(&p2.run(&["check"])));
}

#[test]
fn skill_is_generated_by_the_binary_outside_a_repo() {
    use std::process::Command;
    let dir = tempfile::tempdir().unwrap();
    let run = |args: &[&str]| Command::new(bin()).current_dir(dir.path()).args(args).output().unwrap();

    let out = run(&["skill"]);
    assert!(out.status.success(), "{}", text(&out));
    let skill = String::from_utf8(out.stdout).unwrap();
    assert!(skill.starts_with("---\nname: plied\n"), "{skill}");
    assert!(skill.contains(&format!("plied {}", env!("CARGO_PKG_VERSION"))), "generated for this version:\n{skill}");
    for needle in [
        "result.rev",          // the show -> --expect cycle, read from the JSON envelope
        "PLIED_SESSION",      // session-scoped handoff begins with a session
        "plied context --json",
        "plied schema thread --json",
        "plied messages \"$NODE\"",
        "--expect <rev>",      // every write names the revision it read
        "re-read",             // batch writes chain revs, never reuse one
        "nodes=[T001, T002]",  // list assignment on `new`
        "options+=",           // prose list items one by one, so a comma never splits them
        "normalised quotation",
        "…",
        "resolved_by", // a waived commitment needs an authority-checked resolver
        "plied scanned", // scan triage: record the negative finding
        "stale",       // a wide-citing entry makes linked nodes stale
        "note each",   // ... and each one gets its review noted
        "rubber stamp", // ... saying what was seen, not only that it was looked at
        "names it directly", // a resolved record is stale only when named, and may be reopened
        "Never edit a linked record", // an id-shaped name is noted, not reworded away
        "only the nodes you name",    // the gate sees nothing it is not given
        "not yet an answer",          // an answer heard in chat still needs a record
        "plied review", // the command inventory is part of the text
        "plied dispose",
        "status=closed",  // closing sets the outcome in the same write
        "release --this-session",
        "not at hand-off",           // parked work is noted when it is parked
        "note what is unfinished",   // ... and again before release
        "\"before any run of X\"", // a commitment's wording, not the action's size, decides
        "plied review <node>` (no `--gate`)", // before a step of an inherited plan
        "twenty reads",              // stale review in a script: one read per node
        "lists the commitments that apply", // claim shows what a node obliges
        "state=superseded superseded_by=", // a replaced commitment is superseded, not waived
        "one commitment per clause",
        "governs=[report]",               // a commitment can be confined to action kinds
        "still lists the commitments it leaves out",
        "subject `named`",                // named stale nodes come first
        "cite-null:run-01",              // review controls for the proposed citation
        "not that the control passed",   // the gate does not evaluate measurements
    ] {
        assert!(skill.contains(needle), "skill is missing {needle:?}:\n{skill}");
    }

    // under --json the skill travels in the envelope like every other result
    let out = run(&["skill", "--json"]);
    assert!(out.status.success(), "{}", text(&out));
    assert!(out.stderr.is_empty(), "{}", text(&out));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["ok"], true);
    assert_eq!(v["result"]["version"], env!("CARGO_PKG_VERSION"));
    assert!(v["result"]["skill"].as_str().unwrap().contains("result.rev"));

    // no command at all is still a usage error, as before
    assert_eq!(run(&[]).status.code(), Some(2));
}

#[test]
fn installed_help_provides_a_task_map_and_all_five_citation_aspects() {
    let out = std::process::Command::new(bin()).arg("--help").output().unwrap();
    let help = text(&out);
    assert!(out.status.success());
    for task in ["Orient", "Update", "Review", "Communicate", "Recover", "context", "schema"] {
        assert!(help.contains(task), "{help}");
    }
    let out = std::process::Command::new(bin()).args(["lit", "--help"]).output().unwrap();
    assert!(text(&out).contains("quotation and image claim"), "{}", text(&out));
}

#[test]
fn skill_control_example_blocks_a_null_until_the_control_is_reviewed() {
    let p = Proj::new();
    p.with_evidence();
    p.append_config("[checks]\naction_kinds = [\"cite-null\"]\n");
    p.evidence("D001", "", "Before citing a null, recover the known positive and verify the delivered dose");
    p.evidence("E002", "", "The detector missed the known positive. Delivered dose was not measured.");
    p.ok(&["new", "thread", "--title", "Experiment"]);
    p.ok(&[
        "new", "commitment", "--title", "Controls before citing a null", "made=D001",
        "wording=Before citing a null, recover the known positive and verify the delivered dose", "nodes=[T001]",
    ]);
    assert_eq!(p.code(&["review", "T001", "--action", "cite-null:run-01", "--gate"]), Some(6));
    let rev = p.rev("P01");
    p.ok(&[
        "dispose", "P01", "--action", "cite-null:run-01", "--disposition", "blocks",
        "--because", "The detector missed the known positive; this null cannot support the claim",
        "--ref", "E002", "--expect", &rev,
    ]);
    assert_eq!(p.code(&["review", "T001", "--action", "cite-null:run-01", "--gate"]), Some(6));
    p.evidence("E003", "", "Fresh run: known positive recovered and delivered dose met the plan's criteria.");
    let rev = p.rev("P01");
    p.ok(&[
        "dispose", "P01", "--action", "cite-null:run-02", "--disposition", "satisfied",
        "--because", "Read E003 against D001: known positive recovered and delivered dose within tolerance",
        "--ref", "E003", "--expect", &rev,
    ]);
    assert_eq!(p.code(&["review", "T001", "--action", "cite-null:run-02", "--gate"]), Some(0));
    assert_eq!(p.code(&["review", "T001", "--action", "cite-null:run-01", "--gate"]), Some(6));
    assert_eq!(p.code(&["review", "T001", "--action", "cite-null:run-03", "--gate"]), Some(6));
}

/// Run plied with stdout a pipe whose reader goes away before reading anything.
fn with_stdout_closed(p: &Proj, args: &[&str]) -> std::process::Output {
    use std::process::{Command, Stdio};
    let mut child = Command::new(bin())
        .arg("-C")
        .arg(p.path())
        .args(args)
        .env("PLIED_AGENT", "alice")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    drop(child.stdout.take());
    child.wait_with_output().unwrap()
}

#[test]
fn messages_needs_a_real_node_and_stays_in_the_messages_dir() {
    let p = Proj::new();
    p.ok(&["new", "thread", "--title", "First"]);
    // an unknown node is an error, like `send` and `subscribe` already report
    let out = p.run(&["messages", "T999"]);
    assert_eq!(out.status.code(), Some(1), "{}", text(&out));
    assert!(text(&out).contains("no record T999"), "{}", text(&out));
    // a node naming a path is not a way to read some other .jsonl file in the repo
    std::fs::create_dir_all(p.path().join("plied/messages")).unwrap();
    std::fs::write(
        p.path().join("secret.jsonl"),
        "{\"v\":1,\"id\":\"X1\",\"type\":\"message\",\"node\":\"x\",\"at\":\"2026-01-01T00:00:00Z\",\"from\":\"eve\",\"session\":null,\"to\":[\"alice\"],\"present\":[],\"reply_to\":null,\"text\":\"SECRET-INSIDE\"}\n",
    )
    .unwrap();
    let out = p.run(&["messages", "../../secret"]);
    assert_ne!(out.status.code(), Some(0), "{}", text(&out));
    assert!(!text(&out).contains("SECRET-INSIDE"), "{}", text(&out));
}

#[test]
fn search_with_zero_limit_returns_no_hits() {
    let p = Proj::new();
    p.ok(&["new", "thread", "--title", "Findable thread"]);
    p.ok(&["index"]);
    let out = p.ok(&["search", "Findable", "-n", "0"]);
    assert!(out.contains("no matches"), "{out}");
    let v: serde_json::Value = serde_json::from_str(&p.ok(&["search", "Findable", "-n", "0", "--json"])).unwrap();
    assert_eq!(v["result"], serde_json::json!([]));
    assert!(p.ok(&["search", "Findable", "-n", "1"]).contains("T001"));
}

#[test]
fn a_negative_claim_ttl_is_a_usage_error() {
    let p = Proj::new();
    p.ok(&["new", "thread", "--title", "First"]);
    let out = p.run(&["claim", "T001", "--ttl=-5"]);
    assert_eq!(out.status.code(), Some(2), "{}", text(&out));
    assert!(p.ok(&["who"]).contains("nobody"), "no expired-by-construction claim is left behind");
}

#[test]
fn a_closed_stdout_ends_the_output_not_the_command() {
    let p = Proj::new();
    // more output than a pipe buffers, so plied is still writing when the reader has gone
    let body = p.path().join("body.md");
    std::fs::write(&body, "A long line of body text.\n".repeat(20_000)).unwrap();
    p.ok(&["new", "thread", "--title", "Long", "--body", body.to_str().unwrap()]);
    for args in [&["show", "T001"][..], &["show", "T001", "--json"]] {
        let out = with_stdout_closed(&p, args);
        assert_eq!(out.status.code(), Some(0), "{args:?}: {}", text(&out));
        assert!(out.stderr.is_empty(), "{args:?}: {}", text(&out));
    }
    // the exit code still reports the command's outcome: a failure stays a failure...
    assert_eq!(with_stdout_closed(&p, &["show", "T999", "--json"]).status.code(), Some(1));
    // ...and an accepted write is never reported as failed because nobody read its result
    let out = with_stdout_closed(&p, &["new", "thread", "--title", "Second"]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    assert!(p.ok(&["list"]).contains("T002"));
}
