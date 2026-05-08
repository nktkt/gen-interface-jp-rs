# Contributing to `gen-interface-jp-rs`

`gen-interface-jp-rs` is a Rust port of the Python project [`gen-interface-jp`](https://github.com/yamatoiizuka/gen-interface-jp). The canonical pipeline behavior lives in that upstream repository — when in doubt about *what* the tool should do, the Python source is the source of truth. This crate reimplements that pipeline in Rust.

Thanks for considering a contribution. This document describes the minimum you need to know to land a PR.

## Project layout

The workspace contains three crates under `crates/`. See [`README.md`](./README.md) and [`CLAUDE.md`](./CLAUDE.md) for an overview of what each crate is responsible for and how they fit together.

## Toolchain

This project pins the Rust toolchain via [`rust-toolchain.toml`](./rust-toolchain.toml) (stable). If you have `rustup` installed, the correct toolchain will be selected automatically when you run `cargo` from the workspace root.

## Vendor fonts

Do **not** copy the vendor fonts into this tree. They are expected to live at `../source/vendor/` — i.e. clone the upstream [`gen-interface-jp`](https://github.com/yamatoiizuka/gen-interface-jp) repository as a sibling of this one:

```
parent/
  gen-interface-jp/      # upstream Python repo (source of fonts + reference pipeline)
  gen-interface-jp-rs/   # this repo
```

Keeping the fonts out of the Rust tree avoids duplicating large binary assets and keeps licensing clear.

## Before submitting a PR

Please make sure the following all pass locally. CI runs the same checks.

- **Format**: `cargo fmt --all`
  - CI runs `cargo fmt --all -- --check` and will fail on any diff.
- **Lint**: `cargo clippy --workspace --all-targets -- -D warnings`
  - Warnings are errors. If a lint genuinely needs to be silenced, do it narrowly with `#[allow(...)]` and a comment explaining why.
- **Tests**: `cargo test --workspace`
  - All tests must pass.

### Changes that touch font byte mutation

If your change modifies how font bytes are mutated (glyph table edits, OS/2 patches, name-table rewrites, anything in the byte-mutation path), include in the PR description a mapping back to the corresponding Python source: **file path + line range** in the upstream `gen-interface-jp` repository. Reviewers use this to diff the Rust algorithm against the canonical Python implementation. Without this mapping it is essentially impossible to verify behavioral equivalence, so PRs in this area will not be merged until the mapping is provided.

## License

- The Rust source in this repository is licensed under **MIT**.
- The fonts produced by the pipeline are licensed under **OFL-1.1**.

By submitting a PR you agree that your contribution is licensed under the same terms (MIT for source, OFL-1.1 for any generated font output).
