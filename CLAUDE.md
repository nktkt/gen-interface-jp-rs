# CLAUDE.md — gen-interface-jp (Rust port)

Project conventions for the Rust port of `gen-interface-jp`. Read this before
making changes; it captures the intent, layout, and house style so the port
stays coherent with the original Python implementation.

## Project intent

This crate workspace is a **Rust port of the Python `gen-interface-jp` font
build pipeline**. The reference implementation lives next door at `../source/`
and remains the source of truth for behaviour. This port exists to:

- Replace the Python toolchain with a single statically-linked binary set.
- Trade the `fontTools` dependency for a typed, faster `skrifa` + `write-fonts`
  stack.
- Keep the build artefact bit-equivalent (or as close as the toolchain
  difference allows) to what `../source/` produces.

When in doubt about *what* the pipeline should do, consult the Python source.
When in doubt about *how* to structure the Rust, follow this document.

## Layout

```
rust/
├── Cargo.toml              # workspace root
├── rust-toolchain.toml
├── justfile
├── crates/
│   ├── gen-font/           # Stage 1+2: bake variable -> static, proportionalise
│   │   ├── src/
│   │   └── tests/          # integration tests live here, not under src/
│   ├── gen-webfont/        # WOFF2 / subsetting outputs
│   │   ├── src/
│   │   └── tests/
│   └── gen-release/        # release-bundle assembly (zip, manifests, hashes)
│       ├── src/
│       └── tests/
└── docs/
    └── ARCHITECTURE.md
```

Three crates, all under `crates/`. Each crate keeps its **integration tests in
`crates/<name>/tests/`** alongside the crate, never in a top-level `tests/`
directory and never mixed into `src/`. Unit tests stay in their `mod tests`
blocks inside the module they cover.

### What is NOT ported

- **Vendor fonts** (Noto Sans JP, Inter, etc.) are *not* duplicated into this
  tree. They are read from `../source/vendor/` at build time.
- **The Vite demo site** is *not* ported. It still lives at `../source/site/`
  and is built with the original Node toolchain.

Do not add a `vendor/` or `site/` directory under `rust/`. If you need a font
file in a test, point at `../source/vendor/...`.

## Build pipeline summary

The pipeline mirrors `../source/` and runs in three stages. Each stage is a
function in `gen-font` (or downstream crates) and is independently testable.

### Stage 1 — Bake Noto variable -> static

Input: Noto Sans JP variable font from `../source/vendor/`.
Action: instance the variable font at the target weight axes and emit a static
font. In the Python source this is the `ofl-font-baker` step; here it is the
in-tree **`gen_font::baker`** module — see "Library swaps" below.

### Stage 2 — Proportionalise

Take the static Japanese font and turn it into a horizontally proportional UI
font. Operations, in order:

1. **`palt` -> `hmtx`** — apply the OpenType `palt` (proportional alternate
   metrics) feature destructively into the horizontal metrics table, so the
   font is proportional without needing the feature enabled by the consumer.
2. **Tracking** — apply uniform letter-spacing adjustment.
3. **Glyph spacing** — per-glyph side-bearing tweaks for visual rhythm.
4. **Strip extreme bbox** — neutralise glyphs whose bounding box is
   pathologically tall/wide for a horizontal UI context. See "Vertical metrics
   policy" below.

### Stage 3 — Merge with Inter

Combine the proportionalised JP font with Inter (Latin) into a single font
file. Done via `gen_font::baker`, which exposes the merge primitive that
`ofl-font-baker` provides on the Python side.

## Library swaps

| Python (`../source/`) | Rust (this port)              |
| --------------------- | ----------------------------- |
| `fontTools`           | `skrifa` + `write-fonts`      |
| `ofl-font-baker`      | `gen_font::baker` (in-tree)   |
| `pytest` fixtures     | `OnceLock`-cached test setup  |

`skrifa` is read-only / parsing; `write-fonts` is the mutation+serialisation
half. Reach for `skrifa` when inspecting an existing font and `write-fonts`
when emitting one. Avoid pulling in any other font crate without a clear
reason — the surface area is already wide enough.

The `gen_font::baker` module is the in-tree replacement for the external
`ofl-font-baker` Python package. It owns the variable-instancing and
font-merging primitives. Treat it as a stable internal API: changes there
ripple through every stage.

## Vertical metrics policy

This is a **horizontal-only UI font**. We deliberately do not support vertical
typesetting. As a consequence, `_strip_extreme_glyphs` (Stage 2.4)
**neutralises the iteration marks 〱 (U+3031) and 〲 (U+3032)** — these glyphs
are vertical-only kana iteration marks whose natural bbox wrecks line height
in horizontal layout.

This is an **intentional Illustrator-ergonomics tradeoff**: the font is built
for use in Adobe Illustrator and similar horizontal-layout design tools, where
predictable line metrics matter more than preserving rare vertical-only
glyphs. Do not "fix" this without reading `docs/ARCHITECTURE.md` and
discussing it — the same rationale governs several other small metric
decisions documented there.

## Code style

Idiomatic Rust. Specifically:

- **Borrow over own.** Prefer `&str` over `String`, `&[T]` over `Vec<T>`, and
  `&Path` over `PathBuf` in function signatures. Take owned values only when
  the function genuinely needs ownership (storing, sending, mutating in a way
  the caller shouldn't see).
- **Error types depend on layer:**
  - **Top-level entry points** (binary `main`, top-level CLI handlers) return
    `Result<T, anyhow::Error>`. `anyhow` is fine here because the only
    consumer is a human reading a stack trace.
  - **Library functions** (anything callable from another crate, including
    crate-public functions in `gen-font`) return `Result<T, ThisError>` where
    `ThisError` is a `thiserror`-derived enum scoped to the module or crate.
    Callers need to be able to match on failure modes.
- Avoid `unwrap()` / `expect()` outside tests and `const`-evaluable contexts.
  If you genuinely cannot fail, document why with a comment.
- Keep modules small and named after the domain concept, not the Rust shape
  (`palt`, `tracking`, `baker` — not `utils`, `helpers`, `common`).
- Run `cargo fmt` and `cargo clippy` before committing. Treat clippy warnings
  as errors unless explicitly `#[allow]`ed with a reason.

## Tests

- Run with `cargo test` from the workspace root. Each crate's tests live in
  `crates/<name>/tests/`.
- **Reuse the Noto subset across tests with `std::sync::OnceLock`**, not by
  re-loading the font in every test. This is the Rust equivalent of `pytest`'s
  session-scoped fixtures used in `../source/`. Pattern:

  ```rust
  use std::sync::OnceLock;

  fn noto_subset() -> &'static FontRef<'static> {
      static FONT: OnceLock<FontRef<'static>> = OnceLock::new();
      FONT.get_or_init(|| {
          // load once; subsequent tests reuse this handle
          load_noto_subset()
      })
  }
  ```

  Loading a full Noto font is multi-hundred-megabyte and noticeably slow.
  `OnceLock` keeps the test suite under a second.
- Integration tests should exercise full stages end-to-end where practical
  (Stage 2 input -> Stage 2 output, byte-comparable against a fixture).
- Keep fixtures small and committed; large goldens belong in `../source/`.
