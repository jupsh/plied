# plied format specification

This document is the contract for plied's files and command-line behaviour. An
implementation conforms when it reads and writes these files as described here
and its commands behave as described here. Anything this document leaves open
(wording of messages, all text output other than `--json`, key order and
whitespace in JSON, the layout of files in the runtime directory that are
marked private) is up to the implementation. `tests/conformance.rs` holds
fixtures for the behaviour most likely to be read two ways.

**Versions.** Two numbers govern compatibility.

- `[plied] format` is the version of the on-disk format this document
  describes: format **1**. An implementation knows a set of formats. It MUST
  NOT write a store in a format it does not know: writes are refused (exit
  5), and `check` reports a `version` error. Reads go ahead, with a note that
  they may misread. A later format will come with a migration that rewrites
  records through checked writes. Format 1 needs none.
- `[plied] requires` limits which plied versions may write the store:
  `"X.Y.Z"` for exactly that version, or `">=X.Y.Z"` for that version or a
  later one. A version it excludes is refused like an unknown format. The
  tool's own version is provenance: `check` reports it, and it never decides
  what a record means.

Within one format, the rules on unknown things are fixed:

- unknown record fields are errors (§3.3);
- unknown `plied.toml` keys and tables are errors (§2);
- events of an unknown type or `v` are skipped and reported (§5);
- the `--json` envelope may gain fields but never loses or retypes one
  without a new envelope version (§11.1).

A later plied version may add optional record fields within format 1. An
earlier version reports them as unknown fields, so a store that uses one
should set `requires` to the version that added it. Each field below that was
added after 0.1.0 says when.

The key words MUST, MUST NOT, SHOULD and MAY are used as in RFC 2119.

## 1. Where things live

**Canonical checkout.** In a git repository, the main worktree (the one that
owns the git common directory) holds the record. Every command, in any
worktree, reads and writes the canonical checkout's files. A bare repository
names its canonical checkout with `PLIED_CANONICAL`. Outside git, the canonical
checkout is the nearest directory upwards containing `plied.toml`.

**Tracked files**, all under the canonical checkout:

| path | contents |
|---|---|
| `plied.toml` | configuration (§2) |
| `<root>/threads/*.md`, `<root>/commitments/*.md`, `<root>/questions/*.md` | records (§3); directory names are configurable |
| `<root>/messages/<node>.jsonl` | message logs (§5) |
| `<root>/README.md` | the generated view (§7) |

`<root>` is `plied.root`, default `plied`. These are the *managed paths*: only
plied writes them.

**Runtime directory**, never tracked: `<git common dir>/plied/`, or
`.plied-runtime/` in the canonical checkout outside git, or `$PLIED_RUNTIME`
when set. Its contents are in §6.

## 2. `plied.toml`

TOML. Every field has the type given here; a value of another type (for
example a string where a list of strings is expected) MUST be rejected when the
file is loaded, never coerced. A table or key not listed here is an error too,
so that a misspelling (`seald`) cannot silently leave a default in force.
Regular expressions and templates MUST be checked when the file is loaded.

| table | field | type | default | meaning |
|---|---|---|---|---|
| `plied` | `format` | positive integer | 1 | the on-disk format (*Versions*) |
| | `requires` | string: `"X.Y.Z"` or `">=X.Y.Z"` | none | plied versions that may write the store (*Versions*) |
| | `root` | string | `"plied"` | relative path of plied's records; no `..`, not absolute |
| | `canonical_branch` | string | none | writes are refused while the canonical checkout is on another branch |
| `ids.<kind>` | `dir` | string | `threads`, `commitments`, `questions` | directory under `<root>` |
| | `prefix` | string | `T`, `P`, `Q` | id prefix; prefixes MUST be distinct |
| | `width` | positive integer | 3, 2, 2 | minimum digits |
| `[[sources]]` | `name` | string | required | the source's name, used as the record kind |
| | `glob` | string | required | files, relative to the canonical checkout (§2.2) |
| | `id` | regex | required | an id must match at its start |
| | `id_field`, `title_field` | string | `id`, `title` | frontmatter fields |
| | `date`, `time`, `status`, `authority`, `scope` | string | none | frontmatter fields holding these |
| | `scan_commitments`, `require_linked` | boolean | false | warnings, §8 |
| `access` | `sealed` | list of strings | `[]` | sealed substrings (§9) |
| | `sealed_file` | string | none | file of sealed substrings, one per line, `#` comments |
| `search` | `include`, `exclude` | list of strings | `["<root>/**/*.md"]`, `[]` | search allowlist and denylist (§2.2) |
| `literature` | `manifest` | string | required if the table exists | JSON manifest (§10) |
| | `list` | string | `papers` | key of the paper list in the manifest |
| | `file_field`, `hash_field` | string | `file`, `sha256` | manifest entry fields |
| | `id_field` | string | `id`, or none when `id_from_file` is set | manifest entry field holding the paper id |
| | `id_from_file` | regex with a group | none | the id is group 1 of a search in the file name |
| | `pdf_dir`, `text_dir` | string | none | `text_dir` is a template with `{id}` |
| | `page_file` | template with `{page}` | `p{page:02d}.txt` | page text file name |
| | `ocr_marker` | string | none | page text containing it is OCR |
| | `image_command` | list of template strings | none | page renderer; `{id}`, `{page}`, `{file}` |
| | `image_path` | template | none | where `image_command` writes; `{id}`, `{page}` |
| `checks` | `approve_authorities` | list of strings | `["user", "delegated"]` | |
| | `actors` | list of strings | none | allowed `actor` values |
| | `commitment_phrases` | list of regexes | see below | matched case-insensitively |
| | `commitments_since` | string `YYYY-MM-DD` | none | |
| | `action_kinds` | list of strings, without spaces or `:` | none: any | the kinds an action may have (§12) |
| `messages` | `lease_minutes` | positive integer | 30 | |
| `runtime` | `revision_days` | integer, 0 or more | 14 | days the revision cache keeps content after it was last read or written (§6) |

Default `commitment_phrases`: `\bdeferred\b`, `\bpending\b`,
`\bif\b[^.]{0,80}\bthen\b`, `\bbefore\b[^.]{0,40}\bany\b`.

