# Unfamiliar-user acceptance exercise

The executable first-task replay is a regression test. It is not evidence
that an unfamiliar agent can discover the workflow without help. For that
acceptance check, use a fresh tester after the onboarding changes are available.

plied's users are mostly agents, so a fresh agent with no access to this
repository or its history is an unfamiliar tester; it needs no human. Give the tester an installed binary, a disposable configured project with
at least two nodes and non-default actor/action vocabularies, and an
instruction to work on one named node. Include a quoted commitment, a prior
node message not addressed to the new agent, and a second session's claim.
The tester may use the binary's help, `context`, `schema` and `skill`, but
should not inspect the implementation repository. Keep contributor
orientation as a separate exercise using AGENTS.md and DEVELOPMENT.md.

Record every tool call, documentation consulted, failed attempt, and any
maintainer explanation. Introduce one concurrent update after the tester
reads a record; let the tester handle the resulting conflict. Observe whether
the tester can discover settings, read the node/conversation/commitments,
establish and renew a unique session, make a checked update, interpret a
closed gate and a nonzero JSON verdict, and leave a useful handoff while
releasing only its own session's claims.

To plant the concurrent update without telling the tester, watch the
modification time of the node's file in the runtime revision cache
(`revs/<rev>`): `show` refreshes it when the tester reads the node. A short
while after it moves, make a checked write to the node as another agent.

Use this report template:

| Item | Record |
|---|---|
| Binary version and build capabilities | |
| Initial task and project fixture | |
| Tester familiarity before starting | |
| Tool calls and documentation consulted | |
| Failed attempts and how they recovered | |
| Maintainer explanations required | |
| Conflict handling and revision used | |
| Gate verdict and disposition reasoning | |
| Handoff text and remaining claims | |
| Outcome and unresolved usability gaps | |

Preserve a transcript from the disposable exercise. Do not report this
acceptance check as passed solely because an implementer's scripted replay
passes. Issue #65 remains the place to record an actual unfamiliar-tester result.

## Recorded runs

**2026-10-30, plied 0.11.0.** A fresh agent with no repository access
was given a git project with a group thread, a code task (T002) and its
sibling, a linked decision, a quoted commitment governing `merge`, a lead's
message to a reviewer, a reviewer's claim on T002 and another session's
claim on T003. Its instructions allowed only the binary's help and guidance.
It finished the task unaided: it oriented, claimed, reconciled the planted
conflict from the exit-3 diff, gated the merge (exit 6), disposed, passed
the gate, merged, closed the thread, fulfilled the commitment, left `check`
clean and released only its own session.

Its gaps, and what changed in response:

| Gap | Change |
|---|---|
| The skill's session id (`date-$$`) changes per command in agent harnesses, and history shows only its first 8 characters, the same for every session started on one day | The skill's id starts with random hex, keeps an existing `PLIED_SESSION`, and says to keep it for the whole session |
| "Read the project's authority rules" pointed nowhere | The skill says plied holds no authority rules, where they usually are, and what to do when there are none |
| Nothing said who commits record files, or where | *Records in git* in the skill |
| Pinning the node being worked on guaranteed a `review-input` warning after the next note | The skill says to pin the evidence judged, not the node about to be written |
| `dispose --help` listed no dispositions; `--ref` and `--expect` had no help | Help text for every `dispose` argument and for `--expect` on `note` and `replace` |
| No guidance on `outcome.ref` without a results record, or on `resolved_by` for `fulfilled` | Both explained under *Closing* |
| `claim` didn't say that a reviewer already held the node; lease times had no date | `claim` lists the others on the node (`others` in JSON); leases show their date |
| No guidance on a message addressed to someone else | Under *Messages and presence* |
| The core workflow was hard to find among research-specific sections | *A task, start to finish* at the top of the skill |
