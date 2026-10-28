# Recovering a plied workflow

Start with `plied context --json` to verify the selected checkout, branch,
version compatibility, agent and session. From a linked worktree the record
still lives in the canonical checkout. `-C` also selects the directory for
relative input files. Use the IDs and revisions returned with the text you
read, rather than assumed IDs or a revision fetched immediately before writing.

With `--json`, read stdout even when the process exits nonzero. A command
that reached a verdict returns `result`: a failing check has
`result.diagnostics`, and a closed gate has `result.gate.reasons`.
A command that could not run returns `error`; conflict details are in
`error.detail`, and refused writes put their added diagnostics in
`error.detail.diagnostics`. `notes` may report that recovery completed an
earlier write even though the current command was refused.

| Exit / trigger | Inspect | Next action |
|---|---|---|
| 1: failed check | `plied check --json`, `result.diagnostics` | Read each error's code, location and subject; correct its cause. Warnings are for review. |
| 1: command failure | `error.kind`, `error.message`, `error.detail` | Check the requested record/file/configuration. Do not assume a failed check and a command failure have the same JSON shape. |
| 2: usage | `plied COMMAND --help` | Correct arguments. A gate needs nodes and an action; session-scoped release needs a nonempty session. |
| 3: conflict | `error.detail` and `plied show ID --json` | Read the new text and revision together, reconcile your intended change, then retry with that revision. There is no force overwrite. |
| 4: proposed change adds errors | `error.detail.diagnostics` | Correct the proposal; preserve existing review/scan entries and append corrections. The refused write changed no record. |
| 5: blocked write | `plied context --json`; the reported branch/version blockers | Resolve the canonical checkout's merge/rebase state or branch/version mismatch first. A guard refusal also needs its reported staged/range problems inspected. |
| 6: review gate | `result.gate.reasons`, then `plied review NODE --action ACTION` | Read the applicable commitments. Fulfill or honestly disposition their conditions for this exact action; re-read the commitment before `dispose --expect`. |
| Missing configuration | `plied context --json` | Initialize the selected canonical checkout with `plied init`, inspect the template, then discover context again. An existing malformed config is an error, not an empty project. |
| Pending write intent | `plied check --json`; `<runtime>/intent.json` from `context` | After branch/version blockers are resolved, run `plied recover`. Preserve the intent and target files if recovery refuses; have a maintainer inspect inconsistent bytes rather than deleting the journal. |
| Malformed assignment | `plied set --help`, `plied schema KIND --json` | Use `key=value`, `key+=item`, `key-=item`, or `key=`; quote shell arguments and use `--set-file` for long text. |
| Missing PDF support/library | `plied context --json`, `plied lit extract PAPER --json` | A wheel lacks the PDF feature. A feature-enabled binary also needs a pdfium shared library; see the README installation instructions. |
| Index invalidated by policy/manifest change | `plied search TERMS --json` | Review the new access settings, then rebuild with `plied index`. Do not serve the old index. |

`recover` finishes or undoes only the interrupted write. It does not infer
your intended new change. Read the record again afterward. `show` can add its
revision to the local cache; `context` and `schema` are read-only discovery.

For handoff, first note what is unfinished, its location and the next action
on every claimed node; set scheduling/status to match. Then check and name
any remaining diagnostics. Finally run `plied release --this-session`.
Writing a record note and releasing runtime presence are separate operations;
an expired lease is not reliable handoff history.

The executable [JSON client](../examples/first-task/client.py) uses
`subprocess.run(..., check=False)`, validates the envelope and retains the
exit code. For example:

```sh
python3 examples/first-task/client.py --plied /absolute/path/to/plied -C /project -- review NODE --action run:unique-id --gate
```

Replace `NODE` and the action with your actual task and a fresh action name.
Exit 6 is an expected verdict with JSON, not a missing response. The
[first-task replay](../examples/first-task/README.md) executes conflicts,
invalid writes, closed gates and recovery through re-reading in a disposable
project, and asserts the expected envelopes.
