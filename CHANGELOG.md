# Changelog

All notable changes to plied. The on-disk format and the `--json` envelope
are versioned separately from plied itself; see
[FORMAT.md](https://github.com/jupsh/plied/blob/main/docs/FORMAT.md), *Versions*.

## 0.1.0 — unreleased

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