`image_command` runs a program. `plied.toml` is therefore part of the trust
boundary.

### 2.1 Regular expressions and templates

Regular expressions use the common Perl-style syntax, including lookaround.

A template is text with fields `{name}` or `{name:spec}`; `{{` and `}}` are
literal braces. `spec` is `[[fill]align][0][width][type]`: `align` is one of
`< > ^`, `0` pads a number with zeros, `width` is a minimum width in
characters, `type` is `d` (number) or `s` (string). An unknown field or spec is
a configuration error.

### 2.2 Patterns over paths

Paths are relative to the canonical checkout with `/` separators.

- `glob` in `[[sources]]` selects files: `*`, `?` and `[...]` match within one
  path component; a component `**` matches zero or more directories; a
  wildcard component does not match names starting with `.` unless the
  pattern component itself starts with `.`.
- `include`/`exclude` patterns match whole paths: `*` matches any run of
  characters including `/`; a leading `**/` may also match nothing; `/**/` may
  also match a single `/`.

## 3. Record files

A record is a UTF-8 file named `<ID>-<slug>.md` (or `<ID>.md`) in its kind's
directory: YAML frontmatter between two `---` lines, then a Markdown body.

**Encoding.** Reading ignores a leading byte-order mark and reads CRLF line
ends as LF. A record that is not valid UTF-8 is a `parse` error; a linked
source's file is read with invalid bytes replaced. plied writes UTF-8 with no
byte-order mark, LF line ends and a final newline, so its next write of a
file normalises it. None of this touches the revision, which is always the
hash of the raw bytes.

```
---
id: T004
title: Contact split, during vs after the main bout
parent: T003
status: open
next: 1
actor: lead
links: [D134 §2, F093]
---
Free text.

## History
- 2026-10-24 10:31 UTC lead (session 1a2b3c4d): created — split before the rerun
```

**Ids** are the kind's prefix followed by at least `width` digits. A new
record gets one more than the highest number used by any file in the
directory, including files that fail to parse, so ids are never reused. The
slug is the first six runs of `[a-z0-9]` in the lowercased title joined with
`-`, or `item`.

**Revision.** A record's revision is the lowercase hex SHA-256 of its bytes.
Commands that change a record take the revision the caller read
(`--expect`, at least 8 leading hex digits).

### 3.1 Frontmatter subset

Frontmatter is a strict subset of YAML in which every scalar is a string.

- A mapping of `key: value` lines. Keys match `[A-Za-z_][A-Za-z0-9_.-]*`. A key
  repeated in one mapping is an error.
- Nesting by indentation with spaces; a tab in indentation is an error. The
  top level starts at column 0.
- Block lists (`- item`), at the key's indentation or deeper. An item may be a
  mapping (`- key: value` followed by more keys aligned with the first).
- Flow lists of scalars: `[a, "b, c", 'd']`. Nested collections inside a flow
  list are errors; so is an empty item. As in YAML, a quote opens a quoted
  item only as the item's first character after spaces. Anywhere else it is
  part of a plain item, so `[a, it's b, c]` is three items, and text after a
  quoted item's closing quote is an error. (Before 0.4.0, plied opened a
  quoted span at any quote, and read that list as `a` and `it's b, c`.)
- Scalars: plain (to the end of the line, colons included; there are no inline
  comments), double-quoted (JSON string syntax), single-quoted (`''` is a
  quote).
- Block scalars: literal `|`, `|-` and folded `>`, `>-`. Inside a block, a line
  starting with `#` is text; a `#` line indented no deeper than the key ends
  the block.
- `{}` is an empty mapping; other flow mappings are errors.
- Blank lines and lines whose first non-space character is `#` are ignored
  outside block scalars.

Only the integer fields (`next`, `cites[].page`) are read as numbers: a value
made only of the digits 0–9, possibly surrounded by spaces, is that integer.

Frontmatter of linked sources that fails this grammar is read leniently: only
top-level `key: rest-of-line` lines, ignoring indented lines and lines starting
with `#` or `-`.

### 3.2 Canonical writing

An implementation writing a record MUST produce frontmatter that both this
subset and standard YAML 1.1 and 1.2 parsers read as the same strings. It
SHOULD write the canonical form, so that the same content always gives the same
bytes:

- Fields in the kind's order (§3.3), then any others in their existing order.
  Fields whose value is empty (null, `""`, `[]`, `{}`) are omitted.
- A string is written plain unless it is empty, has leading or trailing
  whitespace or a line break, starts with one of ``- ? : , [ ] { } # & * ! | > ' " % @ ` ``,
  contains `: ` or ` #`, ends with `:`, is one of `true false yes no on off
  null ~ y n` in any case, looks like a number (and contains a digit), looks
  like a date or a sexagesimal number (`^\d{4}-\d\d?-\d\d?`,
  `^[-+]?\d+(:[0-5]?\d)+(\.\d*)?$`), contains a non-printable character, or is
  a flow list item containing `, [ ] { }`. Otherwise it is written as a JSON
  double-quoted string (non-ASCII characters left as they are).
- Integers are written plain.
- A list of scalars is written as a flow list when the whole line fits in 100
  characters and no item is a foldable string; otherwise as a block list,
  indented two spaces deeper than its key.
- A string longer than 100 characters with no line break, no double space and
  no surrounding whitespace, whose line would exceed 100 characters, is written
  folded (`>-`), wrapped at words to `max(40, 100 - indent)` characters. As a
  list item, such a string is double-quoted instead.
- Mappings are written as block mappings indented two spaces.
- The frontmatter ends with a newline before the closing `---`.

### 3.3 Fields

Fields not listed for a kind are errors, and so are unknown subfields of
`approval`, `outcome`, `cites` items and `reviews` items. Every scalar is text
unless its type says otherwise.

- **Empty values.** A field or subfield whose value is empty (null, `""`,
  `[]`, `{}`), at any depth of a mapping, is read as absent. Canonical
  writing (§3.2) never writes one.
- **Lists.** A list field given a single value is read as a one-item list.
- **Required.** "Required" means the record is an error without the field.
  Commands that create records supply `status`/`state` (`open`), but a file
  without one is still an error.
