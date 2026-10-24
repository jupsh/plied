# RFC 0001: Portable operations

Status: **draft**. Nothing here is implemented. Its prerequisite, recoverable
multi-file writes, is (FORMAT.md §11).

## Summary

Make a write to an existing record something that can be serialised. It
could be carried on a branch, sent between machines or held for review,
then applied later under exactly the checks a direct write gets. Every
application leaves a durable receipt, so a retry can be told apart from a
second application. Signatures (SCOPE.md, *Attestations*) will sign these
operations, so their shape has to be settled first.

## Motivation

- **Records on branches.** An engineer on a feature branch
  can draft record changes there, as files. Applying them after the merge
  runs the usual revision and validation checks.
- **Across machines.** The README already suggests sending proposals
  as messages. This gives that proposal a defined format.
- **Attribution and audit.** A signature needs exact bytes to
  sign. An audit trail needs to record which proposal produced which revision.

## Non-goals for version 1

- **Creating records.** `next_id` takes the highest number plus one, so two
  proposals made independently would claim the same id. See *Open questions*.
- **Messages.** Message events already have ids and can be retried by id.
  Version 1 doesn't change their write path.
- **Signatures**, **operations that span repositories**, and **immutability**.
  A local receipt is useful history, not tamper-proof history.

## The operation

An operation is a JSON object. Its exact serialised bytes are its identity:
the **operation digest** is the sha256 of those bytes. plied never
re-serialises an operation it received. It stores and compares the bytes as
given, so no canonical encoding is needed. A later DSSE envelope signs the
same bytes with the payload type `application/vnd.plied.op+json; v=1`.

```json
{
  "v": 1,
  "id": "01JB6Q4M8Z3X…",
  "repo": "6f1c…",
  "format": 1,
  "kind": "update",
  "target": "T004",
  "base": "<64 hex: full revision the change was made against>",
  "result": "<the complete new file content>",
  "result_sha": "<64 hex: sha256 of result>",
  "action": "set",
  "actor": "alice",
  "session": "sess1234…",
  "created": "2026-10-24T16:02:11Z",
  "config": "<64 hex: digest of the plied.toml it was made under>",
  "tool": "plied 0.1.0"
}
```

| field | meaning |
|---|---|
| `v` | operation schema version |
| `id` | ULID chosen when the operation is made. Retries reuse it |
| `repo` | identity of the record store. A new `[plied] id` written by `plied init` (see *Open questions*) |
| `format` | the record format version the result is written in |
| `kind` | `update` only, in version 1 |
| `target` | the record id |
| `base` | the **full** revision the change was made against. A prefix isn't accepted |
| `result` | the exact resulting file bytes, history line included, as a UTF-8 string. A non-UTF-8 result can't be an operation |
| `result_sha` | sha256 of `result`. Redundant on purpose, so a receipt can name it without holding the content |
| `action` | what made it (`set`, `dispose`, `answer`, …), for people reading |
| `actor`, `session`, `created` | **claimed**, like today's history line. Version 1 authenticates none of them |
| `config` | digest of the configuration in force when the operation was made. If it differs at apply time, the operation is still validated against today's configuration, and the receipt records both digests |
| `tool` | provenance only. It never gates anything |

The operation carries the whole result rather than a description of the
change. That is the only way applying it can reproduce exactly the bytes that
were reviewed, or signed. The consequence is deliberate: an operation is
never rebased. If the target has moved on, the operation is stale and its
author makes a new one from the current revision.

## Applying

`plied apply <file>…` takes each operation in turn, under the lock:

1. Recover any interrupted write (FORMAT.md §11). Refuse on the same
   branch, merge-state and version checks as any write.
2. Parse. Refuse (exit 2) if the operation is malformed, has an unknown `v`,
   `kind` or `format`, or if `result_sha` doesn't match `result`.
3. Refuse (exit 5) if `repo` is not this store.
4. Look up a receipt for `id`:
   - the same operation digest: **already applied**. Exit 0 and say so. This
     is what makes retries safe;
   - a different digest: refuse (exit 3). The id was reused for a different
     change.
5. Refuse with a conflict (exit 3, and the usual diff) unless the target's
   current revision is exactly `base`.
6. Refuse (exit 4) if `result` would add a validation error. This uses the
   same rule as direct writes: an error whose identity (code, location,
   subject; FORMAT.md §8) the current store lacks.
7. Commit the result, the regenerated view and the receipt in **one**
   `commit_files`. After successful recovery from a crash, there is then
   never a result without its receipt, or a receipt without its result.
   Before recovery, a reader can see one without the other: the write is
   recoverable, not atomic to readers.

