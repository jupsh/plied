# Issue-board review — 2026-10-29

## Review basis

Reviewed all **17 open issues** and **33 closed issues**, including the full
body and every comment of each issue. Read FORMAT.md, SCOPE.md, RFC 0001,
the README, development guidance and the existing acceptance exercises.
Maintainer design notes take precedence over an issue's original proposed
shape. Closed decisions remain context; this change reopens none of them.

The local checkout initially ended at 0.6.0. Implementation uses the current
`origin/main` at `5e22e93806dc375e046e379f44c164c7a5388342` (0.10.0) in an
isolated worktree, preserving the original checkout's unrelated edits.
Also inspected open [PR #76](https://github.com/jupsh/plied/pull/76), which
addresses part of #75 but is not merged or duplicated here.

Votes count only comments beginning `Vote: +1` or `Vote: -1`. Reactions do
not count. #30 has one positive vote; every other open issue has zero votes.

The closed-issue context consists of #2, #3, #10, #18, #19, #20, #21, #25,
#26, #27, #32, #36, #38, #39, #46, #47, #48, #49, #50, #51, #52, #53,
#54, #55, #56, #57, #58, #59, #60, #61, #62, #63 and #64. In particular,
#27 already settled named/linked stale identities and ordering; #19 rejects
fetching revisions just before a bulk write; #46–#52 establish the installed
tool's discovery and onboarding surfaces; #60–#62 preserve append-only
review history and conservative gate behavior.

## Triage

The stale family (#75, #85, #81, #83, #30 and closed #27) was considered
together. Dated mentions, exact evidence-byte changes, partial review
acknowledgements and presentation are separate claims. Presentation and
comparison must not acknowledge a review. Neither `nodes` nor `made` stops
contributing to existing warnings in this change.

| Issue | Votes + / − | Category | Decision | Reason |
|---|---:|---|---|---|
| [#30](https://github.com/jupsh/plied/issues/30) Evidence revisions examined | 1 / 0 | implement | Add explicit disposition input pins and fix the existing scan comparison race. | The newest maintainer proposal supplies a defensible read-set contract; checking `scanned` before the lock is an existing bug. |
| [#65](https://github.com/jupsh/plied/issues/65) Unfamiliar-user validation | 0 / 0 | defer | Implement the external-ID help observation; retain the fresh-tester acceptance exercise. | The implementers are already familiar, and the user's memory constraint rules out another tester agent in this run. |
| [#77](https://github.com/jupsh/plied/issues/77) Runtime writability discovery | 0 / 0 | implement differently | Explicitly report unverified filesystem writability and explain runtime-writing operations. | A read-only context command cannot reliably prove writability without an actual write probe. |
| [#79](https://github.com/jupsh/plied/issues/79) Diagnostic baselines | 0 / 0 | implement | Compare saved complete check reports using diagnostic identity plus source revisions. | This saves concurrent-work accounting while retaining the full current verdict and making no attribution. |
| [#80](https://github.com/jupsh/plied/issues/80) Participant guidance | 0 / 0 | implement | Add a compact participant workflow to the generated skill. | Roles that can communicate but cannot author records need usable guidance without implied authority. |
| [#81](https://github.com/jupsh/plied/issues/81) Freshness in orientation | 0 / 0 | implement | Add opt-in `show --freshness` and `frontier --freshness`. | Reuse existing stale causes beside the records being read without running unrelated validation. |
| [#82](https://github.com/jupsh/plied/issues/82) Compact show | 0 / 0 | implement | Add opt-in `show --summary` using existing metadata and author-written summaries. | Bounded orientation reduces truncation while full records and exact commitment wording remain available. |
| [#83](https://github.com/jupsh/plied/issues/83) Group stale causes | 0 / 0 | implement | Add opt-in `check --group-stale` with source revisions and reference edges. | Shared causes become visible without suppressing individual diagnostics or reviewing nodes in bulk. |
| [#75](https://github.com/jupsh/plied/issues/75) Attachment and `made` fan-out | 0 / 0 | defer | Keep existing warnings; resolve the review-preserving semantics before adopting PR #76. | Excluding reference fields would clear current warnings without the review required by this task; the new `made` report is a separate part of the cost. |
| [#85](https://github.com/jupsh/plied/issues/85) Scoped review notes | 0 / 0 | defer | Design durable per-source review coverage and its write preconditions first. | An ordinary history line advances the global stale watermark; partial acknowledgement must preserve every unread source. |
| [#78](https://github.com/jupsh/plied/issues/78) Linked-work frontier metadata | 0 / 0 | defer | Define generic field selection, provenance and claim-origin semantics. | A backlog-specific schema or guessed status/authority would make orientation misleading. |
| [#84](https://github.com/jupsh/plied/issues/84) Question context | 0 / 0 | defer | Add the independent authoring guidance now; design bounded traversal and partial-result disclosure separately. | A context bundle needs precise inclusion limits and privacy behavior before it can claim useful coverage. |
| [#31](https://github.com/jupsh/plied/issues/31) Withdrawal history | 0 / 0 | defer | Coordinate a retained outcome snapshot and downstream warnings with evidence revision binding. | Withdrawing a conclusion, its rationale and reopening work are different operations. |
| [#33](https://github.com/jupsh/plied/issues/33) Population completeness | 0 / 0 | defer | Start with an external reconciler and a versioned independent eligibility list. | Existing records cannot establish their own missing population; merged validators and generic abstention semantics remain undecided. |
| [#34](https://github.com/jupsh/plied/issues/34) Outcome labels | 0 / 0 | defer | Exercise project-specific metadata in the research example before adding a core field. | Work completion and evidence informativeness are separate dimensions that one label can conflate. |
| [#35](https://github.com/jupsh/plied/issues/35) Locator-scoped quotation matching | 0 / 0 | defer | Preserve whole-file matching pending a concrete selector contract and fixtures. | Repeated headings, missing sections and cross-section ellipses need rules; free-text locators cannot silently acquire new meaning. |
| [#37](https://github.com/jupsh/plied/issues/37) Export closure | 0 / 0 | defer | Retain the requirements for the post-operations export design. | Exact selected bytes, separate supplier attribution and non-leaking withheld manifests require coordinated closure and trust semantics. |

## Implemented change summary

- Move scan source-revision checks into the store transaction, closing the
  wait-for-lock race before a scan review can be appended.
- Add explicit, repeatable `dispose --read ID@REV` inputs. Store full
  raw-file SHA-256 pairs in the appended disposition and compare the whole
  supplied read set under the write lock. Report current, changed,
  unavailable and unbound evidence claims separately from the exact-action
  gate. A note does not clear a changed-input warning; the latest disposition
  for that action supplies the current evidence claim. Pins identify bytes
  and do not retain them.
- Add saved-check baseline comparison. Preserve all current diagnostics and
  the existing exit verdict; distinguish changes in the sources of aggregated
  stale diagnostics rather than matching message wording. Reject unsupported
  snapshots and incompatible warning/access-policy scopes.
- Add optional freshness and grouped stale presentations using the same
  access-filtered structural causes as the existing checks. Expose source
  revisions, mentioned IDs and the exact reference-field edges without
  changing review requirements.
- Add compact `show` orientation, runtime-writability guidance, unified
  configured-source ID help, participant guidance and question-authoring
  guidance. Keep full records and existing authoring workflows available.
- Keep record format 1 and JSON envelope 1. The package version becomes
  0.11.0 for the new optional `reviews.inputs` field; stores using that
  field should require `>=0.11.0`. This is an Unreleased change, not a tag,
  publication or merge.

### Validation results

All checks passed, serialized with one compiler job and one test thread:

- `cargo test --locked`: **130 tests passed** on stable Rust.
- `cargo +1.89 test --locked`: **130 tests passed** on Rust 1.89.0.
- Stable Clippy, `--all-targets -- -D warnings`, with and without `--features pdf`.
- `cargo build --locked` and the built binary's `--help`.
- First-task, research and scope executable replays.
- Related companion: **4 Rust tests and 11 Python integration tests**, plus build.
- `git diff --check`.

Frozen pre-change format-1 stores remain valid. Regression coverage includes
both lock waits, multi-input conflicts, generic replacement preconditions,
missing/sealed evidence, unchanged gates, cache pruning, source additions
beyond the diagnostic preview, message-independent comparisons, and a file
moved behind an existing seal. Compact views preserve full JSON fields and
do not change records or review requirements.

After the memory warning, about 500 MiB of task build output was moved from
RAM-backed `/tmp` to disk, and all remaining builds/checks ran serially.
The open-board listing was refreshed before publication: the same 17 issues
and update times remained. Issue replies below remain drafts.

### FORMAT.md sections

The changed contract sections are:

- §3.3: optional review input pairs and version compatibility.
- §3.7: compact orientation and the limits of author-written summaries.
- §6: pins identify evidence but do not extend revision-cache retention.
- §8: advisory input diagnostics, freshness presentation and baseline
  comparison identities.
- §9: access filtering and saved-report disclosure limits.
- §11, §11.1 and §11.2: CLI flags, refusals, additive JSON shapes and
  unverified runtime-writability discovery.
- §12: current review-input status remains separate from gate verdicts.

### Unfamiliar-user exercise (#65)

The full unfamiliar-tester exercise is deferred. All implementation agents
have read the source and issue designs, so they cannot meet that criterion.
After the user reported nearly full device memory, new agents were stopped
and no fresh tester was started. Tool help was improved, and the scripted
first-task replay remains regression coverage, not proof of unfamiliar use.
The issue stays open for an independent transcript following
ONBOARDING-VALIDATION.md. No acceptance result or transcript is invented.

## Draft issue replies

These replies are drafts; none has been posted by this review. Implementation
links and the final validation results can be added before posting.

### #30 — implement, including an existing scan bug

The newest design comment provides enough detail to implement explicit
`dispose --read ID@REV` pins. Each input uses a full raw-file SHA-256 from
the read the reviewer judged; the whole set is compared inside the write
transaction. Conflicting pins, self-pins, changed inputs and unavailable
inputs are refused instead of silently recording newer bytes. The existing
`scanned` comparison also moves under the lock, fixing its wait-for-lock race.

`check` and `review` report the latest disposition's input status for each
exact action. Changed or unavailable inputs are advisory, unpinned reviews
are explicitly unbound, and the gate keeps its existing meaning. Notes do
not clear changed-input warnings. The pins remain separate from graph
references and approval-to-artifact binding. Cache pruning does not retain
or recover pinned bytes; retained historical bytes must be verified against
the raw-file hash.

### #65 — defer the unfamiliar-tester exercise; implement its help observation

The installed `show` help and generated guidance now explain that an ID can
identify either a plied record or a record from configured `[[sources]]`,
with paired examples. This directly addresses the latest field observation
without requiring knowledge of another record CLI.

The full unfamiliar-user exercise still needs a fresh tester using the existing
protocol. The implementation agents have already read the source, and this run
does not start another tester under the requested memory limit. The help fix and
scripted replays do not complete that acceptance criterion, so the issue remains
open for an independent transcript.

### #77 — implement differently

Context discovery remains read-only and explicitly says filesystem
writability is unverified. It explains that claims, renewals, messages,
acknowledgements and releases can need runtime writes even when the role
cannot edit tracked records. Logical write blockers and filesystem
writability remain separate claims.

No implicit create/delete probe is added. A mutation still reports its
actual filesystem failure, and a writable runtime grants no project
authority or permission to bypass a sandbox.

### #79 — implement

`check --baseline FILE` compares a saved complete JSON check report with
the full current report. It reports added, persisting and resolved entries
using diagnostic identity, plus source IDs and revisions for aggregated
stale warnings and changed review inputs. A source added under an existing
node warning is therefore visible without parsing message wording.

The current diagnostics and exit verdict remain authoritative. Comparisons
make no claim about who caused a change, write no notes and waive nothing.
Unsupported snapshots and incompatible warning or access-policy scopes
require a fresh baseline.

### #80 — implement

The generated skill now includes a compact participant path: discover local
authority rules, use a unique session, read the actual node and conversation,
review applicable commitments, claim/renew, work within scope, send an
authorized handoff and release only that session's claims.

The path explicitly distinguishes runtime access from record-edit authority.
A claim or acknowledgement grants neither approval nor permission to record
dispositions. An unresolved gate still stops the governed action; required
changes outside the participant's role go to the authorized record author.
The full authoring workflow remains available.

### #81 — implement

`show --freshness` and `frontier --freshness` now expose the existing stale
causes beside the records being read. They distinguish direct naming from
reference-based warnings and include readable source revisions, mentioned
IDs and reference-field edges in JSON.

This reuses the structural stale check without running unrelated project
validation, reading sealed evidence or claiming semantic accuracy. It adds
no review acknowledgement, changes no record and clears no warning.

### #82 — implement

`show ID --summary` provides optional compact orientation using current
metadata, the existing author-written summary when present, bounded recent
history and references for deeper reads. It marks omitted material and
provides a fallback when the record has no body summary.

Full `show` remains available. JSON retains the full existing record and
adds a separate summary view; no generated summary or persistent duplicate
is introduced. The orientation excerpt does not substitute for reading
original evidence or exact commitment wording before consequential review.

### #83 — implement

`check --group-stale` adds groups under each readable newer source record,
separating directly named nodes from nodes reached through reference fields.
JSON includes source revisions and the precise field/ID edges and does not
truncate the structured cause list.

The individual diagnostics, their identities and the check verdict remain
unchanged. This is presentation only: it neither reviews a group nor removes
attachment warnings. It remains useful independently of the design in #75.

### #75 — defer the semantic change

Both the attachment report and the new `made` example describe real repeated
review cost, and both follow §8 today. PR #76 addresses only commitment
`nodes`; it deliberately leaves `made` and explicit `links` contributing
to warnings.

This review's constraint is to preserve every currently required review.
Dropping either field from the watch set would clear existing warnings
without that review, so this change does not merge or duplicate that
semantic adjustment. Grouping and freshness views make the current work
easier to inspect. #30 separately detects amendments to the bytes of evidence
that a disposition explicitly read; it does not replace dated-mention
review. A future reduction needs a review-preserving coverage contract,
considered with #85, rather than treating every contextual mention as an
evidence-byte change.

### #85 — defer for a durable coverage design

The problem is real: a note about one source must not silently acknowledge
another unread source. Scoped notes need a persistent distinction between
the last general update and the exact source revisions individually
reviewed, plus comparisons under the write lock.

The design must cover multiple unread sources, source edits, missing or
sealed sources, concurrent updates and the interaction with later ordinary
writes. History has minute precision, so identifying scoped notes by their
timestamp alone is insufficient. This change preserves existing note
semantics and does not introduce partial acknowledgement until those rules
are specified and tested together.

### #78 — defer for generic configuration and provenance

A task-selection view could save useful work, but core needs a generic
selection schema and a clear provenance contract first: which linked record
and revision supplied each field, how absent values remain unknown, and
how a linked system's claim differs from a plied lease.

This change does not hard-code backlog fields or infer availability from
an open status. The next design should use representative projects to settle
field selection, bounds, access behavior and claim origins, while keeping
permission and scheduling decisions with the project's authoritative rules.

### #84 — defer the context bundle; implement authoring guidance

The independent question-authoring guidance is worthwhile now: record
choices, consequences, what approval authorizes or postpones, supporting
evidence, uncertainty and attributed recommendations using the existing
question fields and body. That guidance is added without new required fields.

The retrieval view still needs a contract for traversal edges, bounds,
deduplication, applicable commitment chains and partial-result disclosure.
In particular, describing an incomplete readable result must not enumerate
protected IDs or paths. This is narrower than export closure, but those
privacy and coverage limits still need to be explicit before adding a
question-context JSON interface.

### #31 — defer with the existing manual workaround

The current-record loss is real, but the maintainer's distinction matters:
withdrawing a conclusion, withdrawing only its rationale and reopening work
while retaining a valid conclusion are different claims. A format design
must define the retained outcome/evidence revision, scope, reason,
replacement, repeat withdrawals and stable downstream diagnostic identities.

Until then, a linked withdrawal decision can retain the old quote and
explain its scope. A checked reopen can clear `outcome`, link that decision
and supply `--why` in one write. This preserves the explanation manually;
it is not automatic withdrawal propagation or a claim that every dependent
conclusion became invalid. Coordinate the eventual design with #30.

### #33 — defer core integration; external reconciler first

Completeness needs an independently defined, versioned population. A useful
first reconciler can already consume `list`/`find --json`, compare stable
eligible-item IDs with forecast or explicit abstention records, and emit its
own report. The acceptance case should define duplicates, corrections and
missing inputs as well as the eligibility-list revision.

The maintainer's external-first direction stands. This change adds neither
merged external diagnostics nor a generic node disposition, and it does not
reinterpret a commitment's `not-triggered` as proof that an eligible item was
considered. Keep this as an acceptance case for the report-only validator
design in SCOPE.

### #34 — defer a core outcome vocabulary

Exercise the vocabulary against the worked research example first. Work
completion and evidence informativeness can vary independently; a single
core label could obscure that distinction. A linked interpretation record
can already carry project-specific metadata queryable through `find`, while
`outcome.quote` retains the stated conclusion.

If examples establish a recurring need, the optional writer-chosen field
should specify absent and unknown values, vocabulary changes,
reopening/withdrawal behavior, type and canonical position together. No
format field is added in this round.

### #35 — defer an evidence-selector contract

The deliberate whole-file quotation rule remains unchanged. Existing
free-text locators stay reader-facing; ordinary heading edits do not acquire
the power to change quotation validity.

A stricter opt-in mode needs a concrete selector grammar and fixtures for
repeated/nested headings, section ends, missing or ambiguous selectors,
frontmatter and ellipses across sections. Failure must never silently fall
back to whole-file matching. Keep this low-priority design open until a
concrete source supplies those cases.

### #37 — defer to the export design after operations

Retain all three acceptance requirements: distinguish current bytes from
the bytes actually examined; separate claimed author, supplier/relay and
verification; and disclose incomplete closure without leaking protected
IDs, paths, hashes, counts or relationships beyond an explicit disclosure
policy.

#30 makes explicitly examined hashes available, but a pin alone does not
retain exportable evidence. The exporter must carry the selected verified
bytes or say historical bytes are unavailable. As the maintainer requested,
no relay field or exporter is introduced before portable operations and
the reference-closure/trust design are settled.