- **Prose.** *Prose* fields are shown and searched. No check or command
  gives them a meaning: in particular, no date is read from `revisit`, and
  `trigger` is never evaluated.
- **Dates.** `due` and `revisit_on` are calendar dates, `YYYY-MM-DD`
  exactly. Anything else is an error. They are advisory: `check` warns, and
  `frontier` lists them, but they never block a write or a gate, and they
  never change a state. `due` asks for attention once its day has passed on
  an unresolved record. `revisit_on` asks for attention from its day on, on a
  `parked` or `held` thread. Days are UTC.

*Ref* is a reference (§3.4) and *refs* a list of them.

**Thread** (canonical order as listed)

| field | type | required | meaning |
|---|---|---|---|
| `id` | id | yes | |
| `title` | text | yes | |
| `parent` | ref to a thread | no | organisational only: it neither blocks nor passes on obligations (§12) |
| `status` | `open` `waiting` `held` `parked` `closed` `group` | yes | below |
| `next` | positive integer | no | position in `actor`'s queue; open nodes only; need not be unique |
| `actor` | text, one of `checks.actors` when that is set | no | whose queue the node is in; not an exclusive assignment |
| `due` | date | no | advisory deadline |
| `waiting_on` | refs | when `waiting` | blocks: an `open` node may not have an unresolved one |
| `uses` | refs | no | does not block; its commitments apply (§12) |
| `approval` | mapping: `state` (`approved` or `not approved`), `by` (ref, required when `approved`) | no | §4. It names the decision, not a revision of this node: later edits keep it |
| `outcome` | mapping: `quote` and `ref`, both required | when `closed`, and only then | what concluded the node; `quote` matches `ref` (§3.6). It records what happened, which need not be success |
| `carries` | refs | no; only when `closed` | problems the node leaves unresolved. Nothing waits on them unless a node names them in `waiting_on` |
| `closes_when` | text | no | prose |
| `revisit` | text | no | prose |
| `revisit_on` | date | no | when a parked or held thread should be looked at again |
| `backlog` | ref to a linked record | no | |
| `links` | refs | no | linking a commitment attaches it (§12) |
| `cites` | citations | no | §10 |
| `scanned` | list of mappings | no | scan reviews: linked records read for commitments and found to hold none to record (§8). Added in 0.2.0 |

`status` is one of `open` (nothing named stops it), `waiting` (on
`waiting_on`), `held` (by a decision), `parked` (set aside), `closed`
(concluded, outcome recorded) or `group` (organisational: no `waiting_on`,
no `next`).

**Commitment**

| field | type | required | meaning |
|---|---|---|---|
| `id`, `title` | id, text | yes | |
| `wording` | text | yes | the promise, quoted from `made` (§3.6) |
| `summary` | text | no | prose |
| `made` | ref | yes | the record that made the promise |
| `trigger` | text | no | prose |
| `state` | `open` `due` `fulfilled` `waived` `missed` `not-triggered` `superseded` | yes | the last five are resolved. The state `due` is set by hand; the `due` date never sets it. `superseded` was added in 0.7.0 |
| `due` | date | no | advisory deadline |
| `applies` | `linked` `subtree` `project` | no, default `linked` | §12 |
| `governs` | list of action kinds, each one of `checks.action_kinds` | no; an error without `checks.action_kinds` | the kinds of action the commitment governs; absent, every kind. The gate leaves it out for other kinds (§12). Added in 0.7.0 |
| `nodes` | refs to threads or questions | no; a warning when absent unless `applies: project` | attachment (§12) |
| `resolved_by` | ref | when resolved | for `waived` and `superseded`, checked for authority over the commitment (§4) |
| `superseded_by` | ref to a commitment, not this one | when `superseded`, and only then | the commitment that now holds the obligation. Added in 0.7.0 |
| `links`, `cites` | refs, citations | no | |
| `reviews` | list of mappings | no | one per disposition, appended in order (§12) |

A `reviews` item has `action` (text, required: compared exactly),
`disposition` (`satisfied` `acting` `not-triggered` `not-applicable` `blocks`,
required), `at` (text: `plied dispose` writes `YYYY-MM-DD HH:MM UTC`), `by`
(text: the agent as claimed), `because` (prose) and `ref` (text: a reference
by convention, not checked).

A `scanned` item has `record` (ref to a linked record, required), `rev` (that
record's revision as read, at least 8 lowercase hex digits, required; `plied
scanned` writes all 64), `at`, `by` and `because`, as for `reviews`. Items
are only ever appended. A scan review is not a reference in the sense of
§12: it attaches no commitment.

**Question**

| field | type | required | meaning |
|---|---|---|---|
| `id`, `title` | id, text | yes | |
| `options` | list of text | no | prose |
| `raised` | ref | yes | the record that raised it |
| `state` | `open` `answered` `withdrawn` | yes | the last two are resolved |
| `due` | date | no | when an answer is needed; advisory |
| `nodes` | refs to threads or questions | no | what the question concerns; shown in the view |
| `answered_by` | ref | when `answered` | checked for authority over the question (§4) |
| `answer` | text | no | prose |
| `links`, `cites` | refs, citations | no | |

**Citation** (`cites` items)

| field | type | required | meaning |
|---|---|---|---|
| `paper` | text: a manifest id | yes (the clearance check fails without it) | |
| `page` | positive integer: the PDF page, from 1 | yes (the page check fails without it) | |
| `quote` | text | no; a warning unless `kind` is set | matched against the page (§3.6, §10) |
| `kind` | `table` or `figure` | no | the value was read from one |
| `image_checked` | `true` or `false` | no | only `true` says a `table` or `figure` value was read from the page image |
| `note` | text | no | prose |

A commitment is `superseded` when a decision moved its obligation into
another commitment, which may differ in timing or wording. `waived` releases
an obligation; `superseded` says it still stands, in `superseded_by`. When a
commitment's clauses end differently (one kept, one replaced), `superseded`
is right as long as any part of the obligation lives on in the replacement.

A record is *resolved* when it is a thread with status `closed`, a commitment
in `fulfilled`, `waived`, `missed`, `not-triggered` or `superseded`, a
question in `answered` or `withdrawn`, or any linked record.

### 3.4 References

A reference is an id, optionally followed by a locator: `D134 §2`. The id is
the text before the first whitespace, `,` or `;`, with trailing `.`, `:` and
`)` removed. An id may name any plied record or any linked record.

A locator is for readers only. It does not narrow anything: a quotation
(§3.6) is matched against the referenced record's whole file text,
frontmatter included, whatever the locator says.

### 3.5 History

Each change appends one line to the body's `## History` section (created at the
end of the body when missing; new lines go after the section's last non-blank
line):

