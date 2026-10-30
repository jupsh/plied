# Scope

What plied does itself, what it only makes possible, and what it leaves to
other tools. This document is for people deciding what to build next, in
plied or on top of it. It takes the gaps that worked scenarios exposed
(research, platform engineering, regulated change control, investigative
journalism, incident response, and others) and assigns each one a place.

The [worked research example](../examples/research/README.md) is a tracked
store and executable replay of interruption, recovery, comparison and
replication. It demonstrates the current gate's evidence and identity limits.

Every item has a status:

- **Implemented**: in the code and in [FORMAT.md](FORMAT.md).
- **Proposed**: agreed in direction, not built. The text says what is still
  undecided.
- **Needs design**: probably wanted, but its scope is not yet understood well
  enough to agree on.
- **Not planned**: outside plied. The text says what plied does at the edge.

## The rule

plied owns three things:

1. **What a record means**: its fields, references, quotations, authority and
   applicability.
2. **When a write is accepted**, and what a caller can assume when one fails.
3. **The hazards its own storage creates**: the runtime caches, the history
   it adds, and the files it leaves behind.

Everything else builds on explicit, versioned interfaces. A feature belongs in
core only if it meets one of those three tests. "Several use cases want it"
is not enough. Most of what they want is better built as a separate tool on
top of a good interface.

Two consequences follow:

- plied verifies evidence and does not evaluate policy. It can check that a
  decision record claims authority, or that a key it was told to trust signed
  something. It does not decide whether engineering verification passed.
- plied says what it cannot establish. A passing check must never suggest
  stronger evidence than plied actually examined. The README's "What it does
  not do" is part of the contract.

## Core

| Item | Status |
|---|---|
| Recoverable multi-file writes | Implemented |
| Behavioural contract in FORMAT.md | Implemented |
| Format compatibility separate from the tool version | Implemented |
| Bounded revision cache | Implemented |
| Action kinds | Implemented |
| Typed dates | Implemented |
| Explicit review evidence revisions | Implemented in 0.11.0 |
| Hashed sources beyond PDFs | Proposed |
| Relayed attribution | Proposed |

**Recoverable multi-file writes.** A write replaces the changed records and
the regenerated view through an intent journal. After a crash,
`plied recover` (or the next write) finishes or undoes it. Every new revision
is durably in `written.txt` before it appears on disk, so the commit guard
never refuses a file that plied wrote. While an intent is pending, `check`
fails and the commit guards refuse.

This is not atomicity to readers. Each file becomes visible at its own
rename, and reads take no lock: they warn when they see an intent, but they
are not snapshot-isolated. Message appends are outside the journal.
FORMAT.md §11 defines the exact outcomes, including recovery that completes
an earlier write before the current one is refused. Still open: whether
reads should take a consistent snapshot under the lock, or a shared lock,
rather than warning.

**Behavioural contract.** FORMAT.md used to specify file syntax far more
completely than behaviour, so a second implementation could follow it and
still disagree with this one about whether a gate passes or a write is
accepted. Items 1–5 are now specified, each pinned down by
[`tests/conformance.rs`](../tests/conformance.rs). Where the code's behaviour
had been an accident, it was changed to match a deliberate rule:

1. *Commitment applicability* (§12): the attachment and traversal rules, which
   commitments take part, and what the gate does and does not mean. Changed:
   a commitment with a malformed `state` now holds the gate instead of
   dropping out of it. Kept deliberately: "latest" means last in list order,
   and every disposition except `blocks` passes.
2. *Field schema* (§3.3): the type, whether it is required, and the meaning
   of every field and subfield, with prose fields labelled. Changed: empty
   values read as absent everywhere. Fixed: `image_checked` is a boolean, so
   `"false"` no longer counts as checked. `cites.kind` and `image_checked`
   reject other values.
3. *Diagnostic identity and message retries* (§8, §5). An error's identity is
   (level, code, location, subject), never its wording, and writes compare
   identities. Fixed: resolving one of several unresolved dependencies used
   to be refused, because the remaining error's message changed. Messages now
   have deliberate rules: ids are unique across logs; reusing an id for
   different content is refused (exit 3); `v` other than 1 is skipped and
   reported; event fields are typed; order is append order.
4. *Access coverage* (§9): a table of what sealing covers, including the
   revision cache (sealing removes nothing already cached) and directory
   listings. Fixed: message logs are now covered, so a sealed log is neither
   read nor appended to.
5. *Authority* (§4): a decision table, saying plainly that a delegation check
   only establishes that the delegating record exists, and that a passing
   check establishes nothing about who wrote the decision.

Items 6 and 7 followed:

