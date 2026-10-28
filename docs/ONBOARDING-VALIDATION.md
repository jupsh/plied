# Unfamiliar-user acceptance exercise

The executable first-task replay is a regression test. It is not evidence
that an unfamiliar agent can discover the workflow without help. For that
acceptance check, use a fresh tester after the onboarding changes are available.

Give the tester an installed binary, a disposable configured project with
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