Applying several files is not atomic across operations. Each one succeeds or
fails on its own, and the JSON result reports each outcome. Making a batch
all-or-nothing is a possible later extension.

## Receipts

A receipt is written to `<root>/receipts/<id>.json`. It is tracked, written
by plied, and listed in `written.txt` like any record:

```json
{"v": 1, "id": "01JB6Q…", "op": "<operation digest>", "target": "T004",
 "base": "<rev>", "result": "<rev>", "applied": "2026-10-25T09:14:00Z",
 "applied_by": "bob", "config_made": "<digest>", "config_applied": "<digest>",
 "tool": "plied 0.1.0"}
```

- **Digests, not content.** The result is already in the record file. A
  receipt holds no record content, so receipts never become a second
  plaintext archive, and garbage collection of the revision cache doesn't
  touch them.
- **One file per operation**, not a shared log. Appending to a shared log
  would cause the merge conflicts that the generated view already causes.
- **Who applied it** is claimed, as everywhere else in version 1.

## What this does and does not protect against

| Threat | Outcome |
|---|---|
| A write made against an old revision | Refused: `base` must equal the current revision |
| The same operation submitted twice (a retry, a re-run script) | Recognised by its receipt; recorded once |
| One id reused for a different change | Refused |
| A crash during apply | Finished or undone by `plied recover` or the next write. After successful recovery, the result and its receipt are both present or both absent |
| **The record is restored to old bytes, so `base` matches again** | Refused, as long as the receipt survives: the id is already applied. An operation with a *new* id built on the restored revision is a new change, and is accepted |
| **The receipts themselves are rolled back** (a `git reset`, a rewritten branch) | **Not detected.** Detecting it needs a checkpoint trusted independently of this repository, such as a protected branch, a transparency log or a signed tree head. Out of scope |
| A forged `actor` | Not detected until attestations exist |

## Where operations live

An operation file can live anywhere outside `<root>`, for example in
`proposals/` on a feature branch. Since it isn't a record file, the commit
guard has nothing to say about it. A merge brings the file onto the main
branch, and `plied apply proposals/*.json` (by hand, or in CI after the merge)
applies it in the canonical checkout. Deleting an applied proposal file is
safe, because the receipt is the record that it was applied.

New commands:

- `plied set … --propose <file>` (and the same flag on every update command)
  writes the operation instead of applying it;
- `plied apply <file>…`;
- `plied receipt <id>`.

Every result is versioned JSON with `--json`, failures included.

## Tests the implementation must have

- A crash at each step of `commit_files` during apply. After successful
  recovery, either both the result and the receipt are present, or neither
  is.
- Applying the same file twice, and the same id with a different `result`.
- A stale `base`, a `base` given as a prefix, and a `result_sha` that doesn't
  match.
- A different `repo`.
- A `config` that changed between making and applying, with both digests in
  the receipt.
- An operation whose result would add a validation error, against a store
  that already has an unrelated error.

## Open questions

1. **Repository identity.** Option (a): a random `[plied] id` written by
   `init`, which is easy to read and survives history rewrites, but is copied
   if the repo is used as a template. Option (b): the root commit hash, which
   needs nothing new but there can be several, and rewriting history changes
   it. (a) is proposed, with existing repositories getting an id on their
   first `plied init --upgrade`.
2. **Creates.** Option (a): ids chosen by the client with a random suffix
   (`T-01JB6Q`), which never collide but break the `T004` style. Option (b):
   reserving id ranges per actor, which needs coordination. Option (c):
   renumbering at apply time, which breaks any text that quotes the id.
   Undecided. Version 1 doesn't do creates.
3. **Should direct writes leave receipts too?** Doing so would make every
   accepted write auditable in one way, at the cost of one tracked file per
   write. It may be better as a setting.
4. **Retention of receipts.** They are small, but there is one per write,
   forever. Do they need archiving or compaction (SCOPE.md, scale) before
   they need anything else?
5. **What did a review or approval examine?** Exact
   operation bytes say what change was proposed. They don't say which
   evidence revisions a disposition relied on, or which revision of a node an
   approval covered. A later version could let an operation that records a
   disposition or an approval carry those revisions. Until then, neither is
   recorded.
6. **Exit code for a reused id.** 3 (conflict) is proposed, matching a
   message id reused for different content (FORMAT.md §5). A new code would
   be more precise but adds to the CLI contract.
