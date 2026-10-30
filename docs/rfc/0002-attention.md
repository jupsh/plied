# RFC 0002: One acknowledgement model, and fewer ways to fall behind

Status: **draft**. Nothing here is implemented. It builds on proposals that
are open as pull requests: scoped review notes (#92), kept prior outcomes
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
  The item is append-only, so it never changes.
- `commitment?`: a `scanned` item already is a revision-pinned
  acknowledgement with a reason. Keep `scanned` as the stored form, but
  accept it from `--reviewed` on a thread with a `--because`, so there is
  one verb.
- `review-input` stays as it is. It concerns the evidence for a gate, and a
  new disposition is the right act, not a note. Its `acknowledge` points to
  `dispose … --read …`.
- `date` stays as it is. It is a calendar, not a record.

### 3. Strict acknowledgement, opt-in

`[checks] acknowledge = "named"` makes ordinary writes stop clearing
`stale` and `withdrawn`: only a scoped review of the causes does. The
default stays `"any-write"` for existing stores. The skill recommends
`"named"` for projects where several agents write. Within a store the rule
is uniform, so an agent learns one.

### 4. One orientation flag

Replace `--summary`, `--freshness`, `--linked` and `--refs` on `show` and
`frontier` with `--with summary,freshness,linked,refs` (any subset;
`--with all`), and keep the old flags as aliases for one minor version.
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
meaning. Part 3 is opt-in. Part 4 keeps aliases for one minor version.
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
  edits to the withdrawn thread.
- `--with` and the old flags produce identical JSON.
- A write adding `outcome.labels` under `requires = ">=0.11.0"` is refused,
  and accepted after raising it; a write not touching labels is not.

## Open questions

- Should `scanned` items become scoped review lines too, so there is one
  stored form as well as one verb? That would change an existing field.
- Should `acknowledge = "named"` become the default at the next format?
- Does `causes` need a bound for diagnostics with very many sources, as the
  human stale preview has, and if so how is the truncation marked?
