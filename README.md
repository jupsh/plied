# plied

A work record that lives in git, for teams of AI agents and people working on
the same project at once.

Plying is the step that twists single threads into one yarn. plied does the
same for work: each agent's line of work stays a separate, readable record,
and together they form one record that can be checked.

- **A work tree.** Nodes (threads) with a status, what they wait on, their
  schedule and approval, and, once closed, a quoted outcome and the problems
  they leave unresolved.
- **Commitments.** Promises made along the way ("if X, then Y"; "before Z, do
  W"), quoted word for word from the record that made them. They come up for
  review before any consequential action.
- **Questions.** Decisions that wait on someone with the authority to make
  them, answered only by a record that has that authority.
- **Messages.** Notes addressed to a node and delivered to whoever is on it now,
  with delivery, read and acknowledgement kept separate.
- **Literature.** Checked citations of cleared papers by PDF page, page search,
  page images and backlinks.
- **Checks.** Broken references, invalid states, dependency cycles and
  paraphrased "quotations" are errors. Stale nodes and unrecorded commitments
  are warnings.

Everything is plain Markdown with YAML frontmatter, or JSON lines, tracked in
git. Generated views make it easy to browse. There is no database, no server and
no daemon: it is one binary. Git is required; optional PDF extraction and
rendering also require a pdfium shared library.

```
$ plied tree
T001  M1: the reference species reproduces food sharing     open
├── T002  In-nest motion                                     closed, outcome D088, carries F068, F075
├── T003  Transfer to the 1999 experiments                   closed, outcome D107, carries F081, F093
│   ├── T004  Contact split, during vs after the main bout   open, next 1, lead
│   └── T005  Step-5 closure wording                         waiting ⟵ Q01
└── T006  Cause of colony synchrony                          open
    └── T007  S2 audit                                       waiting ⟵ Q02, approval: not approved
```

## Install

```
pipx install plied                # or `uv tool install plied`; prebuilt binary, no Rust needed
cargo install plied               # from crates.io
cargo install plied --features pdf   # adds PDF page text and page images (pdfium)
```

You need git. The PyPI wheels are built without the `pdf` feature; supplied
page text still works. For `plied lit extract` or built-in page rendering,
install with Cargo and `--features pdf`. Building from source needs Rust
1.89 or newer. The feature does not install pdfium: provide `libpdfium.so`
(Linux), `libpdfium.dylib` (macOS) or `pdfium.dll` (Windows) on the system
library search path, or place it beside the plied executable.

Verify the build with `plied context --json` (`result.capabilities.pdf`).
In a project with a cleared local PDF and no configured `text_dir`, run
`plied lit extract PAPER --json`, then `plied lit page PAPER 1`. A binary
without the feature reports that PDF support is absent; a missing shared
library reports that pdfium could not be loaded. The
[PDF smoke test](tests/pdf_cache.rs) exercises extraction, reading, citation
checking and search against a real PDF in CI.
To run it once without installing: `cargo run -- …` in a checkout.

## Quick start

```
cd your-repo
plied init                      # writes plied.toml
export PLIED_AGENT=alice        # who is acting (or pass --as on each command)
export PLIED_SESSION="$(date -u +%Y%m%dT%H%M%SZ)-$$"  # a new ID for each session

plied context --json            # verify the selected store, actor/action vocabularies and blockers
plied schema thread --json      # discover fields and conditional requirements
plied new thread --title "Fix a parser" --json  # returns the actual result.id and result.rev
plied tree
plied check
```

For an existing task, use the actual instructed node ID. Read `plied show
ID --json` and `plied messages ID`, review `plied review ID`, then claim it
with `plied claim ID`. Retain the revision that arrived with the text you
read; your next `set` or `note` uses that revision as `--expect`. An inbox
contains only messages addressed to you, so joining a node does not deliver
its older conversation. Renew `claim` manually if you do not use the Claude
hook. Read `plied skill` for the full workflow, including a useful handoff
before `plied release --this-session`.

For a runnable small example with an installed binary, use the
[first-task walkthrough](examples/first-task/README.md). It captures IDs and
revisions and checks expected conflicts, refused writes, gates and session
release in an independent temporary repository. The larger
[research example](examples/research/README.md) follows an evidence workflow.
See [troubleshooting](docs/TROUBLESHOOTING.md) for nonzero JSON verdicts and
recovery, and [development](docs/DEVELOPMENT.md) for contributing to the tool.

The [scope comparison example](examples/scope/README.md) shows how to reuse a
result for a renamed problem or covered subset while keeping a changed-premise
task open. The optional [related-work companion](examples/related/README.md)
retrieves candidate passages with keyword and local semantic matching. The
agent still judges whether the earlier result applies.

Records are written to `plied/threads/`, `plied/commitments/` and
`plied/questions/`. The view is generated in `plied/README.md`. Commit them
like any other file.

## Give it to an agent

```
plied skill > SKILL.md
```

This prints an agent skill generated by the installed binary from its own
version and command list, so it cannot drift from the plied that produced
it. It covers the habits `check` cannot teach: the show → `--expect` write
cycle, session setup, node conversation, quoting commitments, scan triage
and stale reviews. It needs no
repository. Regenerate it when plied is upgraded rather than editing the copy. With `--json` the result is
`{version, skill}` in the usual envelope.

## How it works

### Optional thread summaries

For a thread that is difficult to resume, add a short section in its body:

```markdown
## Summary
E001 excludes integer solutions, including even-coordinate subsets.
The real-coordinate case falls outside that proof and is being investigated
in T004. Check E001's assumptions before reusing the exclusion.

## Evidence and work
The full reasoning, source references and continuation notes go here.
```

This is ordinary author-written Markdown. It is optional, with no new
frontmatter field. Use `new --body FILE` when creating a thread. To update
an existing thread, read its full text and revision with `show`, preserve the
frontmatter and the rest of the file (including History), then use
`plied replace ID FILE --expect REV --why "refresh the synopsis"`.
`note` appends to History; a Summary heading there is not the current synopsis.

`context ID`, JSON `show`/`tree`/`frontier`, and the generated view expose it
for orientation. `body_summary` in JSON is read-only; human previews are
limited to 240 characters and show an ellipsis when shortened. The optional
retrieval companion searches it alongside original passages. The
[scope replay](examples/scope/README.md) demonstrates the workflow.

Include what the thread concerns, what is established and under which
conditions, what remains unresolved or should happen next, and where the
evidence lives. Keep it current when a substantive decision changes those
facts. Plied does not establish that a summary is accurate or up to date:
read the full record, evidence and exact commitment wording before acting.
The summary cannot grant approval, satisfy a gate or change a status.

### One canonical checkout

Every worktree of a repository shares one git common directory. plied treats
the **main worktree** as the canonical record. Every `plied` command reads and
writes the canonical copy, whichever worktree it runs in, so record files in
other worktrees are read-only snapshots. Runtime state that is never tracked
(the lock, presence claims, the revision cache, the search index) lives in
`.git/plied/` and is shared by all worktrees.

### Checked writes

Every change names the revision it was made against: `--expect <rev>`, where
the rev comes from `plied show`. Under a file lock, plied:

1. refuses the write if the canonical checkout is on the wrong branch
   (`canonical_branch`), has a detached HEAD, or is mid-merge, mid-rebase,
   mid-cherry-pick or mid-bisect;
2. refuses it if the record moved on since that revision. You get back the
   current record and a diff from the revision you read, then reconcile and
   retry. There are no silent overwrites;
3. refuses it if it would add a validation error. Errors that already exist
   elsewhere don't block unrelated work;
4. adds a dated history line naming the agent and session, then replaces the
   record and the regenerated view as one recoverable write. An intent journal
   in `.git/plied/` means a crash leaves a state plied can repair:
   `plied recover` (or the next write) finishes or undoes it, and refuses to
   guess if a file was edited in between. Until then, `check` fails and the
   commit guard refuses. The files still change one at a time, so a read
   during a write can see a mix of old and new.

If two agents read revision 7 and both write, the first write is accepted and
the second gets a conflict.

### Status, scheduling, approval and dependencies are separate

| field | meaning |
|---|---|
| `status` | `open` (nothing named stops it) · `waiting` (on `waiting_on`) · `held` (by a decision) · `parked` (set aside, with `revisit`) · `closed` (concluded, outcome recorded, whatever it was) · `group` (organisational) |
| `next`, `actor` | position in an actor's queue (open nodes only) |
| `approval` | `state: approved` or `not approved`, plus `by`: the record that decided it, checked for authority and scope |
| `waiting_on` | blocks. `uses` does not block (the node uses the other's results as they come). `parent` is organisational only |
| `outcome` | closed nodes only: `quote` and `ref`. The quote must appear word for word in `ref` |
| `carries` | problems still unresolved after closing |
| `due`, `revisit_on` | dates (`YYYY-MM-DD`): `check` warns once they pass and `frontier` lists them; they never block anything |

Closing a node records what happened. It doesn't mean success and it doesn't
approve anything that depends on it. A closed parent can have open children.

### Quotations are checked

`outcome.quote`, a commitment's `wording` and a citation's `quote` must appear
in the cited source after normalising whitespace, curly quotes, Markdown
emphasis and hyphenated line breaks. `…` marks omitted text, and the remaining
parts must appear in order. A paraphrase fails, so either quote the record
exactly or write the record first. A match only means the words are there. It
doesn't mean they support the claim.

### Authority

Pointing `approval.by` (or a question's `answered_by`, or the `resolved_by`
of a waived or superseded commitment) at a decision only counts if that decision says it had the
authority. Declare which fields hold this in the decision source:

```toml
[[sources]]
name = "decisions"
glob = "docs/decisions/*.md"
id = "^D\\d{3}$"
authority = "authority"    # user | delegated by D012
scope = "scope"            # the ids it approves: [T004, Q02]
```

A decision whose scope doesn't include the node is an error. A decision with no
authority field gives a warning that it can't be verified, never a pass. A
validator can check this evidence. It can't establish authorisation from a
label.

### Commitment review before consequential actions

```
plied review T004 --action run:fit-20261024 --gate
plied dispose P02 --action run:fit-20261024 --disposition not-triggered \
      --because "rule 1 not read yet" --expect <rev>
```

A commitment applies to a node in four ways:

- it's attached to the node directly (`nodes`, or the node's `links`);
- it's attached to anything the node reaches through `waiting_on` or `uses`;
- it's attached to an ancestor and says `applies: subtree`;
- it says `applies: project`.

`parent` alone never carries an obligation. Each commitment is shown with the
chain of links through which it applies. `--gate` needs `--action` and exits
with code 6 until every applicable commitment has a disposition for that action
and none of them is `blocks`. Dispositions for other actions don't count. A
commitment that names the action kinds it `governs` (from
`checks.action_kinds`) is left out of the gate for other kinds, and `review`
still lists it. Use the gate in your run scripts. Structure can't tell whether a
scientific trigger has fired: that's the reviewer's judgement, recorded with
its reason.

### Messages and presence

```
plied claim T004                          # "I'm on T004": an expiring lease, renewed by the hook
plied send T004 "Hold the rerun until D140 is read."   # to whoever is on T004, plus subscribers
plied send T004 "…" --to bob --id 01HX…   # explicit recipients; your own id makes retries safe
plied inbox                               # marks messages read (--peek doesn't)
plied ack <id>                            # receipt only, never approval
plied messages T004                       # the node's conversation
```

Messages are tracked: one append-only JSON-lines log per node in
`plied/messages/`. Each message records who was present when it was sent. Every
event has a unique id, so a retry with the same id is recorded once; reusing
an id for a different message is refused. Writes are
locked. A torn last line from an interrupted write is cut before the next
append.

Presence is a lease. It estimates that an agent is on a node; it doesn't prove
the agent is running.

**Claude Code** delivers messages at the agent's next prompt, or after a tool
call. Add this to `.claude/settings.json`:

```json
{
  "hooks": {
    "SessionStart":     [{ "hooks": [{ "type": "command", "command": "plied hook claude" }] }],
    "UserPromptSubmit": [{ "hooks": [{ "type": "command", "command": "plied hook claude" }] }],
    "PostToolUse":      [{ "hooks": [{ "type": "command", "command": "plied hook claude" }] }]
  }
}
```

`plied claim` picks up the session from `$CLAUDE_CODE_SESSION_ID`. The hook
is a small native binary and costs a few milliseconds per call. Leave out PostToolUse if
delivery at each prompt is soon enough. Other agents poll `plied inbox` and
claim by hand, so their presence is only as good as their claims.

### Literature

```toml
[literature]
manifest = "literature.json"   # {"papers": [{"id": "Smith2020", "file": "Smith2020.pdf", "sha256": "…"}]}
pdf_dir = "papers"
# text_dir = "cache/pages/{id}"  # your own page text (pNN.txt = PDF page NN); default: `plied lit extract`
# ocr_marker = "[OCR"            # pages whose text is OCR
```

Only papers in the manifest are cleared. A record cites them like this:

```yaml
cites:
  - paper: Smith2020
    page: 4
    quote: "The mean bout lasted 12.5 s"
    kind: table          # values from tables or figures must be read from the page image…
    image_checked: true  # …and say so
```

`plied lit check` reports five results for every citation, separately:
clearance, source hash, page existence, quotation and the image-check claim.
The image result records the writer's claim; plied does not inspect the image.
A citation without a
quote is fine for a table or figure value (its quotation result is
`unchecked`); anywhere else it gets a warning, since nothing ties it to the
page. Matches on OCR'd pages
are warnings to verify against the image. Other commands: `plied lit search`,
`plied lit page Smith2020 4 [--image]`, and `plied lit cites Smith2020`, which
finds citations and author–year mentions such as "Smith et al. (2020)".

### Search, with access rules

`plied index` builds a keyword (BM25) index of the records and cleared paper
pages that `[search]` allows. `plied search "donor volume"` queries it.

- **Sealed paths** (`[access] sealed`, plus `sealed_file` with one substring
  per line) are never read, indexed or shown, by any command, including when
  reached through a symlink. `show`, `find`, `check` and the views leave sealed
  records out. plied can't verify a link to a record it may not read, so
  `check` reports one as a broken reference. If a sealed file is configured but
  missing, plied refuses to read anything.
- The index stores a digest of the access policy: the sealing rules, the
  literature manifest and the allowlist. When any of them changes, the index is
  refused, never served, until it's rebuilt.
- Every hit is checked against the current policy before it's shown. A file
  that changed since indexing is marked.
- Ordinary `.git/` files are excluded. The generated runtime paper cache is
  a specific exception for cleared pages; source seals and explicit excludes
  still apply. Put anything else that must never be searched
  (e.g. pending results) in `exclude`.

Structured queries cover all records, plied's and linked ones:

```
plied find '{"kind": "thread", "status": "waiting", "waiting_on": "Q02"}'
plied find '{"kind": "decisions", "date": {"$gte": "2026-10-01"}, "body": {"$regex": "S2"}}'
plied backlinks D134
plied frontier
```

### Keeping record files out of code branches

```
plied guard install       # pre-commit and pre-merge-commit hooks (leaves existing hooks alone)
plied guard range main HEAD   # for CI or review: fails if HEAD brings record changes
```

The guards refuse a commit of record files from any worktree except the
canonical one. They also refuse staged record content that plied didn't write
(a hand edit), deleting a record, and a merge that brings in record content
plied didn't write. They protect the defined workflow, but someone who bypasses
the tool or the hooks (`--no-verify`) isn't stopped. A passing guard never
authorises a commit: your project's own commit rules still apply.

## Linking other record systems

plied doesn't take over your existing decision logs, findings or backlog. It
links to them. Each `[[sources]]` entry names a glob, the frontmatter field
holding the id, and an id pattern. It can also name fields for the date, time,
status, authority and scope, and set `scan_commitments` (warn about
conditionals and deferrals that no commitment records) and `require_linked`
(warn when no plied record links to an item). Linked records stay
authoritative: plied cites them and never restates them.

When a scanned record turns out to hold no commitment, say so on a thread, and
the warning stays quiet until the record changes:

```
plied scanned D134 --rev <D134 rev> --on T009 --because "describes a past deferral" --expect <T009 rev>
```

## Configuration reference

```toml
[plied]
format = 1                  # the on-disk format; a plied that doesn't know it refuses writes
requires = ">=0.1.0"        # plied versions that may write: "X.Y.Z" exactly, or ">=X.Y.Z"
root = "plied"              # where plied's records live
canonical_branch = "main"   # optional

[ids]                       # optional
thread = { prefix = "T", width = 3 }
commitment = { prefix = "P", width = 2 }
question = { prefix = "Q", width = 2 }

[[sources]]                 # any number; see above

[access]
sealed_file = ".sealed-paths"
sealed = ["held/"]

[search]
include = ["plied/**/*.md", "docs/**/*.md"]
exclude = ["logs/**"]

[literature]                # see above
manifest = "literature.json"
id_field = "id"             # or id_from_file = "^([^_]+)_"
pdf_dir = "papers"
text_dir = "cache/papers/{id}"
page_file = "p{page:02d}.txt"
ocr_marker = "[OCR"
image_command = ["tool", "{id}", "{page}"]   # optional renderer, instead of the built-in PDF renderer (runs this program)
image_path = "cache/img/{id}-p{page}.png"

[checks]
approve_authorities = ["user", "delegated"]
actors = ["lead", "user"]   # optional
commitment_phrases = ["\\bdeferred\\b", "\\bif\\b[^.]{0,80}\\bthen\\b"]
commitments_since = "2026-10-01"
action_kinds = ["run", "publish"]   # optional: actions must be kind or kind:instance

[messages]
lease_minutes = 30

[runtime]
revision_days = 14          # how long cached revisions (for conflict diffs) are kept
```

Every field is type-checked when the file is loaded: `sealed = "secret"` is an
error, not a list of six one-letter seals, and a misspelt key (`seald`) is an
error rather than a setting silently left at its default. Bad regular expressions and
templates are errors too. Treat `plied.toml` as part of your trust boundary:
`image_command` runs whatever program it names.

Environment: `PLIED_AGENT`, `PLIED_SESSION` (or `CLAUDE_CODE_SESSION_ID`),
`PLIED_RUNTIME` (move the runtime directory), `PLIED_CANONICAL` (for bare
repositories).

Exit codes: 0 ok · 1 error or failed check · 2 usage · 3 conflict · 4 the
change would add validation errors · 5 blocked (branch, merge state, format or
version; guard refusal) · 6 review gate.

Add `--json` to any command for one versioned result envelope on stdout,
failures included (see [FORMAT.md §11.1](https://github.com/jupsh/plied/blob/main/docs/FORMAT.md#111-json-results)).

The runtime directory caches the revisions plied reads and writes, so that a
conflict can show what changed. That cache holds content that may never have
been committed. It is pruned after `revision_days`; `plied gc --all` empties
it.

## What it does not do

- **Prove that a record preceded an event.** Record dates are claims by the
  writer: History and review times use the writing machine's clock, and git
  commit dates can be set by the committer. Precedence needs an anchor the
  writer cannot create or rewrite alone, such as an independently held
  checkpoint. The proposed receipts and attestations would need that anchor
  too; see [docs/SCOPE.md](https://github.com/jupsh/plied/blob/main/docs/SCOPE.md).
- **Guarantee anything for writes that bypass it.** Edit files by hand, or
  commit with `--no-verify`, and only `plied check` and `plied guard range` can
  still notice.
- **Coordinate across machines live.** Writes are local to one checkout.
  Agents elsewhere work in their own clones and send proposals, for example as
  messages.
- **Decide anything.** Approval, waivers and answers come from records with
  authority. Warnings never change a status.
- **Semantic (embedding) search in the core CLI.** Core search uses keywords.
  The optional [related-work companion](examples/related/README.md) experiments
  with local semantic retrieval; it supplies candidates for an agent to assess.
- **Keep secrets.** Sealing controls what plied reads and shows. Sealed files
  are still plaintext in the clone and in git history. There is no
  encryption, and no secure deletion.
- **Notify anyone, or run a server.** Building those on top of plied is the
  intended route; see [docs/SCOPE.md](https://github.com/jupsh/plied/blob/main/docs/SCOPE.md).

## Further reading

- [Worked research example](examples/research/README.md) — replay an interrupted
  experiment through recovery, comparison, interpretation and replication,
  with a tracked store and explicit limits of a passing gate.
- [Format specification](https://github.com/jupsh/plied/blob/main/docs/FORMAT.md) — the contract for files and
  command-line behaviour.
- [Scope](https://github.com/jupsh/plied/blob/main/docs/SCOPE.md) — what plied does itself, what it only makes
  possible, and what it leaves to other tools.
- [RFC 0001: portable operations](https://github.com/jupsh/plied/blob/main/docs/rfc/0001-operations.md) — draft.

## Licence

MIT.


