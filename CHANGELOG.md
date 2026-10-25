# Changelog

All notable changes to plied. The on-disk format and the `--json` envelope
are versioned separately from plied itself; see
[FORMAT.md](https://github.com/jupsh/plied/blob/main/docs/FORMAT.md), *Versions*.

## Unreleased

- **`plied skill`** prints an agent skill (Markdown) generated from the
  installed binary's version and command list. It needs no repository
  (FORMAT.md §11).

## 0.2.0 — 2026-10-24

- **Scan reviews.** `plied scanned D001 --rev … --on T004 --because …`
  records on a thread that a `scan_commitments` record was read and holds no
  commitment to record. Its `commitment?` warning goes quiet until the record
  changes. The new thread field `scanned` keeps who, when, why and the
  revision read (FORMAT.md §3.3, §8). plied 0.1.0 reports it as an unknown
  field, so a store that uses it should set `requires = ">=0.2.0"`.
  ([#3](https://github.com/jupsh/plied/issues/3))
- **A closed stdout no longer panics.** `plied list | head -1` used to print
  a panic and exit 101 when the reader went away first. Now plied stops
  writing and exits with the code the command earned, so an accepted write
  still exits 0 (FORMAT.md §11, *Streams*). ([#2](https://github.com/jupsh/plied/issues/2))

## 0.1.0 — 2026-10-24

The first release. Record format 1, JSON envelope version 1.

- **Records** in git: threads, commitments and questions as Markdown with
  YAML frontmatter, and linked records from other systems, which are read
  only.
- **Checked writes.** Every change names the revision it was made against
  (`--expect`), and a write that would add a validation error is refused.
  Records and the generated view are replaced as one recoverable write: after
  a crash, `plied recover` (or the next write) finishes or undoes it.
- **Commitment review.** `plied review --gate` blocks an action until every
  applicable commitment has a disposition for it. FORMAT.md §12 defines which
  commitments apply.
- **Authority checks** on approvals, answers and waivers, against linked
  decision records (§4).
- **Messages** addressed to nodes, with presence leases, delivery, read and
  acknowledgement kept separate, and safe retries by id.
- **Literature.** Citations of cleared papers are checked by PDF page: five
  results per citation (§10).
- **Search and access.** Keyword search, structured `find`, and sealed paths
  that plied never reads.
- **Dates.** `due` and `revisit_on` are advisory dates that `check` and
  `frontier` report.
- **Action kinds.** An optional vocabulary for the action names given to
  `review` and `dispose`.
- **JSON.** `--json` on every command prints one versioned envelope, failures
  included (§11.1).
- **Bounded runtime cache.** The revision cache keeps content for
  `runtime.revision_days`, and `plied gc` prunes it.
- **Git guards**, so that record files change only through plied in the
  canonical checkout.