6. *CLI and JSON shapes* (§11, §11.1). `--json` is accepted by every command
   except `hook`. It prints exactly one envelope (`plied`, `ok`, `exit`,
   `status`, then `result` or `error`, plus `notes`) and nothing on stderr.
   Usage errors and failures get an envelope too, and a conflict or refused
   write carries structured detail. Result shapes are listed per command.
   Fixed along the way: an assignment's operator was found anywhere in the
   argument, so a value containing `+=` was misread.
7. *Smaller items.* Locators never narrow quotation matching (§3.4).
   Encodings (§3): a BOM is ignored, CRLF reads as LF, invalid UTF-8 in a
   record is an error, and revisions always hash raw bytes. A citation always
   gets five results in a fixed order, the new `image` result among them
   (§10). The store lock is specified as a protocol (§6), and sharing a
   runtime directory across machines is excluded.

The scenarios raise two separate questions: which evidence revisions did a
review examine, and which artifact revision did an approval authorise?
Optional review inputs now answer the first for explicitly supplied records
(below). Approval still names a decision without binding the artifact's
revision (§3.3, §12); that remains part of the operation and attestation
designs below.

**Format compatibility** (FORMAT.md, *Versions*). `[plied] format` (now 1)
is the on-disk format, and a plied that doesn't know it refuses writes.
`requires` accepts `">=X.Y.Z"` as well as an exact version, and `init` now
writes the minimum form, so upgrading no longer means editing every project.
The tool version stays as provenance. Within a format, unknown record fields
and `plied.toml` keys are errors, and unknown events are skipped and
reported. No migration exists yet, because format 1 is the only format.

**Bounded revision cache** (§6). Content in `revs/` is kept for
`runtime.revision_days` (default 14) after it was last read or written. It
is pruned at most daily after a write, or by `plied gc` (`--all` empties it).
`written.txt` holds hashes only and is never pruned, so the commit guard's
evidence survives. Without the cached revision, a conflict is still refused,
just without a diff.

**Action kinds** (§12). An action is `kind` or `kind:instance`. With
`checks.action_kinds` set, `review` and `dispose` refuse an unknown kind
(exit 2). `check` only warns about reviews recorded before the vocabulary
existed. A commitment can name the kinds it `governs`, which needs the
vocabulary, so that a misspelt kind is an error rather than an open gate. The gate still compares whole names. It does not check that the
caller chose the right nodes, or that the gate was run at all.

**Typed dates** (§3.3). `due` (every kind) and `revisit_on` (threads) are
`YYYY-MM-DD` dates. `check` warns when one asks for attention, and
`frontier` lists them. They never block a write or a gate and never change a
state. Nothing is sent anywhere. `expires` was left out: nothing yet needs
it.

**Explicit review evidence revisions** (§3.3, §8, §12). Since 0.11.0,
`dispose --read ID@FULL_REV` can record the exact configured records a
reviewer read. Each supplied revision is a full raw-byte SHA-256, compared
against the store loaded under the write lock before accepting the
disposition. Inputs are explicit: `--ref` remains unchecked prose, and no
input is inferred from it or the reference graph. The lock coordinates plied
writers; it does not provide snapshot isolation against arbitrary edits to
linked files.

`check` warns when an input of the latest disposition for an exact action
changed or became unavailable, and `review` shows its current input status.
An unrelated note does not clear that warning. A replacement disposition
records a new review; an unpinned disposition remains explicitly unbound.
These claims are advisory and do not change the exact-action gate. They
neither attach commitments nor carry stale-by-link warnings.

A pin identifies bytes without retaining them. `revs/` remains a disposable
cache, including after `gc --all`; preserving the exact source bytes, for
example in git, is still necessary. A raw-file SHA-256 is not a git object
id: any historical retrieval must verify the recovered bytes against the
pin. Pins for arbitrary files and approval-to-artifact binding remain
separate proposed work.

**Hashed sources beyond PDFs.** Datasets and other files can be cited by hash,
like cleared papers, so that "computed from dataset version 7" can be checked.
A hash identifies evidence; it cannot bring missing evidence back. "Valid as
of" checking also needs the content that was referenced, the policy then in
force and the validator's rules at the time, so it is not claimed here.

**Relayed attribution.** Add a field such as `relayed_by` next to `from`, so
that a transcribed email does not record the transcriber as its author.

## Interfaces

These are built so that other tools can do the work.

| Item | Status |
|---|---|
| Versioned JSON results, errors included | Implemented |
| Portable operations and application receipts | Proposed: [RFC 0001](rfc/0001-operations.md) |
| One acknowledgement model; versioned feature writes | Proposed: [RFC 0002](rfc/0002-attention.md) |
| Attestations | Proposed, after operations |
| Export closure | Needs design |
| External reference helpers | Needs design |
| Report-only external validators | Needs design |