```
- YYYY-MM-DD HH:MM UTC <agent>[ (session <first 8 characters>)]: <what>[ — <why>]
```

The time is the minute of the change according to the writing machine's
clock. A node's *last update* is its latest history line.

Record dates are claims by the writer, not evidence that the record existed
before an external event. History times and a review's `at` use the writing
machine's clock; linked dates are supplied by their writers, and git commit
dates can be set by the committer. Establishing precedence needs an anchor
the writer cannot create or rewrite alone, such as an independently held
checkpoint. The proposed receipts and attestations (see [SCOPE.md](SCOPE.md))
would need such an anchor too; they are not implemented timestamp guarantees.

### 3.6 Quotations

`outcome.quote`, a commitment's `wording` and a citation's `quote` match their
source when every part of the quotation appears in the source, in order, after
normalising both:

1. Unicode NFKC;
2. `‘ ’` become `'`, `“ ”` become `"`, no-break space becomes space;
3. `*` and `` ` `` are removed;
4. a hyphen at a line end between two word characters is removed with the line
   break and the next line's indentation;
5. runs of whitespace become one space; leading and trailing whitespace is
   removed.

Parts are separated by an ellipsis (`…` or `...`) with any surrounding
whitespace. A quotation with no parts never matches. Matching is
case-sensitive.

## 4. Linked sources

Each `[[sources]]` entry makes the files its `glob` selects into linked records
of kind `name`. A file whose id field is empty is not a record. An id not
matching `id` is skipped with a warning. Linked records are never written.
Their dates come from `date` (`YYYY-MM-DD`) and `time` (`HH:MM`, default
`00:00`), in UTC.

**Authority.** These references must point to a decision with authority over
what they decide: `approval.by` (over the thread), `answered_by` (over the
question) and the `resolved_by` of a waived or superseded commitment (over
the commitment). The
decision is a linked record. Its authority field reads `<who>`,
`<who> by <ref>`, `<who>: <ref>` or `<who> <ref>`, where `<who>` is a word
(letters, digits, `_`, `-`). Its scope field is a list of ids, or text with ids
separated by whitespace, commas and brackets. The outcome, checked in this
order:

| the referenced record | outcome |
|---|---|
| does not exist | error: a broken reference (`ref`, §8) |
| is a plied record | error |
| comes from a source that declares no authority field | **warning**: cannot be verified |
| has an empty or absent authority field | **warning**: cannot be verified |
| has a `<who>` not in `checks.approve_authorities` | error |
| has `delegated` naming no record, or one that does not exist | error |
| has `delegated by <ref>`, and `<ref>` exists | the delegation is accepted. `<ref>`'s own authority is not examined, and chains of delegation are not followed |
| comes from a source that declares no scope field | warning here, and a configuration error once (§8), so `check` fails |
| has an empty scope | error |
| has a scope without the thread, question or commitment | error |
| anything else | accepted |

A warning does not stop the approval, answer or waiver from counting. A
passing check therefore means only this: a linked record says it has one of
the allowed authorities, and names the node in its scope. It does not
establish who wrote that record, or that the authority it claims was really
given.

## 5. Message logs

`<root>/messages/<node>.jsonl` holds one JSON object per line, each an event.
Key order and whitespace are not significant. Logs are append-only: a writer
appends under the store lock (§6) and MUST first cut a trailing partial line
(a torn write), and MUST NOT append an event whose `id` already appears in the
log. Event ids are unique across all logs; writers generate a ULID unless the
caller supplies one.

Every event has `v` (the number 1), `id` (text) and `type`, and these fields,
all required. *Time* is UTC, RFC 3339 with seconds and `Z`.

| type | fields |
|---|---|
| `message` | `node` (text), `at` (time), `from` (text), `session` (text or null), `to` (list of text: sorted agents), `present` (list of `{agent, session}` with a live claim on the node when sent), `reply_to` (message id or null), `text` (text) |
| `delivered`, `read`, `ack` | `msg` (text: a message id), `agent` (text), `session` (text or null), `at` (time); `delivered` may carry `via` (text) |
| `subscribe`, `unsubscribe` | `node` (text), `agent` (text), `at` (time) |

**Reading.** Readers MUST skip lines that are not events: invalid JSON, not
an object, no `id`, an unknown `type`, or `v` other than 1. `plied check`
reports each as an error, as it does an event with a missing or mistyped
field. Unknown fields are ignored, and kept, since logs are never rewritten.

**Recipients.** A message's recipients are fixed when it is sent: the agents
named with `--to`, the agents with a live claim on the node, and the node's
subscribers, without the sender. Claiming a node later does not deliver
earlier messages. A successor reads them with `plied messages`. A
`delivered` event is per session; a recipient's `read` and `ack` are per
agent. An acknowledgement records receipt, never approval or completion.

**Ids and retries.** Event ids are unique across all logs plied may read
(§9). Writers generate a ULID unless the caller supplies an id. When a
caller-supplied id is already recorded, the send is a retry if the recorded
event is a message with the same `node`, `from`, `text` and `reply_to`, and
every explicitly named recipient is in its `to`. A retry writes nothing and
returns the original event, with exit 0. Otherwise the send is refused, with
exit 3. `at`, `present`, and the recipients derived from claims and
subscriptions are not compared, since they legitimately differ between
attempts. A receipt (`delivered`, `read`, `ack`) that already exists for the
same message and agent (and session, for `delivered`) is not written again.

**Order.** A log's order is the order of appending. There is no order across
logs. `at` is the writing machine's clock: it is shown, and used to sort an
inbox, but nothing is decided by it. A receipt names a message that appears
earlier in the same log; anything else is an error.

## 6. Runtime directory

| path | contents |
|---|---|
| `lock` | the store lock: an exclusive whole-file lock on this file, as `flock(2)` with `LOCK_EX` takes it on Unix and `LockFileEx` with `LOCKFILE_EXCLUSIVE_LOCK` on Windows. It is held for every write to managed paths and to `claims.json`, from recovery (§11) to the last rename. Implementations sharing a runtime directory MUST take it this way. Whether it works across machines depends on the filesystem (often not on network filesystems): plied does not support a runtime directory shared between machines |
| `claims.json` | `{"claims": [{node, agent, session, since, renewed, ttl_min}]}`; times as in §5. A claim is live until `renewed + ttl_min` minutes. Entries not of this shape are ignored and dropped at the next write; claims expired for a day are dropped |
| `revs/<sha256>` | revisions plied read or wrote, by content, for conflict diffs. A revision is kept for `runtime.revision_days` after it was last read or written (its file time), then removed: at most once a day after a write, or by `plied gc` (`--all` removes every one). Without its revision, a conflict is still refused, only without a diff |
| `pruned` | its file time is when the cache was last pruned |
| `written.txt` | revisions plied wrote, as hashes, one per line; the commit guard accepts only these. Pruning never touches it |
| `intent.json` | present only while a write is in progress or was interrupted: `{"version": 1, "state": "prepare" \| "commit", "files": [{path, tmp, prev, sha}]}`. Paths are absolute (§11) |
| `sessions/<session>.json` | private: hook state |
| `index.json` | private: search index, refused when built under another access policy |
| `hashcache.json`, `papers/`, `paper-img/` | private: literature caches |

## 7. The view

`<root>/README.md` is generated from the records after every write. Its content
is not part of this specification, except that it MUST be a deterministic
function of plied's own records, so `plied view --check` can tell whether it is
current. It MUST NOT depend on linked records (§4), which change without a
plied write: a linked file edited by hand, or by the system it comes from,
never makes the view out of date. It may name a linked record's id, but not
show its state or other content. (Before 0.4.0, the view showed a `backlog`
record's state.)

## 8. Checks

`plied check` reports diagnostics. Errors fail it, and a write may not add
one. Warnings change nothing.

**Identity.** A diagnostic has a level, a code, a location, a subject and a
message. Its **identity** is (level, code, location, subject). The message is
free text and is never part of the identity. A location is one of:

- a path relative to the canonical checkout, with `/` separators;
- `plied.toml`;
- a linked record's id;
- a message log line, `<root>/messages/<node>.jsonl:<line>`;
- a cycle, `A → B → A`, written starting from its smallest id, so the same
  cycle always has the same location.

With `--json`, a diagnostic is an object with `level`, `code`, `where` (the
location), `subject` (omitted when empty) and `message`.

**Writes** (§11). A write validates the whole store twice, as it is and as it
would be: records, linked sources and citations, but not message logs and
not the view. It is refused (exit 4) if the result has an error whose
identity the current store lacks. Errors that already exist, on the edited
record or elsewhere, neither block the write nor have to be fixed by it.
Warnings never block. Because identities are fine-grained, resolving one of
several unresolved dependencies changes nothing about the others, while
adding a dependency, or switching an approval to a different invalid
decision, is a new error.

**Errors.** An implementation MUST report at least these:

| code | location | subject | reported when |
|---|---|---|---|
| `version` | `plied.toml` | `format` or `requires` | an unknown format, or a version `requires` excludes (*Versions*) |
| `config` | `plied.toml` | the source's name | a source declares an authority field but no scope field |
| `parse` | the file | — | the frontmatter breaks §3.1 |
| `id` | the file | — | an id is missing or malformed, or the file name does not start with it |
| `duplicate` | the file | — | another file uses the same id |
| `field` | the record | the field: `next`, `actor`, `due`, `revisit_on`, `applies`, `approval.state`, `cites.kind`, `reviews[<i>].action`, `scanned[<i>].record`, `scanned[<i>].rev`, `governs` (without `checks.action_kinds`, or an item that is not text), `governs:<kind>` (one per kind not in `checks.action_kinds`), an unknown field's name, … | an unknown field or subfield; a value of the wrong type or outside its allowed values (§3.3) |
| `required` | the record | the field: `title`, `outcome`, `outcome.quote`, `outcome.source`, `wording.quote`, `wording.source`, `approval.by`, `resolved_by`, `superseded_by`, `raised`, `answered_by` | a required field is missing |
| `ref` | the record | `<field>:<id>` | a broken reference (saying when it may be sealed); a reference to the wrong kind (`parent` not a thread, `nodes` not a thread or question, `superseded_by` not a commitment, `backlog` or `scanned` a plied record); a node depending on itself; a commitment superseded by itself |
| `state` | the record | `status` or `state` (invalid value), `next` (on a node that is not open), `waiting_on` (`waiting` without it), `waiting_on:<id>` (one per unresolved dependency of an `open` node), `group`, `outcome`, `carries`, `superseded_by` (set on a commitment that is not `superseded`) | a state that contradicts the other fields |
| `quote` | the record | `outcome` or `wording` | a quotation does not match (§3.6) |
| `authority` | the record | `<approval, answer, waiver or supersession>:<decision id>` | §4 |
| `cycle` | the cycle | `waiting_on`, `parent` or `superseded_by` | a cycle |
| `literature` | the record | `cites` | `cites` without `[literature]` |
| `lit-<aspect>` | the record | `<paper> p<page>` | a citation check fails (§10) |
| `messages` | the log or line | — | a malformed or unsupported event, a repeated id, a message filed under the wrong node, a receipt for an unknown message, a torn last line |
| `intent` | the intent file | — | a write is pending or was interrupted (§11). Reported by `check` only, never compared by writes |

A review whose disposition is not allowed has the `field` code with subject
`reviews[<i>].disposition`. Indexes are stable, because reviews are only
ever appended.

**Warnings:** a source selecting no files allowed by the access policy (code
`source-empty`, location `plied.toml`, subject the source's `name`); a linked
id not matching its pattern; an approval whose authority
cannot be verified; a waiting node whose `waiting_on` is all resolved; a
commitment attached to nothing; a body mentioning ids that do not exist; a
*stale* node (below); a scanned
source record with a commitment phrase that no commitment links
(code `commitment?`; `scan_commitments`, from `commitments_since`) and that
no thread's `scanned` covers at its current revision; an unlinked record of a
`require_linked` source; a stale view; citation warnings (§10); a date that
asks for attention (§3.3, code `date`); a review whose action has a kind not
in `checks.action_kinds` (code `action`, a warning so that adopting a
vocabulary never blocks records made before it); a superseded commitment
whose replacement, followed through `superseded_by` to the first commitment
that is not superseded, is unresolved and does not apply (§12) to every node
the superseded one applies to (code `superseded`, added in 0.7.0).

`source-empty` counts files after access filtering (§9), before parsing or
checking ids. Directories do not count; an idless template or a file with an
invalid id does. A source whose matches are all sealed or resolve outside the
repository gets the same warning as one with no matches, without listing
those paths. This warning never fails `check` or blocks a write and is omitted
by `--errors-only`. It does not check literature manifests.

**Stale.** A plied record is stale when a dated linked record (§4), dated
strictly after the record's last update (§3.5), mentions the record's id or,
while the record is unresolved (§3.3), an id in one of its reference fields
other than `parent`. To *mention* is to
contain the id anywhere in the linked record's file text, frontmatter
included, with no letter, digit or `_` directly before or after it. A linked
record that the record itself references never makes it stale, and a record
with no history line is never stale. `parent` is left out because it is
organisational (§3.3): a record about a group makes the group stale, not
each of its children. (Before 0.4.0, `parent` counted.) A resolved record's
references are history: a record about something a closed thread or a
fulfilled commitment once linked cannot change what it concluded, while one
that names the resolved record itself may be reopening it. (Before 0.5.0, a
resolved record went stale through its references too.)

A node has at most two stale warnings, told apart by subject: `named` lists
the newer records that mention the node's own id, and `links` those that
mention only ids in its reference fields. All `named` warnings are reported
before any `links` warning. A record that names the node is more likely to
change it, while one that cites only its links usually needs just a note.
(Before 0.7.0, a node had one stale warning, with no subject.)

## 9. Access

A path is *sealed* when its text (as given, with `/` separators, or resolved
through symlinks) contains any sealed substring. Sealed paths, and paths that
resolve outside the canonical checkout, are never read. A `sealed_file` that
is configured but missing makes every read fail.

Sealing controls what plied reads and shows. It is not secrecy: a sealed file
is still plaintext in the clone and in git history, readable by any other
program. What it covers:

| what | when its path is sealed |
|---|---|
| records and linked sources | not loaded: absent from every command, the view and `check`. A reference to one is a broken reference whose message says it may be sealed |
| message logs | not read: their events are absent from `inbox`, `messages`, hook delivery and `check`, and id uniqueness (§5) cannot see them. Appending to one is refused |
| literature page text | treated as absent, so page and quote results are `unchecked`, not passed (§10) |
| the search index | sealed paths are not indexed. The index stores a digest of the access policy (sealed substrings, sealed file, manifest, allowlists, literature settings) and is refused when the digest differs |
| the view, diagnostics, command output | built only from what was loaded, so they quote nothing sealed |
| the revision cache (§6) | content cached before a path was sealed stays there; sealing removes nothing. Cached revisions are read only by hash, to show a conflict diff for a readable record |
| directory listings | names and sizes of files are read (to notice that logs changed); their content is not |

`plied.toml`, the `sealed_file` and the literature manifest are always read,
since they define the policy.

## 10. Literature

The manifest is JSON: either a list of entries or an object holding the list
under `list`. Each entry names a file and an id (§2) and may give a SHA-256.
Only listed papers are cleared. Page text for paper `id`, page `n` is the file
`page_file` (with `{page}` = n) in `text_dir` (with `{id}`), or in the runtime
cache.

Each citation gets exactly five results, always all of them and in this
order. Each result's status is `ok`, `warn`, `fail` or `unchecked`; `fail` is
an error and `warn` a warning (§8, code `lit-<aspect>`).

| aspect | ok | warn | fail | unchecked |
|---|---|---|---|---|
| `clearance` | the paper is in the manifest | | it is not | |
| `hash` | the PDF's SHA-256 matches the manifest | | it does not, or the PDF exists but cannot be read (sealed, outside the checkout, an I/O error) | no hash given, no `pdf_dir`, or the PDF is absent from this checkout |
| `page` | the page has text | | `page` is not a page number, or the paper has no such page | the paper has no page text at all |
| `quote` | the quotation matches the page (§3.6) | a match or mismatch on OCR text; no quotation, unless the value is from a table or figure | the quotation is not on the page, or its text cannot be read | no quotation for a table or figure value |
| `image` | a `table` or `figure` value says `image_checked: true` | a `table` or `figure` value without it | | not a table or figure value |

A result that cannot be reached because an earlier one did not pass is
`unchecked`, and its message says why. If the paper is not cleared, hash,
page and quote are unchecked. If the page check fails or has nothing to check
against, quote is unchecked. The image result depends only on the citation.
`image: ok` records a claim the citation makes; plied does not look at the
image.

## 11. Command line

**Exit codes:** 0 ok · 1 error or failed check · 2 usage · 3 conflict (the
record moved on since `--expect`) · 4 the change would add validation errors ·
5 blocked (branch, merge state or version pin; guard refusal) · 6 review gate.

**Environment:** `PLIED_AGENT` (who acts, unless `--as`), `PLIED_SESSION` or
`CLAUDE_CODE_SESSION_ID` (session, unless `--session`), `PLIED_RUNTIME`,
`PLIED_CANONICAL`.

**Streams.** Without `--json`, a command's result goes to stdout. Errors,
gate reasons, guard refusals and notes go to stderr. That text is for people,
and its wording is not part of this contract. With `--json` (§11.1), stdout
holds exactly one JSON document and stderr stays empty. `plied hook` is the
exception: it speaks its agent's hook protocol.

The exit code reports what the command did, never whether its output was
read. When stdout is a pipe whose reader has gone (`plied list | head -1`),
plied stops writing to it and exits as it would have otherwise, so an accepted
write still exits 0. Any other failure to write stdout is said on stderr, and
also leaves the exit code alone.

**Assignments** (`plied new … key=value`, `plied set`). The operator is at an
assignment's first `=`. `key=value` sets the field, and `key=` removes it.
`key+=item` adds items that a list does not already have. `key-=item` removes
items. Dotted keys address subfields (`approval.state=approved`), and
removing a mapping's last subfield removes the mapping. A value is read as a
§3.1 flow list or quoted scalar when it starts with `[`, `"` or `'`, as an
integer when it is all digits, and as text otherwise. List fields (§3.3) given
a single value hold a one-item list. `id` can never be assigned. Each write
appends one history line (§3.5) that says what changed, followed by `--why`.

**Other writes.** `plied note` adds only a history line. `plied replace`
takes a whole file. It must keep the record's id, and it gets a history line
like any write. `plied dispose` appends one `reviews` item (§3.3) and never
edits an earlier one. `plied scanned R --rev REV --on T --because …`
appends one `scanned` item to thread `T`, after checking that `R` is a linked
record of a source with `scan_commitments` and that its revision still starts
with `REV`. Otherwise it is a conflict (exit 3, with `detail` naming `R`), so
a review always covers what was read.

**Side effects of reading.** `plied inbox` records `read` for the messages it
shows, unless `--peek` is given. `plied show` adds the revision it shows to
the revision cache (§6), so a later conflict can show a diff from it.

**Writes** happen under the lock and are refused, in this order, when the
canonical checkout is mid-merge, mid-rebase, mid-cherry-pick, mid-revert or
mid-bisect, has a detached HEAD or is on a branch other than
`canonical_branch`; when the version pin differs; when the record's revision is
not the expected one; when the change would add an error.

An accepted write replaces the changed records and the regenerated view as
one **recoverable multi-file write**. It is not atomic to readers: each file
becomes visible at its own rename. The steps:

1. Write `intent.json` in state `prepare`. It lists each target, its temporary
   file in the same directory, and the target's revision before (`prev`, null
   when absent) and after (`sha`).
