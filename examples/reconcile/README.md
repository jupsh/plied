# Reconciling a ledger against what should exist

A ledger can have perfect provenance for every entry it holds and still be
incomplete. A forecasting ledger that records only the matches its system
spoke about overstates what it knows. Each eligible match needs a forecast or
a recorded abstention. plied's own checks compare records that exist; none can
say what should have existed, because that list is defined outside the
store. This example is a report-only validator built on plied's JSON
interface (FORMAT.md §11.1), as SCOPE.md proposes. It writes nothing and can
approve nothing.

```sh
python3 examples/reconcile/reconcile.py --plied "$(command -v plied)" --store . \
  --eligible fixtures-2026-27.json --source ledger --item-field match
```

**Inputs.**

- An eligibility list kept outside the store, as JSON:
  `{"name": "...", "version": "...", "items": [{"id": "M001"}, ...]}`. An id
  listed twice is an input error (exit 2). Name and version must be nonempty
  text; item ids must be nonempty text without leading or trailing whitespace.
- The linked records of one `[[sources]]` entry: the ledger. Each entry
  names the item (`--item-field`, default `item`) and says `forecast` or
  `abstain` (`--entry-field`, default `entry`). An entry can name an earlier
  entry for the same item that it `corrects` (`--corrects-field`); the
  corrected entry no longer counts.
  Corrections must name one readable entry for the same item and must not
  form a self-reference or cycle. A malformed correction never suppresses
  its target. Replacement is explicit; chronological order is not inferred
  from ids or dates.

**Report** (JSON on stdout):

| key | meaning |
|---|---|
| `eligibility` | the list's path, name, version and the SHA-256 of its bytes |
| `covered` | eligible items with exactly one counting entry, each with the entry's id and full revision |
| `missing` | eligible items with no readable counting entry |
| `duplicate` | eligible items with more than one, with all of them |
| `ineligible` | entries for items not on the list |
| `malformed` | entries without an item or a known entry, or correcting what they cannot |
| `corrected` | entries replaced by a correction |
| `limits` | what the report does not establish |

Exit 0 when every eligible item is covered exactly once and nothing is
ineligible or malformed, 1 when the report has gaps, 2 when the inputs could
not be read.

**What it does not establish.** The list is the caller's evidence: keep it
versioned and independent of whoever writes the ledger, and keep the
report's `sha256` with the result. An entry in a sealed path is not readable,
so its item is reported missing. A covered item has one entry; nothing here
judges a forecast or the reason for an abstention.

[test_reconcile.py](test_reconcile.py) builds a disposable store with every
kind of gap and checks the report:

```sh
python3 examples/reconcile/test_reconcile.py --plied "$PWD/target/debug/plied"
```
