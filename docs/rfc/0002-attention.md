# RFC 0002: One acknowledgement model, and fewer ways to fall behind

Status: **draft**. The additional changes proposed below are not implemented.
The baseline features have their own contracts in FORMAT.md: scoped review notes (#92), kept prior outcomes
and the `withdrawn` warning (#98), linked-work and one-hop orientation views
(#94, #95), and outcome labels and heading locators (#97, #100).

## Summary

plied asks an agent to look at things in several ways, and each one is
cleared by a different rule. This RFC proposes one model for all of them: a
diagnostic that asks for attention names its **causes** as exact record
revisions, and one acknowledgement, naming those revisions, clears exactly
those causes. It also proposes folding the growing set of orientation flags
into one, and making plied refuse to write a feature that the store's
`requires` would not let every allowed version read.

## Motivation

Two fresh agents worked a task with plied 0.11.0 and a build of #93, using only
the binary (docs/ONBOARDING-VALIDATION.md). Both finished. What cost them most
was not any one command but the number of rules to learn. Counting the open
proposals, an agent meets these:

| warning | what it means | what clears it |
|---|---|---|
| `stale` | a newer linked record mentions the node | any write to the node; with #92, a scoped review of that revision |
| `withdrawn` (#98) | a referenced thread's conclusion was withdrawn | any write to the node after the withdrawal |
| `commitment?` | a linked record has a commitment phrase no commitment links | a `scanned` item at that revision, on any thread |
| `review-input` | evidence a disposition pinned has changed | a new disposition for that action |
| `date` | a date has come | changing the date or the state |

Three things follow.

1. **Clearing is uneven in strength.** `review-input` and `scanned` are
   bound to the exact bytes reviewed. `stale` and `withdrawn` are cleared by
   any later write, including `set next=2` by someone who never read the
   cause. The weakest rule is the one agents hit most often.
2. **The cure is not in the diagnostic.** Each message says in prose what
   to do. An agent reading `--json` must map a code to a procedure it learned
   from the skill.
3. **Each new kind of attention invents its own rule.** #92 and #98 each
   had to define one.

A later case, [#106](https://github.com/jupsh/plied/issues/106), exposes a
second cost in `review-input`: correcting a formula's description in a design
record required 31 replacement dispositions across five completed actions and
11 commitments. Both implementations already used the intended formula; the
correction changed no computed result. Detecting the new evidence bytes was
useful. Restating the impact assessment as 31 new action dispositions was the
cost. This case motivates the separate historical assessment in §2a below.

Separately, orientation now has one flag per idea (`--summary`,
`--freshness`, `--group-stale`, and with #94 and #95 `--linked` and `--refs`),
and each optional field or key added within format 1 breaks older binaries
on that store, so every changelog entry asks stores to raise `requires` by
hand.

## Proposal

### 1. Causes on every attention diagnostic (additive; can ship first)

Every warning that asks a reader to look at something gains, under
`--json`, `causes: [{record, rev, part?}]`: the exact revisions that raised
it (`part` names a list item when the cause is one, such as
`prior_outcomes[0]` or `reviews[3].inputs[1]`). It also gains `acknowledge`:
the argument vector of the write that would clear it, with placeholders only
for what the agent must supply:

```json
{"level": "warning", "code": "stale", "where": "plied/threads/T001-fit.md", "subject": "named",
 "causes": [{"record": "D140", "rev": "3f2a…"}, {"record": "D141", "rev": "9b0c…"}],
 "acknowledge": ["note", "T001", "<what you concluded>", "--reviewed", "D140@3f2a9c1e5b7d",
                 "--reviewed", "D141@9b0c22aa31f0", "--expect", "<T001 rev you read>"]}
```

`comparison_basis` already records causal sources for `stale` and
`review-input`; this generalises that and exposes it.

### 2. One acknowledgement: a scoped review

`--reviewed ID@REV` (#92) becomes the acknowledgement for every cause that
is a record revision, on any write to the node that holds the warning:

- `stale`: the linked record's revision (as #92).
- `withdrawn`: the thread's revision is the wrong unit, since any later edit
  would change it. Name the item instead: `--reviewed T003#prior_outcomes[0]`.
  Checked writes preserve the item as an unchanged prefix. That does not
  establish its identity across direct edits or rewritten git history. The
  final syntax must also bind the item's content digest and specify the
  bytes that digest covers; an index alone is not enough. This part needs
  that design before implementation.
- `commitment?`: a `scanned` item already is a revision-pinned
  acknowledgement with a reason. Keep `scanned` as the stored form, but
  accept it from `--reviewed` on a thread with a `--because`, so there is
  one verb.
- `review-input`: a new disposition is still the act that supplies a new
  evidence claim for an action, and its ordinary `acknowledge` points to
  `dispose … --read …`. A correction assessed after completed actions is a
  different claim; §2a proposes a bounded historical acknowledgement without
  replacing those actions' original dispositions. Neither form changes the gate.
- `date` stays as it is. It is a calendar, not a record.

### 2a. Historical evidence-impact assessments (design proposal, #106)

Keep two claims separate:

1. **What the original review read.** Its disposition and `inputs` identify
   the evidence available to that reviewer at that action. A later correction
   cannot change those historical bytes.
2. **What a later reviewer concluded about the correction.** A reviewer can
   assess a particular change and conclude that it leaves specified completed
   actions' recorded bases unaffected. That is a new, attributed judgment;
   plied cannot infer it from a small diff or the word "correction".

Propose one explicit assessment covering a finite selection of historical
reviews. The design must bind all of these:

- The evidence record, its original full revision and the corrected full
  revision the reviewer actually read.
- Every covered commitment, exact action and original disposition item,
  including that item's content identity. A list index or an action name
  alone is insufficient: later replacement dispositions for the same action
  must not inherit the assessment. The item-digest design needed for
  `withdrawn` acknowledgements also applies here.
- The reviewer's identity, time, explanation and explicit assertion that the
  selected actions are complete. Passing a gate, closing a thread or
  fulfilling a commitment does not establish execution or completion of
  every action. Completion is a caller-supplied assertion, not a fact that
  plied has independently verified.

One explanation may cover several explicitly selected reviews when the
reviewer judged the same correction against each of them. There is no
wildcard for all actions, all commitments or future reviews. The selection
must be visible before writing and in the resulting audit record.

The write must compare the corrected evidence and every selected commitment
against the full revisions the reviewer read, under the write lock. It must
validate the selected disposition items and their original pins, and reject
the whole assessment if any precondition changed. It must refuse unavailable
evidence without probing protected content. The audit record must be durable
and carried with the records it qualifies; a runtime cache cannot hold the
only copy. Its representation and compatibility requirements remain open.

The effect is deliberately narrow:

- Preserve the original dispositions, their pins and their append-only order.
  Report that the original input changed and that the exact change was
  assessed as leaving specified historical reviews unaffected. Do not present
  the old pin as current or claim the old reviewer read the corrected bytes.
- A valid assessment can acknowledge only the `review-input` attention for
  those selected review/input pairs at the corrected revision. Other changed
  inputs, warnings and reviews continue to ask for attention.
- A further evidence edit asks for attention again. A new disposition for the
  same action is a new target and receives no inherited acknowledgement.
- Gate verdicts remain exactly as recorded, including `blocks`. The assessment
  supplies no missing disposition, changes no commitment state, and cannot
  authorize a new action or replace a current review before its execution.
- If the reviewer finds that the correction changes an action's basis, record
  that impact and any required follow-up explicitly. Do not erase the original
  review or label the correction unaffected to clear a warning.

This is an extension to the attention design, not a new command or stored
field in the current release. In particular, an ordinary `note --reviewed`
continues to leave `review-input` warnings intact. Do not add a second batch
write format just for this case; coordinate any multi-record application and
receipt with RFC 0001.

### 3. Strict acknowledgement, opt-in

`[checks] acknowledge = "named"` makes ordinary writes stop clearing
`stale` and `withdrawn`: only a scoped review of the causes does. The
default stays `"any-write"` for existing stores. The skill recommends
`"named"` for projects where several agents write. Within a store the rule
is uniform, so an agent learns one.

### 4. One orientation flag

Replace `--summary`, `--freshness`, `--linked` and `--refs` on `show` and
`frontier` with `--with summary,freshness,linked,refs` (any subset;
`--with all`), and retain the old flags as compatibility aliases. Removing
them would require a separate deprecation plan. A command only accepts the
views it supports; an unsupported view is a usage error, never silently
ignored. Default output and existing result keys remain unchanged.
`check --group-stale` stays: it changes the shape of a different command.
`plied context` lists the available views so an agent can discover them
without the skill.

### 5. Writes refuse features `requires` does not cover

Each optional field, subfield and `plied.toml` key added within format 1
has a version (FORMAT.md already records it). A checked write that would
put such a field into a record, while the store's `requires` lets in a
version older than that field, is refused with exit 4 and a `requires`
diagnostic that names the field and the line to set. Loading a
`plied.toml` that uses a newer key warns the same way. This turns "the
changelog asks you to raise `requires`" into something plied enforces when
it matters, before an older teammate's binary meets an unknown field.

## Non-goals

- Changing what any warning means, or its identity. Causes are added beside
  existing identities; `comparison_basis` entries keep their form.
- Acknowledging errors. Errors are fixed, never acknowledged.
- Interpreting labels, statuses or linked fields. Acknowledgement records
  that a reader looked, not what is true.

## Migration

Part 1 is additive JSON. Part 2 extends #92's syntax; old notes keep their
meaning. Part 3 is opt-in. Part 4 retains existing flags as compatibility aliases.
Part 5 can refuse writes in stores whose `requires` is lower than the
fields they already use; the refusal only applies to writes that add such a
field, so existing records stay writable, and `check` reports the gap first
as a warning for one minor version.

## Tests the implementation must have

- Every attention code's `acknowledge` vector, run as given with the
  placeholders filled, clears exactly that warning and no other.
- Under `acknowledge = "named"`, `set next=2` on a stale node leaves it
  stale; a scoped review of one cause leaves the others.
- A `withdrawn` acknowledgement names an item index and survives later
  unrelated edits to the withdrawn thread. It must not cover a changed
  item at the same index after a direct edit or rewritten history.
- `--with` and the old flags produce identical JSON.
- A write adding `outcome.labels` under `requires = ">=0.11.0"` is refused,
  and accepted after raising it; a write not touching labels is not.
- Reproduce #106 with 31 historical review/input pairs across several
  completed actions and commitments. One explicitly scoped assessment covers
  the selected change; all original dispositions and pins remain byte-for-byte
  unchanged, and gate verdicts (including a blocking review) are unchanged.
- Leave one review and one second changed input outside that assessment.
  Both keep asking for attention. The original changed pin remains observable
  alongside the later assessment; it is never reported as the corrected pin.
- Change the evidence again, or append a new disposition for the same action.
  The prior assessment covers neither. A changed disposition at the same list
  index after a direct edit must not retain the old assessment either.
- Change any selected commitment or the corrected evidence while the assessment
  waits for the lock. Reject the entire write, with no partial acknowledgement.
  Unknown targets, mismatched original pins, and missing or sealed evidence
  also refuse without leaking protected data.
- Prune the revision cache and round-trip the stored assessment. Its target
  identities and audit explanation survive, while unavailable historical bytes
  remain explicitly unavailable rather than recovered from the current source.

## Open questions

- Should `scanned` items become scoped review lines too, so there is one
  stored form as well as one verb? That would change an existing field.
- Should `acknowledge = "named"` become the default at the next format?
- Does `causes` need a bound for diagnostics with very many sources, as the
  human stale preview has, and if so how is the truncation marked?

- Which exact representation does a withdrawn item's digest cover, and
  how is that digest carried in a history line and checked on later reads?
  List position is a locator, not a content identity.
- Where should a historical evidence-impact assessment live so one statement
  can cover several commitments, remain append-only and travel with an export?
  How is its finite target selection represented without creating an unchecked
  reference or leaking a target hidden by the access policy?
- What exact bytes identify a disposition item, and how does the assessment
  bind that digest as well as the full record revision checked at write time?
- How should a caller assert and support completion of the selected actions?
  The implementation must not infer it from existing gate or thread states.
- How should JSON expose the historical changed pin and its later assessment
  while preserving diagnostic identities and existing exact-action gate behavior?
