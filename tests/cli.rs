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
