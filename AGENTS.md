# Notes for agents working on plied

`docs/FORMAT.md` is the contract. A change to files, checks, exit codes or
`--json` output updates it in the same change, along with a test (a
conformance fixture in `tests/conformance.rs` when another implementation
could read it two ways) and a line under *Unreleased* in `CHANGELOG.md`.

Things that aren't obvious from the code:

- **Never `println!`/`eprintln!`.** Use `outln!`/`errln!` (`src/out.rs`). A
  closed pipe must not panic, and the exit code reports what the command did,
  never whether its output was read (§11, *Streams*).
- **A diagnostic's identity is (level, code, location, subject), never its
  message.** A write is refused when it would add an error identity the store
  lacks. Pick subjects so that fixing one problem never looks like adding
  another (one `waiting_on:<id>` error per dependency, `reviews[<i>]` indexes
  that stay stable because lists are only appended to).
- **Adding a record field** touches `OwnKind::fields` (canonical order),
  `LIST_FIELDS` for lists, `subfields` in `check.rs` for mapping items, and
  `ref_fields` only if it should attach commitments (§12) and count for
  `stale`. The format stays 1. Older plied reports the field as unknown, so
  FORMAT.md marks the version that added it and the CHANGELOG tells stores to
  set `requires`.
- **CI also runs** the MSRV (`rust-version` in Cargo.toml, 1.89) and clippy
  with `--features pdf`, which the test jobs never build:
  `cargo clippy --all-targets --features pdf -- -D warnings`.
- **Releasing:** bump `version` in Cargo.toml, date the CHANGELOG section,
  merge, then push tag `vX.Y.Z` on main. The tag must equal the Cargo.toml
  version. `.github/workflows/release.yml` publishes the wheels to PyPI and
  the crate to crates.io. Neither can be undone.
