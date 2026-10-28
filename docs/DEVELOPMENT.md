# Contributing to plied

This repository implements the tool. A project using it has its own
`plied.toml` and work records; do not initialize a store here just to develop
the binary. Tool users should start with the README's first-task guidance
or `plied skill` from their installed binary.

Use Rust 1.89 or newer, Git, and Python 3.8+ for executable examples. Build
and run the checkout's binary explicitly so an older installed plied cannot
be mistaken for the code under development:

```sh
cargo build --locked
./target/debug/plied --help
cargo run --locked -- context --json
cargo test --locked
```

On Windows the executable is `target/debug/plied.exe`. A first contributor
task is to run the [first-task replay](../examples/first-task/README.md)
against this binary, inspect one conformance fixture and its implementation,
then make a small change with the contract, fixture and changelog together.
`docs/FORMAT.md` is authoritative; `docs/SCOPE.md` explains product boundaries.

## Source and test map

| Change | Start here | Verification |
|---|---|---|
| CLI arguments, JSON, exit classes | `src/main.rs`, `src/error.rs`, FORMAT §11 | `tests/cli.rs`, relevant conformance fixtures |
| Record fields, states, references | `src/records.rs`, `src/schema.rs`, `src/check.rs`, `src/writer.rs` | `tests/conformance.rs`; schema example fixtures |
| Conflicts, recovery, canonical checkout | `src/writer.rs`, `src/fsx.rs`, `src/repo.rs` | fsx unit tests; CLI/recovery/worktree fixtures |
| Commitment applicability and gates | `src/review.rs`, `src/check.rs`, `src/main.rs` | §12 conformance fixtures, malformed dispositions and empty gates |
| Access, search, literature | `src/access.rs`, `src/records.rs`, `src/index.rs`, `src/literature.rs` | policy/symlink/cache fixtures; `tests/pdf_cache.rs` with pdfium |
| Presence, messages, hooks | `src/messages.rs`, `src/hook.rs` | message/claim/session integration fixtures |
| Agent guidance and discovery | `src/skill.rs`, `src/schema.rs`, CLI inventory in `src/main.rs` | skill-output checks, discovery fixtures, executable first-task replay |

The command path is CLI → repository/configuration and access policy → store
loading → validation → writer under lock → recoverable filesystem journal →
regenerated view. Reads do not take the write lock. Message appends and
runtime claims have their own rules; see FORMAT §§5–6 and §11 before
combining them with record writes.

## Checks to run

```sh
cargo test --locked
cargo +1.89.0 test --locked
cargo +stable clippy --locked --all-targets -- -D warnings
cargo +stable clippy --locked --all-targets --features pdf -- -D warnings
python3 examples/first-task/replay.py --plied "$PWD/target/debug/plied"
python3 examples/research/replay.py --plied "$PWD/target/debug/plied"
python3 examples/scope/replay.py --plied "$PWD/target/debug/plied"
cargo test --locked --example related
cargo build --locked --example related
python3 examples/related/test_related.py --plied "$PWD/target/debug/plied" --related "$PWD/target/debug/examples/related"
```

Install the named Rust toolchains with rustup if necessary. Default tests do
not exercise pdfium. With a pdfium shared library on the system search path
or beside the binary, also run:

```sh
cargo +stable test --locked --features pdf --test pdf_cache
```

CI installs pinned pdfium binaries via pypdfium2 in a separate job and runs
that test on a real one-page PDF. The ordinary matrix covers Linux, macOS
and Windows; the MSRV job tests Rust 1.89; lint covers both feature sets;
Linux replays both examples. Regression fixtures cover canonical first use,
relative inputs, session-scoped handoff, expected nonzero JSON verdicts and
the supported page-cache locations.

Run the focused test for your change first, then the checks appropriate to
its scope. A change to files, checks, exit codes or JSON needs a matching
FORMAT update, a meaningful fixture and an Unreleased changelog entry.