2. Write and fsync every temporary file. Append every `sha` to `written.txt`
   and fsync it. Fsync the directories holding the temporary files.
3. Rewrite the intent in state `commit`.
4. Rename every temporary file over its target, fsync the targets'
   directories, then remove the intent.

A revision is therefore durably in `written.txt` before it appears on disk.
A directory fsync that the filesystem doesn't support is accepted. Any other
I/O failure is an error. A target that exists but can't be read is an error,
never treated as absent.

**Recovery.** A record write, `plied view` and `plied recover` MUST recover a
leftover intent under the lock before doing anything else. They do this
after the branch, merge-state and version-pin refusals, which therefore also
hold back recovery.

- A `prepare` intent is undone. Its temporary files are deleted and no target
  is touched.
- A `commit` intent is finished. Each target that holds `prev` is replaced by
  its temporary file, provided that file holds `sha`. A target that already
  holds `sha` is left alone.
- Writes are refused (exit 5), and the intent is left for a person to inspect
  and remove, if any of these hold: a target holds neither revision; a target
  or temporary file is missing, unreadable or damaged; the intent itself
  exists but can't be read or parsed.

Only a missing `intent.json` means nothing is pending.

**Outcomes.** A command that recovers an earlier write first reports it on
stderr ("before this command: …"), separately from its own outcome. The
command can then still be refused, for example as a conflict, with exit 3.
So a nonzero exit can follow a change made by recovery, but never a change
made by the refused command itself. There is one exception: a failure in
step 4 (a rename, a directory fsync, or removing the intent; the message says
so) leaves a `commit` intent, and recovery finishes it. `plied show` gives
the current revision in every case.

