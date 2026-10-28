#!/usr/bin/env python3
"""Run a small first task in an independent disposable repository (Python 3.8+)."""
import argparse
import json
import os
from pathlib import Path
import shlex
import shutil
import subprocess
import tempfile
import uuid

from client import explanation, invoke


def replay(binary, destination):
    env = os.environ.copy()
    for name in ["PLIED_RUNTIME", "PLIED_CANONICAL", "CLAUDE_CODE_SESSION_ID", "GIT_DIR", "GIT_WORK_TREE", "GIT_COMMON_DIR"]:
        env.pop(name, None)
    env["PLIED_AGENT"] = "example-agent"
    env["PLIED_SESSION"] = str(uuid.uuid4())
    subprocess.run(["git", "init", "-q", "-b", "main", str(destination)], env=env, check=True)
    transcript = []

    def run(*args, expected=0):
        code, data = invoke(binary, destination, list(args), env)
        transcript.append({"args": list(args), "response": data})
        print(f"$ plied {shlex.join(args)}  [exit {code}]")
        if code != expected:
            raise AssertionError(f"expected {expected}, received {code}: {data}")
        if code:
            print(json.dumps(explanation(data), indent=2))
        return data.get("result", data.get("error"))

    run("init")
    with (destination / "plied.toml").open("a", encoding="utf-8") as config:
        config.write('\n[checks]\nactors = ["example-agent"]\naction_kinds = ["run"]\n')
    context = run("context")
    assert context["identity"]["session"] == env["PLIED_SESSION"]
    assert context["vocabularies"]["actors"] == ["example-agent"]
    run("schema", "thread")
    run("schema", "commitment")
    (destination / "task.md").write_text("Before running the parser, add a regression case for values containing equals signs.\n", encoding="utf-8")
    created = run("new", "thread", "--title", "Fix a parser", "--body", str(destination / "task.md"))
    node = created["id"]
    run("send", node, "Keep equals signs inside the value.", "--as", "mentor", "--to", "mentor")
    read = run("show", node)
    assert "Before running the parser" in read["text"]
    promise = run("new", "commitment", "--title", "Add the regression first", f"made={node}", f"nodes=[{node}]",
                  "wording=Before running the parser, add a regression case for values containing equals signs.")
    commitment = promise["id"]
    messages = run("messages", node)
    assert messages  # earlier messages remain on the node, even if not addressed to this agent
    run("review", node)
    run("claim", node)
    inbox = run("inbox", "--peek")
    assert not inbox["messages"]
    # This revision came with the text already read; no last-second revision lookup.
    run("set", node, "actor=example-agent", "next=1", "--expect", read["rev"], "--why", "taking the parser task")
    failure = run("note", node, "using stale input", "--expect", read["rev"], expected=3)
    assert failure["detail"]["id"] == node
    reconciled = run("show", node)
    run("note", node, "Reconciled the assignment change; the parser requirement still stands.", "--expect", reconciled["rev"])
    fresh = run("show", node)
    invalid = run("set", node, "status=not-a-state", "--expect", fresh["rev"], expected=4)
    assert invalid["detail"]["diagnostics"]

    # Fault injection only in this disposable fixture: an empty prepare journal
    # models a crash before commit. Never manufacture a journal to repair real work.
    intent = Path(context["runtime"]) / "intent.json"
    assert not intent.exists()
    intent.write_text(json.dumps({"version": 1, "state": "prepare", "files": []}), encoding="utf-8")
    failed_check = run("check", expected=1)
    assert failed_check["diagnostics"]
    run("recover")
    assert not intent.exists()
    # A canonical-branch blocker is another command failure with a JSON envelope.
    config_path = destination / "plied.toml"
    original_config = config_path.read_text(encoding="utf-8")
    try:
        config_path.write_text(original_config.replace('# canonical_branch = "main"', 'canonical_branch = "another-branch"'), encoding="utf-8")
        assert run("context")["write_blockers"]
        run("note", node, "must be blocked", "--expect", fresh["rev"], expected=5)
    finally:
        config_path.write_text(original_config, encoding="utf-8")

    action = "run:parser-" + uuid.uuid4().hex
    closed = run("review", node, "--action", action, "--gate", expected=6)
    assert closed["gate"]["passed"] is False and closed["gate"]["reasons"]
    run("review", "--action", action, "--gate", expected=2)
    # Fulfill the stated precondition before gating the consequential action.
    (destination / "regression.txt").write_text('input: key=a=b\nexpected: ["key", "a=b"]\n', encoding="utf-8")
    review = run("show", commitment)
    run("dispose", commitment, "--action", action, "--disposition", "satisfied",
        "--because", "regression.txt records key=a=b with expected value a=b; read before this run",
        "--expect", review["rev"])
    assert run("review", node, "--action", action, "--gate")["gate"]["passed"]
    assert "key=a=b".split("=", 1) == ["key", "a=b"]
    # A new action never borrows the previous action's disposition.
    run("review", node, "--action", "run:another-" + uuid.uuid4().hex, "--gate", expected=6)
    read = run("show", node)
    run("note", node, "The first-equals regression passed. Case and expected output are in regression.txt. Remaining: inspect empty keys and values; no parser process is left running.", "--expect", read["rev"])
    read = run("show", node)
    run("set", node, "status=held", "next=", "--expect", read["rev"], "--why", "hand off the remaining edge cases")
    checked = run("check")
    assert checked["errors"] == checked["warnings"] == 0, checked
    # A second session of the same agent is a fixture for release isolation.
    other = "other-" + uuid.uuid4().hex
    run("claim", node, "--session", other)
    assert run("release", "--this-session")["released"] == 1
    claims = run("who", node)
    assert len(claims) == 1 and claims[0]["session"] == other
    (destination / "transcript.json").write_text(json.dumps(transcript, indent=2) + "\n", encoding="utf-8")
    print("PASS: checked update, exits 1–6, recovery, gate, handoff and isolated release; 0 errors, 0 warnings.")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    source = parser.add_mutually_exclusive_group()
    source.add_argument("--plied", help="installed binary or checkout binary path")
    source.add_argument("--source", action="store_true", help="build and use this checkout's binary")
    parser.add_argument("--keep", type=Path, help="keep the disposable repository and transcript in a NEW directory")
    args = parser.parse_args()
    if args.source:
        root = Path(__file__).resolve().parents[2]
        subprocess.run(["cargo", "build", "--locked"], cwd=root, check=True)
        binary = root / "target/debug" / ("plied.exe" if os.name == "nt" else "plied")
    else:
        selected = args.plied or shutil.which("plied")
        if not selected:
            parser.error("no installed plied found; use --plied PATH or --source")
        binary = Path(shutil.which(selected) or selected).resolve()
    if args.keep:
        destination = args.keep.resolve()
        destination.mkdir(parents=True, exist_ok=False)
        replay(binary, destination)
        print(f"Kept example and transcript at {destination}")
    else:
        with tempfile.TemporaryDirectory(prefix="plied-first-task-") as directory:
            replay(binary, Path(directory))


if __name__ == "__main__":
    main()
