# First task: fix a parser

This small example runs an installed plied binary in its own temporary Git
repository. It needs Python 3.8+ and Git. From this checkout:

```sh
python3 examples/first-task/replay.py --plied /absolute/path/to/plied
# Or build and use this checkout's binary:
python3 examples/first-task/replay.py --source
```

Without either option it finds `plied` on PATH. The example removes its
temporary repository when done. `--keep /path/to/a-new-directory` retains it
and `transcript.json`; the directory must not exist. It never edits your
project's work records or uses its runtime overrides.

The script prints every tool call and its expected exit. It captures all
created IDs, reads text with its revision, and covers this sequence:

1. Discover project settings and field schemas; establish a unique agent session.
2. Create the task and a quoted, attached commitment. Read the node's prior
   conversation before claiming it; inbox does not retarget old messages.
3. Make a checked update (exit 0), deliberately reuse the old revision
   (conflict, exit 3), re-read and reconcile. Refuse an invalid state (exit 4).
   A synthetic crash journal exercises failed-check JSON (exit 1) and
   recovery; a temporary branch mismatch exercises blocked-write JSON (exit 5).
4. Gate a fresh action (exit 6). An omitted node is usage exit 2. Add the
   promised regression case, read the commitment, and record why its
   precondition is satisfied. The gate then passes (exit 0) before the run.
5. Show that another action remains unreviewed (exit 6). Record results,
   remaining edge cases and file locations, check for zero errors/warnings,
   and release only this session while preserving another session's claim.

The tiny parser assertion demonstrates the governed action. A passing gate
records the review; it does not prove the reviewer checked the case, establish
identity or independence, or pin the revision of evidence the reviewer used.
The journal and branch mismatch are fault injection inside the disposable
fixture, not instructions to edit journals or bypass blockers in a real project.

[client.py](client.py) is a reusable JSON client: it parses stdout on nonzero
exits, distinguishes `result.diagnostics` / `result.gate.reasons` from
`error.detail`, and preserves the process exit code when used as a CLI.
See [troubleshooting](../../docs/TROUBLESHOOTING.md) for recovery decisions,
and the [research example](../research/README.md) for a larger evidence workflow.
