# Reusing a result within its scope

This synthetic example checks the record workflow proposed in [rStation
General post 45](https://rstation.ai/r/general/posts/45), particularly the
replies by `codex_arithmetic` and `lattice_lift_research`. It is not an agent
evaluation or a novel mathematical result.

Build Plied in the implementation repository, then run:

```sh
cargo build --locked
python3 examples/scope/replay.py --plied "$PWD/target/debug/plied"
```

The script creates an independent temporary Git repository. An optional
`--output /absolute/path/to/new-directory` chooses a destination and refuses
an existing directory. Each checked write reads the text and revision together;
IDs come from creation results. `trace.json` and `summary.json` retain the calls
and verified results. Dates reflect the replay's clock.

An elementary proof shows that `x² + y² = 3` has no integer solution: an
integer square is 0 or 1 modulo 4. The example then records three decisions:

| Proposed task | Authored reasoning | Resulting record |
|---|---|---|
| Rename x,y to u,v | The equation and integer assumptions are unchanged | Reuse the proof; close the duplicate task |
| Require x to be even | The proposed pairs are a subset of the excluded integer family | Reuse the proof; close the duplicate task |
| Allow real coordinates | The integer premise is absent; `(sqrt(3),0)` is a real witness | Keep the real-domain task open while the integer result stays closed |

The original closed record's revision is checked to remain unchanged. Changing
the question does not withdraw the old theorem. The open task is a representation
of ongoing work; the demonstration does not pretend the displayed real witness
is an unsolved mathematical problem.

Each new thread carries a short comparison in its body: object mapping,
assumptions, evidence, bounded conclusion and next step. The earlier result has
an `outcome` and a prose `revisit` condition. No fields or states are added.
The evidence source is explicitly included in the search allowlist so the
optional [related-work companion](../related/README.md) can retrieve it.

A protocol commitment explicitly adopted for this example requires recording
the comparison before a `decide` action. Each gate exits 6 without the action's
review and 0 after an authored disposition. The final `check` has no errors or
warnings. **That validates the record mechanics, not the reasoning.** Every
decision is supplied by the fixture; a passing gate does not establish that
an earlier theorem applies or that a changed premise justifies new research.

The project-wide commitment is a demonstration choice. Ordinary research
preferences should remain prose unless a project actually adopts an obligation.
Likewise, `uses` transmits applicable commitments, not mathematical validity.

For a future handoff evaluation, include both duplicate avoidance and correct
reconsideration, plus changed evidence and uncertain applicability. Compare
existing notes, notes plus a compact Markdown summary, and the same information
through Plied in independent fresh sessions. Keep authored answers out of the
agent inputs and account for record-authoring and reading cost. This replay
does not run those sessions.
