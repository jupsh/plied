#!/usr/bin/env python3
"""Reconcile a plied ledger against an independently defined list of what should exist.

A ledger with perfect provenance for every entry it holds can still be incomplete:
the days a forecaster spoke are not all the days it could have. This program reads
an eligibility list kept outside the store, reads the ledger through `plied find
--json`, and reports which eligible items have exactly one forecast or recorded
abstention, and which do not. It is a report-only validator: it writes nothing and
approves nothing. Python 3.8+, standard library only.

Exit codes: 0 every eligible item is covered exactly once, 1 the report has gaps,
2 the inputs could not be read.
"""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import sys

ENTRIES = ("forecast", "abstain")


def plied(binary, store, *args):
    proc = subprocess.run([binary, "-C", str(store), "--json", *args], capture_output=True, text=True)
    envelope = json.loads(proc.stdout)
    if envelope.get("plied") != 1:
        raise SystemExit(f"unsupported plied JSON envelope: {envelope.get('plied')!r}")
    if not envelope["ok"]:
        raise SystemExit(f"plied {' '.join(args)} failed: {envelope['error']['message']}")
    return envelope["result"]


def load_list(path):
    raw = Path(path).read_bytes()
    data = json.loads(raw)
    if not isinstance(data, dict) or any(not isinstance(data.get(k), str) or not data[k].strip() for k in ("name", "version")):
        raise ValueError("the eligibility list needs a nonempty name and version")
    items = data.get("items")
    if not isinstance(items, list) or any(not isinstance(item, dict) or not isinstance(item.get("id"), str)
                                         or not item["id"].strip() or item["id"].strip() != item["id"] for item in items):
        raise ValueError("items must be a list of objects with nonempty, unpadded text ids")
    ids = [item["id"] for item in items]
    repeated = sorted({i for i in ids if ids.count(i) > 1})
    if repeated:
        raise SystemExit(f"{path}: the eligibility list names {', '.join(repeated)} more than once")
    return data, ids, hashlib.sha256(raw).hexdigest()


def reconcile(entries, eligible, item_field, entry_field, corrects_field):
    by_id = {e["id"]: e for e in entries}
    issues, edges = {}, {}
    for e in entries:
        problems = []
        if not isinstance(e.get(item_field), str) or not e.get(item_field):
            problems.append(f"no {item_field}")
        if e.get(entry_field) not in ENTRIES:
            problems.append(f"{entry_field} is {e.get(entry_field)!r}, not one of {', '.join(ENTRIES)}")
        target = e.get(corrects_field)
        if target is not None:
            if not isinstance(target, str) or not target:
                problems.append(f"{corrects_field} must name one entry (text)")
            elif target == e["id"]:
                problems.append(f"{corrects_field} cannot name the entry itself")
            elif target not in by_id:
                problems.append(f"{corrects_field} names {target}, which is not a readable entry")
            elif by_id[target].get(item_field) != e.get(item_field):
                problems.append(f"{corrects_field} names {target}, an entry for another item")
            else:
                edges[e["id"]] = target
        if problems:
            issues[e["id"]] = problems
    # An explicit correction graph has no chronology inferred from ids or timestamps. Cycles
    # cannot establish a replacement, and malformed entries never suppress the entries they name.
    checked = set()
    for origin in edges:
        trail, positions, current = [], {}, origin
        while current in edges and current not in positions and current not in checked:
            positions[current] = len(trail)
            trail.append(current)
            current = edges[current]
        if current in positions:
            for record in trail[positions[current]:]:
                issues.setdefault(record, []).append(f"{corrects_field} forms a correction cycle")
        checked.update(trail)
    malformed = [{"record": e["id"], "rev": e["rev"], "problems": issues[e["id"]]} for e in entries if e["id"] in issues]
    corrected = {target for record, target in edges.items() if record not in issues}
    bad = {m["record"] for m in malformed}
    # an entry counts unless it is malformed or a valid entry explicitly corrects it
    effective = [e for e in entries if e["id"] not in bad and e["id"] not in corrected]
    per_item = {}
    for e in effective:
        per_item.setdefault(e[item_field], []).append(e)
    pin = lambda e: {"record": e["id"], "rev": e["rev"], "entry": e[entry_field]}
    covered, missing, duplicate = [], [], []
    for item in eligible:
        found = per_item.get(item, [])
        if not found:
            missing.append(item)
        elif len(found) > 1:
            duplicate.append({"item": item, "entries": [pin(e) for e in found]})
        else:
            covered.append({"item": item, **pin(found[0])})
    ineligible = [{"item": item, "entries": [pin(e) for e in found]} for item, found in sorted(per_item.items()) if item not in eligible]
    return {
        "covered": covered, "missing": missing, "duplicate": duplicate, "ineligible": ineligible,
        "malformed": malformed, "corrected": sorted(corrected),
        "counts": {"eligible": len(eligible), "forecast": sum(c["entry"] == "forecast" for c in covered),
                   "abstain": sum(c["entry"] == "abstain" for c in covered), "missing": len(missing),
                   "duplicate": len(duplicate), "ineligible": len(ineligible), "malformed": len(malformed)},
    }


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--plied", default="plied", help="the plied binary")
    ap.add_argument("--store", default=".", help="a directory in the plied store")
    ap.add_argument("--eligible", required=True, help="the eligibility list (JSON: name, version, items[{id}])")
    ap.add_argument("--source", required=True, help="the [[sources]] name holding the ledger entries")
    ap.add_argument("--item-field", default="item", help="entry field naming the eligible item")
    ap.add_argument("--entry-field", default="entry", help="entry field: forecast or abstain")
    ap.add_argument("--corrects-field", default="corrects", help="entry field naming an entry it corrects")
    args = ap.parse_args(argv)
    try:
        listing, eligible, digest = load_list(args.eligible)
        context = plied(args.plied, args.store, "context")
        if args.source not in context.get("linked_sources", []):
            raise ValueError(f"{args.source!r} is not a configured linked source")
        entries = plied(args.plied, args.store, "find", json.dumps({"kind": args.source}))
        version = subprocess.run([args.plied, "--version"], capture_output=True, text=True).stdout.strip()
        report = reconcile(entries, eligible, args.item_field, args.entry_field, args.corrects_field)
    except (OSError, KeyError, ValueError, TypeError, SystemExit) as e:
        print(f"reconcile: {e}", file=sys.stderr)
        return 2
    report = {
        "v": 1,
        "eligibility": {"path": args.eligible, "sha256": digest, "name": listing.get("name"), "version": listing.get("version")},
        "ledger": {"source": args.source, "entries": len(entries), "plied": version},
        **report,
        "limits": [
            "The eligibility list is the caller's evidence: this report does not establish that it is complete or independent.",
            "Entries are those plied can read: one in a sealed path counts as missing.",
            "A covered item has one entry; nothing here judges the forecast or the reason for abstaining.",
            "Corrections explicitly name what they replace; this report does not infer chronological order.",
        ],
    }
    print(json.dumps(report, indent=2))
    gaps = report["counts"]
    return 1 if gaps["missing"] or gaps["duplicate"] or gaps["ineligible"] or gaps["malformed"] else 0


if __name__ == "__main__":
    sys.exit(main())
