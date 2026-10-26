# Changelog

All notable changes to plied. The on-disk format and the `--json` envelope
are versioned separately from plied itself; see
[FORMAT.md](https://github.com/jupsh/plied/blob/main/docs/FORMAT.md), *Versions*.

## Unreleased

- **A commitment can be superseded by another.** A decision that replaces
  one commitment with another (new timing, new wording) used to be recorded
  as `waived`, which says the obligation was released. The new resolved
  state `superseded` says it lives on, in the commitment `superseded_by`
  names. Like a waiver, its `resolved_by` is checked for authority over the
  old commitment. `check` reports a `superseded_by` that is not a
  commitment, or that loops back. It warns (code `superseded`) when the
  replacement, while unresolved, does not apply to every node the old one
  did. The view shows `superseded → P56`. (FORMAT.md §3.3, §4, §8;
  [#25](https://github.com/jupsh/plied/issues/25))
- **A commitment can say which kinds of action it governs.** A rule about
  how readings are reported needed a `not-applicable` disposition for every
  run, and the commitments that did govern runs got lost among them. The new
  field `governs: [report]` names the action kinds a commitment governs. The
  gate leaves it out for other kinds, and `review` still lists it as not
  governing the action (`governs_action: false` under `--json`). `governs`
  needs `checks.action_kinds`, so a misspelt kind is an error, and a
  `governs` that `check` rejects governs every action: the gate fails
  closed. (FORMAT.md §3.3, §12, §11.1;
  [#26](https://github.com/jupsh/plied/issues/26))
- **`check` tells a node named by a newer record from one reached through
  its links.** Both used to give the same stale warning. Now the warning's
  subject is `named` or `links`, the messages differ, and named nodes come
  first, since they are the likelier to need a change rather than a note.
  (FORMAT.md §8, *Stale*; [#27](https://github.com/jupsh/plied/issues/27))
- A store that uses `superseded` or `governs` should set
  `requires = ">=0.7.0"`: earlier plied reports them as an unknown state and
  an unknown field.

## 0.6.0 — 2026-10-25

- **The skill covers hand-offs that never happen, and small runs.** Work
  parked outside the record is noted on the node when it is parked, since a
  session can end without handing off, and hand-off notes what is unfinished
  before `release`. A commitment's wording decides what it covers: "before
  any run" includes a smoke run. A step of a plan an earlier session wrote
  is preceded by `plied review <node>`. Stale review in a script takes each
  rev from the `show` whose text was judged, one node at a time.
  ([#18](https://github.com/jupsh/plied/issues/18),
  [#19](https://github.com/jupsh/plied/issues/19),
  [#21](https://github.com/jupsh/plied/issues/21))
- **`claim` lists the commitments that apply to the node.** Taking up a
  node used to say only how long the lease runs, so an agent could start on
  a node without seeing what it obliges. `claim` now lists the applicable
  commitments (§12) and points to `plied review`, and its `--json` result
  carries them as `commitments`. `show --json` carries the same list for a
  thread or question, which plain `show` already printed (FORMAT.md §11.1).
  ([#20](https://github.com/jupsh/plied/issues/20))

## 0.5.0 — 2026-10-25

- **A resolved record goes stale only when named.** A closed thread, a
  fulfilled or waived commitment or an answered question used to go stale
  whenever a new entry mentioned anything it linked, and each needed a
  review note saying, in effect, "untouched". Its links are history, so now
  only an entry that names the record itself makes it stale, since that
  entry may be reopening it (FORMAT.md §8, *Stale*).

## 0.4.0 — 2026-10-25

- **An apostrophe no longer swallows the rest of a flow list.** A quote in
  the middle of an item (`[a, it's b, c]`) opened a quoted span that ran on
  to the next quote or the list's end, so the list was read as `a` and
  `it's b, c`, on assignment and when reading a file, with no error. Now, as
  in YAML, a quote opens a quoted item only at the item's start (FORMAT.md
  §3.1). ([#10](https://github.com/jupsh/plied/issues/10))
  - Lists that earlier plied *wrote* hold the merged item quoted
    (`[a, "it's b, c"]`). That is what the file says, so it still reads as
    two items. A long list was written in block form, where the merged item
    is not quoted at all (`- it's b, c`). Find them in either form with
    `plied find '{"options": {"$regex": "\\S[\u0027\"].*,"}}'` (`options` is
    the only prose list field: a mis-split ref list fails `check` as a broken
    reference) and fix them with `plied set`. Hand-written lists now read
    correctly. (Corrected after release: the grep first given here matched
    flow lists only.)
  - plied 0.3.0 and earlier misread the lists this version writes, and a
    write by them would merge the items again. A store that upgrades should
    set `requires = ">=0.4.0"`.
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
