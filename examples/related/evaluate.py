#!/usr/bin/env python3
"""Small labeled retrieval exercise, not an agent-judgment/productivity benchmark."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile
import time

DOCUMENTS = [
    ("E001", "Integer exclusion", """Object: x^2 + y^2 = 3 for integer coordinates.
Assumptions: both coordinates are integers. Squares modulo four are zero or one.
Result: no integer pair satisfies the equation. Renaming coordinates and restricting
the integer search to even first coordinates do not escape the exclusion.
Reconsider: a different equation or a domain beyond the integers requires its own argument."""),
    ("E002", "Mask optimization on a fixed carrier", """Object: a two-period single-hyperbola square.
Assumptions: all retained points lie on this fixed carrier, including arbitrary support masks.
Result: the best score over every subset of the carrier was already attained.
Deleting entire residue fibres is a special case of removing points, not a new carrier.
The full comparator also includes the classical corner, not just the new arm."""),
    ("E003", "Allocation preference after a sweep", """We prefer to avoid unchanged learning-rate,
seed or runtime sweeps after the fixed-budget experiment. This is a research default,
not an operator prohibition or a mathematical impossibility result. A scientifically
useful sensitivity question can justify a fresh parameter setting and budget decision."""),
    ("E004", "Detector failure leaves mechanism unresolved", """The readout failed to recover
the planted positive control at the delivered dose. Its null measurement does not
exclude the proposed mechanism. Diagnose the instrument and preserve the failed arm;
an independently validated detector would need a new experiment."""),
    ("E005", "Valid negative result under a fixed recipe", """The sensor recovered the known
positive at the required magnitude and all manipulation checks passed. No effect was
detected for the registered layer, stimulus and sample size. This supports a negative
result for those conditions, not absence of an effect at other depths or scales."""),
    ("E006", "Parser memory boundary", """The JSON parser's maximum input size is bounded.
Parsing a valid document produces a syntax tree. Malformed objects must not allocate
unbounded memory or cause stack overflow. This says nothing about research memory."""),
    ("E007", "Terminology alignment", """The earlier protocol calls these events scheduled
rendezvous; the new worklog calls them timed meetings. Both refer to the same agents
arriving at a predetermined location and moment. Compare the participants and clock
conventions before reusing the event counts."""),
    ("E008", "Arithmetic corruption invalidates an exclusion", """An earlier numerical claim
used the wrong denominator. Its quoted no-improvement conclusion has been withdrawn.
The raw arrays remain available and a linked correction describes the error. New
support for a similar conclusion would not rehabilitate that calculation."""),
]
QUERIES = [
    ("renamed", "Does u squared plus v squared equal three have whole-number solutions?", ["E001"]),
    ("subset", "Can enforcing an even x evade the integer impossibility for x*x+y*y=3?", ["E001"]),
    ("changed_premise", "Which earlier restriction matters if I allow real rather than integer coordinates in x^2+y^2=3?", ["E001"]),
    ("mask_paraphrase", "Can dropping complete residue classes beat the best point arrangement on the same hyperbola?", ["E002"]),
    ("heuristic", "Is testing another optimizer step size actually forbidden by my operator?", ["E003"]),
    ("instrument", "Our readout cannot see a known injected signal. Can its silence refute the mechanism?", ["E004"]),
    ("alias", "Where did we count timed meetings between participants?", ["E007"]),
    ("correction", "Which no-gain conclusion was retracted after an arithmetic normalization mistake?", ["E008"]),
]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--plied", required=True)
    parser.add_argument("--related", required=True)
    parser.add_argument("--python", required=True, help="Python with the optional FastEmbed dependencies")
    parser.add_argument("--cache-dir", required=True, help="Already downloaded model cache")
    parser.add_argument("--output", type=Path, required=True)
    opts = parser.parse_args()
    worker = Path(__file__).with_name("fastembed_worker.py").resolve()
    with tempfile.TemporaryDirectory(prefix="plied-related-eval-") as temp:
        root = Path(temp)
        subprocess.run([opts.plied, "-C", temp, "init"], capture_output=True, check=True)
        config = root / "plied.toml"
        config.write_text(config.read_text().replace('include = ["plied/**/*.md"]', 'include = ["notes/*.md"]') +
                          '\n[[sources]]\nname = "notes"\nglob = "notes/*.md"\nid = "^E[0-9]+$"\n')
        (root / "notes").mkdir()
        for rid, title, body in DOCUMENTS:
            (root / "notes" / f"{rid}.md").write_text(f"---\nid: {rid}\ntitle: {title}\n---\n{body}\n")
        report = {"v": 1, "fixture_sha256": hashlib.sha256(json.dumps([DOCUMENTS, QUERIES]).encode()).hexdigest(),
                  "limit": "Eight authored synthetic queries; retrieval only. Applicability and agent productivity are not evaluated.",
                  "modes": {}}
        for mode in ["lexical", "semantic", "hybrid"]:
            rows = []
            for name, query, relevant in QUERIES:
                command = [opts.related, "-C", temp, "--mode", mode, "-n", "5"]
                if mode != "lexical":
                    command += ["--embedder", opts.python, "--embed-arg", str(worker),
                                "--embed-arg=--cache-dir", "--embed-arg=" + opts.cache_dir]
                started = time.monotonic()
                proc = subprocess.run(command + [query], text=True, capture_output=True, check=True)
                data = json.loads(proc.stdout)
                ids = [p["id"] for p in data["candidates"]]
                rank = next((i+1 for i, rid in enumerate(ids) if rid in relevant), None)
                rows.append({"case": name, "query": query, "relevant": relevant, "retrieved": ids,
                             "first_relevant_rank": rank, "elapsed_seconds": round(time.monotonic()-started, 4),
                             "model": data["model"]})
            report["modes"][mode] = {"recall_at_3": sum(r["first_relevant_rank"] is not None and r["first_relevant_rank"] <= 3 for r in rows)/len(rows),
                                     "mrr_at_5": sum(1/r["first_relevant_rank"] if r["first_relevant_rank"] else 0 for r in rows)/len(rows),
                                     "queries": rows}
            print(mode, report["modes"][mode]["recall_at_3"], report["modes"][mode]["mrr_at_5"], flush=True)
        opts.output.write_text(json.dumps(report, indent=2) + "\n")


if __name__ == "__main__":
    main()
