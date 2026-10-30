#!/usr/bin/env python3
"""Check reconcile.py against a disposable store covering every kind of gap.

Requires git, Python 3.8+ and a plied binary. No network.
"""
import argparse
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

HERE = Path(__file__).resolve().parent


def store(root, entries, seal=()):
    subprocess.run(["git", "init", "-q", "-b", "main", str(root)], check=True)
    env = {k: v for k, v in os.environ.items() if not k.startswith("PLIED_")}
    subprocess.run([ARGS.plied, "-C", str(root), "init"], check=True, env=env, capture_output=True)
    config = root / "plied.toml"
    text = config.read_text(encoding="utf-8").replace("sealed = []", f"sealed = {json.dumps(list(seal))}")
    config.write_text(text + '\n[[sources]]\nname = "ledger"\nglob = "ledger/*.md"\nid = "^F[0-9]{3}$"\ndate = "date"\n', encoding="utf-8")
    (root / "ledger").mkdir()
    for fid, fields in entries.items():
        meta = "".join(f"{k}: {v}\n" for k, v in fields.items())
        (root / "ledger" / f"{fid}.md").write_text(f"---\nid: {fid}\ntitle: {fid}\ndate: 2026-10-01\n{meta}---\n", encoding="utf-8")


def reconcile(root, items):
    listing = root / "eligible.json"
    listing.write_text(json.dumps({"name": "fixtures", "version": "r1", "items": [{"id": i} for i in items]}), encoding="utf-8")
    proc = subprocess.run([sys.executable, str(HERE / "reconcile.py"), "--plied", ARGS.plied, "--store", str(root),
                           "--eligible", str(listing), "--source", "ledger"], capture_output=True, text=True)
    return proc.returncode, json.loads(proc.stdout) if proc.stdout else proc.stderr


def main():
    with tempfile.TemporaryDirectory() as tmp:
        full = Path(tmp) / "full"
        store(full, {
            "F001": {"item": "M001", "entry": "forecast"},
            "F002": {"item": "M002", "entry": "abstain"},
            "F003": {"item": "M003", "entry": "forecast"},
            "F004": {"item": "M003", "entry": "forecast", "corrects": "F003"},
            "F005": {"item": "M004", "entry": "forecast"},
            "F006": {"item": "M004", "entry": "abstain"},
            "F007": {"item": "M006", "entry": "forecast"},
            "F008": {"item": "M099", "entry": "forecast"},
            "F009": {"item": "M001", "entry": "maybe"},
            "F010": {"item": "M002", "entry": "forecast", "corrects": "F001"},
        }, seal=["ledger/F007"])
        code, report = reconcile(full, ["M001", "M002", "M003", "M004", "M005", "M006"])
        assert code == 1, (code, report)
        assert [c["item"] for c in report["covered"]] == ["M001", "M002", "M003"], report["covered"]
        assert next(c for c in report["covered"] if c["item"] == "M003")["record"] == "F004", "a correction replaces what it corrects"
        assert report["corrected"] == ["F003"]
        assert [d["item"] for d in report["duplicate"]] == ["M004"]
        assert report["missing"] == ["M005", "M006"], "a sealed entry is not readable, so its item is missing"
        assert [i["item"] for i in report["ineligible"]] == ["M099"]
        assert sorted(m["record"] for m in report["malformed"]) == ["F009", "F010"]
        assert report["counts"] == {"eligible": 6, "forecast": 2, "abstain": 1, "missing": 2, "duplicate": 1, "ineligible": 1, "malformed": 2}
        assert len(report["eligibility"]["sha256"]) == 64 and report["eligibility"]["version"] == "r1"
        assert all(len(c["rev"]) == 64 for c in report["covered"]), "each counted entry carries its revision"

        clean = Path(tmp) / "clean"
        store(clean, {"F001": {"item": "M001", "entry": "forecast"}, "F002": {"item": "M002", "entry": "abstain"}})
        code, report = reconcile(clean, ["M001", "M002"])
        assert code == 0, (code, report)

        code, err = reconcile(clean, ["M001", "M001"])
        assert code == 2 and "more than once" in err, (code, err)
    print("reconcile: all checks passed")


if __name__ == "__main__":
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--plied", required=True, help="the plied binary")
    ARGS = ap.parse_args()
    main()
