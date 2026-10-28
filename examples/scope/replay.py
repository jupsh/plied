#!/usr/bin/env python3
"""Exercise scope-aware records, not an agent's ability to judge scope.

All decisions are authored fixtures. No model calls or network requests occur.
Requires git, Python 3 and the Plied binary built from this checkout.
"""
import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile


def replay(binary, dest):
    dest.mkdir(parents=True, exist_ok=False)
    subprocess.run(["git", "init", "-q", "-b", "main", str(dest)], check=True)
    env = {k: v for k, v in os.environ.items() if not k.startswith("PLIED_")}
    env.update(PLIED_AGENT="scope-reviewer", PLIED_SESSION="scope-example")
    trace = []

    def run(*args, expected=0):
        proc = subprocess.run([binary, "-C", str(dest), "--json", *args],
                              env=env, capture_output=True, text=True)
        envelope = json.loads(proc.stdout)
        trace.append({"args": list(args), "expected": expected, "actual": proc.returncode,
                      "envelope": envelope})
        if proc.returncode != expected:
            raise RuntimeError(f"{args}: expected {expected}, got {proc.returncode}: {proc.stdout}")
        return envelope.get("result", {})

    def change(command, node, *args):
        read = run("show", node)
        # Display the exact text associated with the revision used for the write.
        print(read["text"])
        return run(command, node, *args, "--expect", read["rev"])

    def evidence(name, title, body):
        path = dest / "evidence" / f"{name}.md"
        path.write_text(f"---\nid: {name}\ntitle: {title}\n---\n\n{body}\n")

    def bodyfile(name, body):
        path = dest / f"{name}.md"
        path.write_text(body)
        return str(path)

    run("init")
    config_path = dest / "plied.toml"
    config_path.write_text(config_path.read_text().replace(
        'include = ["plied/**/*.md"]', 'include = ["plied/**/*.md", "evidence/*.md"]'))
    with (dest / "plied.toml").open("a") as f:
        f.write('\n[[sources]]\nname = "evidence"\nglob = "evidence/*.md"\nid = "^E[0-9]{3}$"\n')
        f.write('\n[checks]\naction_kinds = ["decide"]\n')
    (dest / "evidence").mkdir()
    evidence("E001", "A deliberately elementary scope fixture", """Object: x^2 + y^2 = 3 over integer x and y.

Assumptions: x and y are integers. Integer squares are 0 or 1 modulo 4;
their sum cannot be 3 modulo 4.

Conclusion: No integer pair satisfies x^2 + y^2 = 3.

Scope: Renaming variables and restricting the integer domain do not escape
this exclusion. It makes no exclusion claim over real variables.

This is an elementary demonstration, not a new mathematical result.""")
    evidence("E002", "Explicit demonstration protocol", """Before deciding whether a prior result settles a proposed task, record its object mapping, required assumptions, evidence reference and next step.

This is an explicit protocol commitment for this demonstration. A heuristic
such as 'avoid another parameter sweep by default' should remain prose unless
the project actually adopts it as an obligation.""")
    source = run("new", "thread", "--title", "Exclude integer solutions",
                 "links=[E001]", "revisit=Consider a distinct task if the domain or equation changes")['id']
    change("set", source, "status=closed", "outcome.ref=E001",
           "outcome.quote=No integer pair satisfies x^2 + y^2 = 3.")
    original = run("show", source)
    commitment = run("new", "commitment", "--title", "Record the scope comparison",
                     "made=E002", "applies=project", "governs=[decide]",
                     "wording=Before deciding whether a prior result settles a proposed task, record its object mapping, required assumptions, evidence reference and next step.")['id']
    cases = [
        ("alias", "Same equation under different names",
         "Object mapping: u maps to x, v maps to y; u^2 + v^2 = 3.\n"
         "Assumptions: u and v are integers, matching E001.\n"
         "Evidence: E001.\nConclusion: E001 applies after renaming.\n"
         "Next step: reuse the proof; no repeated search is needed.",
         "Reuse the integer exclusion after renaming; do not repeat the search.", True),
        ("subset", "Integer search restricted to even first coordinate",
         "Object mapping: the first coordinate is x=2k, with integer k; y remains integer.\n"
         "Assumptions: both coordinates are still integers; this is a subset of E001.\n"
         "Evidence: E001.\nConclusion: the restriction does not remove the old premise.\n"
         "Next step: reuse the exclusion; extra restriction is not an escape.",
         "Reuse the integer exclusion for the narrower subset; do not repeat the search.", True),
        ("changed", "Investigate the equation over real variables",
         "Object mapping: the equation is unchanged, but the domain is real, not integer.\n"
         "Assumptions: E001's integer-domain premise is absent.\n"
         "Evidence: E001 supplies no real-domain exclusion; (sqrt(3),0) is a real witness.\n"
         "Conclusion: E001 remains valid for integers and does not settle this new task.\n"
         "Next step: pursue the real-domain task; retain the old integer result unchanged.",
         "Pursue the real-domain task without withdrawing the integer result.", False),
    ]
    outcomes = []
    for index, (name, title, mapping, decision, close) in enumerate(cases, start=3):
        node = run("new", "thread", "--title", title, f"uses=[{source}]",
                   "--body", bodyfile(name, mapping))['id']
        action = f"decide:{name}"
        run("review", node, "--action", action, "--gate", expected=6)
        ref = f"E{index:03d}"
        evidence(ref, title + " - authored judgment", mapping + "\n\n" + decision)
        change("dispose", commitment, "--action", action, "--disposition", "satisfied",
               "--because", mapping.replace("\n", " "), "--ref", ref)
        run("review", node, "--action", action, "--gate")
        if close:
            change("set", node, "status=closed", f"outcome.ref={ref}", f"outcome.quote={decision}")
        else:
            change("set", node, "actor=scope-reviewer", "next=1")
        final = run("show", node)
        expected_state = "closed" if close else "open"
        assert final["status"] == expected_state, final
        outcomes.append({"case": name, "node": node, "authored_decision": decision,
                         "expected_status": expected_state, "actual_status": final["status"]})
    after = run("show", source)
    assert original["rev"] == after["rev"], "The old valid conclusion must remain unchanged"
    checked = run("check")
    assert checked["errors"] == checked["warnings"] == 0, checked
    run("tree")
    (dest / "trace.json").write_text(json.dumps(trace, indent=2) + "\n")
    summary = {"binary_version": subprocess.check_output([binary, "--version"], text=True).strip(),
               "cases": outcomes, "original_outcome_unchanged": True,
               "check": checked,
               "limit": "Gate verdicts check recorded review; they do not validate the authored scope judgment."}
    (dest / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    print(json.dumps(summary, indent=2))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--plied", default="plied")
    parser.add_argument("--output", type=Path)
    opts = parser.parse_args()
    binary = shutil.which(opts.plied)
    if binary is None:
        parser.error("Plied binary not found; use --plied /absolute/path/to/plied")
    destination = opts.output or Path(tempfile.mkdtemp(prefix="plied-scope-")) / "store"
    replay(str(Path(binary).resolve()), destination.resolve())
