# An interrupted experiment, recovered and concluded

This is a synthetic worked example. All measurements, people and conclusions
are invented. The [tracked store](store/) preserves the original unpinned
0.7.0 example and remains valid. [replay.py](replay.py) requires plied 0.11.0
or later: it builds the workflow from scratch with checked writes and adds
explicit evidence revision pins to the run 02 review. It asserts gate results
and advisory input warnings along the way. It uses Python's standard library,
git and a plied binary. No network or external services are needed.

From the repository root:

```sh
cargo build
python3 examples/research/replay.py --plied "$PWD/target/debug/plied"
```

The script creates a temporary, independent git repository and prints its
location. It prints each command and the record read before each `--expect`
write and each input read used for `--read`. `--output
/absolute/path/to/new-directory` chooses another destination; it refuses an
existing directory. The replay's input pins and generated History and review
dates differ from the older tracked snapshot.

To inspect the final snapshot without replaying, copy it **outside this git
repository** and initialise the copy:

```sh
cp -R examples/research/store /tmp/my-plied-research-example
git -C /tmp/my-plied-research-example init -b main
plied -C /tmp/my-plied-research-example check
plied -C /tmp/my-plied-research-example tree
plied -C /tmp/my-plied-research-example show T001
```

Choose an unused destination. Running `plied -C examples/research/store` inside
this repository would select this repository's canonical root, not the example.
No `.git` directory or runtime cache is included in the tracked example. To use
commit guards on a copied snapshot, establish provenance by replaying the writes;
the copy alone has no runtime `written.txt` evidence.

## What happens

| Step | Evidence and record | What the replay checks |
|---|---|---|
| Declare criteria | [E001](store/evidence/E001-plan.md) defines control, dose and effect thresholds; P01 quotes the control requirement, P02 the replication requirement | Both commitments attach to T001; T002 and T003 reach them through `uses` |
| Interrupt | [E002](store/evidence/E002-interruption.md) preserves partial run 01 and the missing dose check | `interpret:run-01` is `blocks`; its gate exits 6 |
| Recover with a deviation | [E003](store/evidence/E003-recovery.md) records the replacement sensor, controls, dose and fresh run 02; T001 notes the recovery | Partial observations remain in E002 and are excluded; the new action needs its own review |
| Interpret once | [E004](store/evidence/E004-comparison.md) applies E001's criteria after the control review | The replay pins the explicitly read E001/E003 bytes; `interpret:run-02` passes after P01's disposition; T002 closes with the bounded quotation and carries Q01 |
| Replicate | [E005](store/evidence/E005-replication.md) records run 03 against the same criteria | `interpret:run-03` initially exits 6 despite the run 02 review, then passes after its own review |
| Conclude | [E006](store/evidence/E006-closure.md) records the replication interpretation and bounded closure | `close:study-01` needs P02's disposition; T001 and T003 close, carrying Q01 |

The final store has three closed threads, two commitments with their review
history, an open question about generalisability, and six linked evidence
records. `check` reports zero errors and zero warnings. Closure means the
declared work concluded, not success or certainty. The recurring commitments
stay open so a fresh action still needs a fresh review.

`governs=[interpret]` and `governs=[close]` come from the plan's wording and
the configured action vocabulary. They are not inferred by plied. A script
that selects the wrong action kind can omit an obligation; read the review's
excluded commitments as well as the applicable ones.

## What the gate does not establish

- **Unchanged evidence.** `ref` alone remains unchecked prose, as in the
  older tracked snapshot. The replay explicitly reads E001 and E003 and
  records their full raw-byte SHA-256 revisions through repeatable `--read`.
  After closure it temporarily appends a failed-control correction to E003.
  `check` reports `review-input`, and `review` shows the latest run 02
  disposition's input as changed, while `interpret:run-02` still exits 0.
  The old `satisfied` disposition remains the latest for that exact action;
  input warnings do not change gate verdicts. The replay then restores the
  original bytes. A new disposition after reviewing current evidence can
  replace the evidence claim; an unrelated note cannot. Unpinned reviews
  remain explicitly unbound.
- **Retained evidence.** A pin records a hash, not the bytes themselves.
  `revs/` can expire or be emptied by `gc --all`. Preserve exact source bytes
  elsewhere, for example in git, and verify recovered bytes against the pin.
  A raw-file SHA-256 is not a git object id. The gate does not evaluate the
  measurements or recover historical evidence.
- **Who reviewed.** `reviews.by` says `example-reviewer` because that is what
  the caller supplied. It is an attribution claim, not an authenticated
  identity. History and review dates likewise do not establish independent
  preregistration or precedence over an external event.
- **Independence.** The same fictional reviewer evaluates both runs. plied
  does not enforce a second reviewer, independence of a replication, or the
  scientific adequacy of these thresholds. Those are project policies.
- **Withdrawal propagation.** If the evidence later invalidates an outcome,
  this store has no special outcome-withdrawal operation. Reopening currently
  requires clearing `outcome`; the quoted prior conclusion survives in git or
  a separately written evidence record. A passing gate cannot settle it.

This exercises the optional [review revision pins](https://github.com/jupsh/plied/issues/30)
implemented in 0.11.0 and gives [outcome withdrawal](https://github.com/jupsh/plied/issues/31)
a concrete case for further design. Approval-to-artifact revision binding also
remains proposed. A forecasting variant would additionally need an independently
defined eligibility list and explicit abstentions; this example makes no
ledger-completeness claim.
