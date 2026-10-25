# Changelog

All notable changes to plied. The on-disk format and the `--json` envelope
are versioned separately from plied itself; see
[FORMAT.md](https://github.com/jupsh/plied/blob/main/docs/FORMAT.md), *Versions*.

## Unreleased

- **The skill draws on a first session in a real store.** Stale-review
  notes now say what the entry touches and why the node is or is not
  affected, since "no change" notes become a rubber stamp. An id-shaped name
  in an entry is noted on the node, never reworded away in the linked
  record. The gate needs every node an action depends on, and a fresh
  action name per run. An answer heard in conversation counts only once a
  decision record holds it. Keeping `actor` and `next` current keeps the
  frontier true, and `plied view` need not be run after writes.

- **`parent` no longer spreads staleness.** A linked record citing a group
  thread used to make every child stale as well, each needing its own
  review note. Now only the group goes stale, as `parent` is organisational
  and carries no obligations either (FORMAT.md §8, *Stale*, which now also
  defines what counts as a mention).

- **The view no longer goes stale when a linked file changes.** It showed
  the state of each thread's `backlog` record, so editing that linked file
  made `plied view --check` fail although plied had written nothing. The view
  now names the backlog id only; `plied tree` still shows its state
  (FORMAT.md §7).
- **`plied frontier` lists open threads in no queue** (no `next`), as the
  view already did: under `--json` as `unscheduled`, with `--actor` only that
  actor's (FORMAT.md §11.1).
- **The skill takes revs from `plied show`.** It said to parse `--json` for
  every rev and never use the human output, but `--expect` takes the 12-digit
  prefix `show` prints. It now says to use the rev printed by the command
  that showed the text, and to give prose list items (`options`) one `+=`
  each, since a flow list splits at every unquoted comma.

## 0.3.0 — 2026-10-25

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