**Versioned JSON results** (FORMAT.md §11.1). Every command reports its
result, failures included, in one envelope with a version. Notification
bridges, dashboards and issue-tracker sync are built on this, outside plied.

**Portable operations.** A write that can be serialised, carried on a branch
or over any transport, and applied later under the same checks. Each
application leaves a durable receipt. See RFC 0001. Signatures, proposals and
an audit trail all build on this, so it comes first.

**Attestations.** These are optional SSH signatures, verified against signers
that `plied.toml` explicitly trusts, with revocation. OpenSSH already has
signature namespaces, allowed-signers files and revocation lists. Three
claims are kept separate: who wrote something, whether a decision had
authority, and whether a change may be applied. An attestation must also bind
its subject: which revision of a node a decision approved, and which
evidence revisions a review examined. Signing a decision that names only a
node id doesn't settle either. `approval.by` keeps pointing
at the decision record, and the attestation is attached to that decision's
exact revision. Initial trust is a configuration choice that plied cannot
make for anyone. For root rotation, TUF's model is the reference: thresholds
from both the old and the new root, plus version checks. Attestations sign
exact payload bytes in a DSSE envelope, which binds the payload type, so no
canonical encoding is needed. Verification happens in `check` and
`guard range`. Enforcement belongs wherever writes can actually be refused: a
protected branch, CI or a server. Locally, a signature is evidence, not a
barrier.

**Export closure.** Core computes a selection plus everything it references,
reports references that leave the selection, and records what was withheld.
Still to decide: which references count; what happens when the evidence
itself is sealed; and how to keep the withheld manifest from revealing the
identifiers it withholds. Output formats and redaction rules are built
outside plied.

**External reference helpers.** References such as `gh:org/repo#4821` or
`repo:satellite/T004` would be resolved by optional helper programs, like
git's credential and remote helpers. This brings program execution and
network access into the trust boundary, as `image_command` already does, so
it needs its own threat model before it is built.

**Report-only validators.** External programs that report diagnostics, for
example numeric tolerances, which `check` merges into its results. A
validator can report a problem. It can never approve a write or a gate.

## Not planned

| Item | What plied does at the edge |
|---|---|
| Encryption at rest, confidentiality against an adversary | Treats a file it cannot decrypt as sealed, not as a broken reference. Never describes sealing as secrecy |
| Secure deletion and rewriting history | Bounds its own caches. A tombstone record (proposed) would let a lawful deletion leave the rest of the record consistent. Rewriting history is done with tools like `git filter-repo` |
| Live coordination across machines; a server | Portable operations and JSON results are the building blocks for one |
| Delivering notifications | Typed dates and JSON results; delivery is done by a separate bridge |
| Semantic search; corroboration analysis | `find`, backlinks and JSON output |
| Accounts for people outside the repository | Relayed attribution records who transcribed what |
| A policy language | Attestations from a signing service that evaluates whatever policy it likes (Cedar, OPA) |
| Immutability guarantees, resistance to rollback | Receipts and attestations make tampering detectable against a checkpoint. Trusting that checkpoint (a transparency log, a protected branch) is someone else's job |

The optional [related-work companion example](../examples/related/README.md)
prototypes semantic retrieval above the checked record model. It reuses the
record reader and access policy but has its own experimental output, optional
local model adapter and evaluation fixture. It neither adds semantic matching
to core gates nor establishes that a retrieved conclusion applies to a new
task. The [scope replay](../examples/scope/README.md) demonstrates recording
that judgment separately, using existing fields.

Encryption and lawful destruction both cut against plied's append-only,
plaintext-in-git design. If either ever comes into scope, they should be
designed together and late. Adding either as a feature would produce exactly
false confidence, in exactly the settings (journalism with sources at risk,
regulated records) where confidentiality is tested by adversaries.

## Order

| Step | Deliverable |
|---|---|
| 1 | This document; recoverable multi-file writes and `plied recover` *(done)* |
| 2 | Contract items 1–5 above, with conformance fixtures; fix the two known bugs (`image_checked`, message reads ignoring sealing) *(done)* |
| 3 | Format compatibility separate from the tool version; bounded revision cache; action kinds; typed dates *(done)* |
| 4 | Versioned JSON results and errors (contract item 6) *(done)* |
| 5 | RFC 0001 agreed, then unsigned proposals for updates to existing records, with receipts. Tests for interrupted writes, duplicate applications and stale proposals |
| 6 | Optional SSH attestations, with trusted signers and revocation |

Export closure, reference helpers and validators each get their own design
after step 5.