**While an intent is pending** (a write in progress, or one interrupted):

- `plied check` reports it as an error.
- The pre-commit and pre-merge-commit guards refuse. Some managed files may
  hold new revisions and others old ones, and the new ones are already in
  `written.txt`.
- Commands that read records take no lock, so they may see some files
  replaced and others not. They say so on stderr when they see an intent
  before or after reading. Reads are not snapshot-isolated.

**Message logs (§5) are outside this scheme.** They are appended under the
same lock, and the log's resulting revision is in `written.txt` before the
append. An intent never lists a message log, so appends neither recover nor
wait for a pending intent. Appends are also not subject to the branch,
merge-state or version-pin refusals, and they don't regenerate the view.

### 11.1 JSON results

`--json` may be given to any command except `hook`, before or after the
command name. stdout then holds one object, whether the command succeeded or
not:

```json
{"plied": 1, "ok": true, "exit": 0, "status": "ok", "result": {...}, "notes": []}
{"plied": 1, "ok": false, "exit": 3, "status": "conflict",
 "error": {"kind": "general", "message": "...", "detail": {...}}, "notes": []}
```

- `plied` is the envelope's version. A consumer MUST refuse a version it
  does not know.
- `exit` is the process's exit code. `ok` is true exactly when it is 0.
  `status` names its class: `ok`, `error` (1), `usage` (2), `conflict` (3),
  `invalid` (4), `blocked` (5) or `gate` (6).
