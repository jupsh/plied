#!/usr/bin/env python3
"""Replay the synthetic research workflow with an installed/built plied binary."""

import argparse
import json
import os
from pathlib import Path
import shlex
import shutil
import subprocess
import tempfile


def replay(binary, destination):
    destination.mkdir(parents=True, exist_ok=False)
    subprocess.run(["git", "init", "-q", "-b", "main", str(destination)], check=True)
    env = {key: value for key, value in os.environ.items() if not key.startswith("PLIED_")}
    env.update(PLIED_AGENT="example-reviewer", PLIED_SESSION="research-example")

    def run(*args, expected=0):
        print("plied " + shlex.join(args))
        out = subprocess.run(
            [binary, "-C", str(destination), "--json", *args],
            capture_output=True, text=True, check=False, env=env,
        )
        if out.returncode != expected:
            raise RuntimeError(f"expected exit {expected}, got {out.returncode}: {out.stdout}\n{out.stderr}")
        result = json.loads(out.stdout)
        assert result["exit"] == expected
        return result.get("result")

    def change(command, record, *args):
        # Read and display the text whose rev is used; never obtain a rev separately.
        current = run("show", record)
        print(current["text"])
        return run(command, record, *args, "--expect", current["rev"])

    def evidence(filename):
        shutil.copyfile(Path(__file__).parent / "store" / "evidence" / filename, destination / "evidence" / filename)

    def gate(node, action, expected):
        run("review", node, "--action", action, "--gate", expected=expected)

    def dispose(record, action, disposition, reason, ref):
        change("dispose", record, "--action", action, "--disposition", disposition, "--because", reason, "--ref", ref)

    run("init")
    with (destination / "plied.toml").open("a") as config:
        config.write('\n[[sources]]\nname = "evidence"\nglob = "evidence/*.md"\nid = "^E[0-9]{3}$"\n')
        config.write('\n[checks]\naction_kinds = ["interpret", "close"]\n')
    (destination / "evidence").mkdir()
    evidence("E001-plan.md")
    run("new", "thread", "--title", "Recover and compare", "actor=example-reviewer", "next=1")
    run("new", "thread", "--title", "Interpret against the plan", "uses=[T001]")
    run("new", "thread", "--title", "Replicate the comparison", "uses=[T001]")
    run("new", "commitment", "--title", "Check controls", "made=E001", "nodes=[T001]", "governs=[interpret]",
        "wording=Before interpreting a comparison, verify a known positive and delivered dose against this plan.")
    run("new", "commitment", "--title", "Replicate before closure", "made=E001", "nodes=[T001]", "governs=[close]",
        "wording=Before closing the study, record one replication against the same criteria.")

    evidence("E002-interruption.md")
    change("note", "T001", "Run 01 interrupted; partial observations retained in E002. Restart as run 02 with a replacement sensor; do not pool runs.")
    dispose("P01", "interpret:run-01", "blocks", "E002 has no delivered-dose measurement and only partial observations", "E002")
    gate("T002", "interpret:run-01", 6)

    evidence("E003-recovery.md")
    change("note", "T001", "Recovery recorded in E003: replacement sensor, known positive 6, dose 10; partial run 01 remains excluded.")
    gate("T002", "interpret:run-02", 6)
    dispose("P01", "interpret:run-02", "satisfied", "E003 positive 6 >= 5 and dose 10 within 9..11 meet E001; run 01 is excluded from the completed comparison", "E003")
    gate("T002", "interpret:run-02", 0)
    evidence("E004-comparison.md")
    run("new", "question", "--title", "Does this generalise beyond these conditions?", "raised=E004", "nodes=[T001, T002, T003]")
    change("set", "T002", "status=closed", "outcome.ref=E004", "carries=[Q01]",
           "outcome.quote=The declared effect criterion was not met in run 02; this does not establish absence of an effect elsewhere.")
    gate("T001", "close:study-01", 6)

    evidence("E005-replication.md")
    gate("T003", "interpret:run-03", 6)
    dispose("P01", "interpret:run-03", "satisfied", "E005 positive 6.5 >= 5 and dose 9.5 within 9..11 meet E001; difference is 0.2", "E005")
    gate("T003", "interpret:run-03", 0)
    evidence("E006-closure.md")
    change("set", "T003", "status=closed", "outcome.ref=E006", "carries=[Q01]",
           "outcome.quote=Run 03 also did not meet the declared effect criterion.")
    dispose("P02", "close:study-01", "satisfied", "E005 records run 03 against the same E001 criteria; generalisability is still Q01", "E005")
    gate("T001", "close:study-01", 0)
    change("set", "T001", "status=closed", "next=", "outcome.ref=E006", "carries=[Q01]",
           "outcome.quote=Two completed comparisons did not meet the declared effect criterion; generalisability remains unresolved.")
    checked = run("check")
    assert checked["errors"] == 0 and checked["warnings"] == 0, checked

    # Demonstrate a limit in a disposable replay, then restore the evidence exactly.
    path = destination / "evidence" / "E003-recovery.md"
    original = path.read_bytes()
    try:
        path.write_text("---\nid: E003\ntitle: Changed evidence\n---\nThe known positive was missed.\n")
        gate("T002", "interpret:run-02", 0)
        print("LIMIT: the gate still passes after reviewed evidence changes.")
    finally:
        path.write_bytes(original)
    run("check")
    print(f"Example store: {destination}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--plied", default="plied", help="installed binary name or absolute path")
    parser.add_argument("--output", type=Path, help="new directory for the example (must not already exist)")
    args = parser.parse_args()
    binary = shutil.which(args.plied)
    if binary is None:
        parser.error("plied executable not found; build it or pass --plied /absolute/path/to/plied")
    output = args.output or Path(tempfile.mkdtemp(prefix="plied-research-")) / "store"
    replay(str(Path(binary).resolve()), output.resolve())