- `result` is present when the command ran to its verdict. That includes a
  failing `check` (exit 1), a closed gate (exit 6), `view --check` on a stale
  view and a failing `lit check`. Its shape is the command's, below.
- `error` is present instead when the command could not run to a verdict.
  `kind` is `general`, `literature`, `policy` or `query`. `message` is free
  text. `detail` is present for a conflict
  (`{"id", "expected", "current"}`, with `current` null when the record is
  absent) and for a write that would add errors (`{"diagnostics": [...]}`,
  diagnostics as in §8).
- `notes` lists things worth knowing that are not the result, as text, for
  example that recovery finished an earlier write (§11) or that a read may
  mix old and new files.
- A usage error that the argument parser finds also gets an envelope (exit
  2), provided `--json` is among the arguments.

Records are objects holding their frontmatter fields plus `id`, `kind`,
`title`, `state`, `path`, `rev`, `body`, `text` and `own`. The results, by
command:

| command | `result` |
|---|---|
| `init` | `{path}` |
| `skill` | `{version, skill}`: the agent skill as Markdown text, rendered from this binary's version and command inventory. Needs no repository. Without `--json` the skill itself is the stdout text. |
| `show` | a record, plus `backlinks: [{id, how: [field or "mentions", …]}]`; a plied thread or question also has `commitments: [{id, rev, state, title, chains, governs}]`, the commitments that apply to it, with their chains as in `review` (§12; added in 0.6.0; `governs` in 0.7.0, null when absent) |
| `new`, `set`, `note`, `replace`, `dispose`, `scanned` | `{id, rev}`: the record written |
| `list` | `[{id, kind, title, state, rev, path}]` |
| `find` | records without `body` and `text` |
| `backlinks` | `[{id, how}]`, as in `show` |
| `tree` | `[{id, title, status, rev, children: [...]}]`; a thread met a second time in a `parent` cycle is `{id, cycle: true}` |
| `frontier` | `{queues: {actor: [{id, next, title, rev}]}, unscheduled: [{id, actor, title, rev}], open_questions: [{id, title, rev}], dates: [{id, field, date, passed}]}`; `unscheduled` lists the open threads without `next`, so in no queue, in id order (`actor` null when unset; with `--actor`, only that actor's; added in 0.4.0); `dates` lists every date field on an unresolved record, earliest first, and `passed` says whether it asks for attention today |
| `view` | `{path}`; with `--print` `{path, text}`; with `--check` `{path, current}`, exit 1 when not current |
| `recover` | `{recovered: "finished", "undone" or null, files}` |
| `gc` | `{removed}` |
| `check` | `{diagnostics, errors, warnings, records, linked, config}` |
| `claim` | a claim (§6), plus `commitments` as in `show`, empty unless the node is a plied thread or question (added in 0.6.0). Without `--json`, `claim` lists them too |
| `release` | `{released}` |
| `who` | claims, each with `expires` |
| `send` | `{event, recorded_now}`; `recorded_now` is false for a retry (§5) |
| `subscribe`, `unsubscribe` | `{event}` |
| `inbox` | `{messages, marked_read}`; each message carries `delivered`, `read`, `acked` (times or null) |
| `ack` | `{acknowledged}` |
| `messages` | the node's events, in log order |
| `review` | `{commitments: [{id, rev, state, wording, chains, reviews, governs, governs_action}], gate}`; `gate` is null without `--gate`, else `{action, passed, reasons}`. `governs` is the commitment's list, or null; `governs_action` is false when the gate leaves the commitment out for `--action` (§12), and true without `--action` (both added in 0.7.0) |
| `index` | `{records, pages, policy}` |
| `search`, `lit search` | `[{key, kind, path, score, snippet, stale, ocr}]` |
| `lit list` | `[{id, file, pages}]` |
| `lit page` | `{paper, page, text, ocr}`; with `--image` `{paper, page, image}` |
| `lit extract` | `{paper, pages}` |
| `lit cites` | `[{id, how: "cites" or "mentions", pages, title}]` |
| `lit check` | `{citations: [{record, paper, page, results: [{aspect, status, message}]}], manifest, failed}`, exit 1 when `failed` > 0 |
| `guard pre-commit`, `pre-merge-commit`, `range` | `{problems}`, exit 5 when there are any |
| `guard install` | `{installed}` |

Fields may be added to results in a later version with the same envelope
version. Fields are never removed or retyped without a new one.

**`plied find`** takes a JSON object. Each key is a field (dotted for nested
ones) and each value a condition; all must hold. A plain value matches equal
values, a list field containing it, and a number equal to a string of digits.
Operators: `$eq $ne $in $nin $gt $gte $lt $lte $regex` (with `$options: "i"`)
`$exists $contains $size $not`, and at the top level `$and` and `$or` taking
lists. `$gt` and friends compare numbers as numbers and anything else as text.
A malformed query exits 2.

## 12. Commitment review

`plied review N… [--action A] [--gate]` lists the commitments that apply to
the nodes `N…`, each with the chain of links through which it applies.

**Which commitments take part.** Every commitment except a resolved one
(§3.3). A commitment with a missing or invalid `state` is an error (§8), and
still takes part, so a damaged record holds the gate instead of opening it.
An invalid `applies` is an error, and is read as `linked`.

**Attachment.** A commitment is *attached* to a record when the commitment's
`nodes` names it, or when the record's `links` names the commitment.

**Applicability.** A commitment applies to a node `N` when any of these
holds:

1. it is attached to `N`;
2. it is attached to a node that `N` reaches by following `waiting_on` and
   `uses` (both, transitively, through plied records only), or `N` reaches
   the commitment itself that way;
3. it says `applies: subtree` and is attached to an ancestor of `N`
   (following `parent` from `N`);
4. it says `applies: project`.

Nothing else carries an obligation. `parent` alone does not (rule 3 needs
`subtree`). Ancestors of dependencies are not followed. `carries`, `backlog`
and `cites` never make a commitment apply.

**Governed actions.** A commitment with `governs` governs only actions of
the kinds it lists. It still applies, and `review` still lists it, marked as
not governing the action, but the gate leaves it out for any other kind.
A `governs` that `check` rejects (no `checks.action_kinds`, or a kind not in
it) governs every action, so a misspelling holds the gate instead of opening
it. Narrowing `governs` opens gates just as removing a node from `nodes`
does, and like that it needs no authority: the history line records it.

**The gate.** `--gate` needs `--action` (otherwise a usage error, exit 2). It
exits 6 when any applicable commitment that governs `A` has no review for
`A`, or when its latest review for `A` has disposition `blocks`. Otherwise it
exits 0, including when no commitment applies.

- *Latest* is last in list order. `at` is not consulted.
- Action names are compared as exact, case-sensitive text. A review for any
  other action does not count, so a new action name starts without reviews.
- An action is `kind` or `kind:instance`, with the kind before the first
  `:` (`run:fit-20261024` has kind `run`). When `checks.action_kinds` is
  set, `review --action` and `dispose --action` refuse any other kind as a
  usage error (exit 2). The kind is a vocabulary check only: the gate still
  compares the whole name.
- Every disposition except `blocks` passes, `acting` included. A passing gate
  means each applicable commitment was reviewed for this action. It does not
  mean the obligation was fulfilled, or that the evidence a review relied on
  is unchanged since.
- The gate examines only the nodes it is given. It cannot tell whether the
  caller named the right nodes, or whether the action it gates ever runs.

