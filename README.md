# gen-interface-jp-rs

A Rust port of [`yamatoiizuka/gen-interface-jp`](https://github.com/yamatoiizuka/gen-interface-jp) — a font build pipeline that blends Inter with Noto Sans JP into a typeface designed for digital interfaces.

> **Status — work in progress.** The workspace compiles cleanly (`cargo check`, `cargo build --release`) and the test suite passes (`cargo test --workspace`). The classification, range, JIS, CSS, manifest, ZIP, npm-package, and GitHub-asset-URL layers are fully ported. The actual font byte mutation (palt baking, tracking, glyph spacing, bbox strip, x-scale, baker bake / merge) is wired in shape but currently `bail!`s with a `TODO(impl)` message — it needs the `skrifa` 0.36 / `write-fonts` 0.42 surface threaded through. See `docs/ARCHITECTURE.md` for the full pipeline spec.

## Why a Rust port

The original project is a Python pipeline built on `fontTools` + `ofl-font-baker`. This port:

- Replaces the Python toolchain with a single statically-linked binary set.
- Trades `fontTools` for [`skrifa`](https://crates.io/crates/skrifa) + [`write-fonts`](https://crates.io/crates/write-fonts) (the Google Fonts crates).
- Reimplements `ofl-font-baker` as an in-tree `gen_font::baker` module — no external Python package, no extra release boundary.
- Keeps the build artefact intent bit-equivalent to what `../source/` produces.

When in doubt about *what* the pipeline should do, consult the original Python source. When in doubt about *how* to structure the Rust, see `CLAUDE.md`.

## Layout

```
rust/
├── Cargo.toml              # workspace root
├── rust-toolchain.toml
├── justfile                # shortcuts for cargo + npm flows
├── crates/
│   ├── gen-font/           # Stage 1+2: bake variable -> static, proportionalise, merge
│   ├── gen-webfont/        # Stage 4: WOFF2 subsetting + @font-face CSS + manifest
│   └── gen-release/        # Stage 5: GitHub Release zip, npm package, Pages mirror
└── docs/
    └── ARCHITECTURE.md     # full pipeline spec (English + Japanese)
```

Each crate ships:

- A library (`lib.rs`) covering the domain logic (palt extraction, range merging, manifest serialisation, etc.).
- A binary (`bin/<crate>_build.rs`) exposing the same CLI surface as the original Python `python3 -m <module>` invocation.
- Integration tests under `crates/<crate>/tests/`.

## Quick start

Prerequisites:

- Rust stable (`rustup show` should resolve via `rust-toolchain.toml`).
- Vendor fonts laid out under a sibling `../source/vendor/` tree — this port deliberately does not duplicate the original's `vendor/fonts/` directory. Clone [`yamatoiizuka/gen-interface-jp`](https://github.com/yamatoiizuka/gen-interface-jp) next to this repository as `source/`.
- `just` (optional, for the shortcuts).

```bash
# Stage 1+2+3: build TTFs for every (family × weight)
just font
# or: cargo run -p gen-font --release --bin font_build

# Stage 4: build subset WOFF2 + per-weight CSS + manifest
just webfont
# or: cargo run -p gen-webfont --release --bin webfont_build -- --all --clean

# Stage 5: package GitHub Release zip, npm package, Pages mirror
just release
# or: cargo run -p gen-release --release --bin release_build
```

## Build pipeline summary

```
Source (vendored separately)
  Inter / InterDisplay (each weight, static TTF)
  Noto Sans JP (single variable font, wght axis)
        │
        ▼  gen-font: per (family × weight)
  [1] bake     — pin Noto wght axis (off-grid e.g. 465 for Regular)
  [2] proportionalise
        ├ palt → hmtx (three-bucket policy)
        ├ apply tracking
        ├ apply per-glyph sidebearing tweaks
        └ strip extreme-bbox glyphs (〱〲)
  [3] merge    — Inter + proportional Noto via gen_font::baker
        │
        ▼  gen-webfont
  [4] subset   — googlefonts/nam-files japanese_default.txt
                 + per-weight CSS (`100.css`, `display-400.css`, `all.css`)
        │
        ▼  gen-release
  [5] package  — GitHub Release zip + npm package + Pages mirror
```

See `docs/ARCHITECTURE.md` for the full spec, including the proportional-metrics three-bucket policy, the Illustrator bbox problem, and per-strategy webfont slicing details.

## Tests

```bash
cargo test --workspace
```

Currently 113 tests across the workspace cover:

- Glyph-name / kana / CJK classification (`gen_font::classify`)
- Family / weight tables and lookup helpers
- `gen_font::baker::parse_codepoint_spec` (single, range, mixed, error)
- Codepoint-range merge and `unicode-range:` formatting
- JIS X 0208 row → Unicode mapping via EUC-JP
- Subset plan disjoint-coverage invariants
- googlefonts/nam-files textproto parser (including `}`-in-comment edge case)
- `@font-face` CSS shape (multi-line + minified)
- Manifest gzip / brotli sizing and JSON shape
- `OFL.txt` composition and GitHub asset URL contract
- `version` priority chain (CLI arg → `GITHUB_REF_NAME` → `Cargo.toml`)

Tests that need real font fixtures are deferred until the font-mutation stages land.

## Status: what works, what doesn't

**Implemented (compiles, runs, has tests):**

- All non-font-bytes logic — classification, range math, JIS mapping, subset planning, googlefonts strategy parsing, CSS generation, manifest building, ZIP / npm / GitHub URL packaging, version resolution.
- All three CLI binaries (`font_build`, `webfont_build`, `release_build`) with `--help` output matching the Python originals.

**Stubbed (signature exists, body is `bail!` with a `TODO(impl)` message):**

- `gen_font::baker::bake` and `gen_font::baker::merge_fonts` — variable-font instancing and font merge against `skrifa`/`write-fonts`.
- `gen_font::proportional::make_proportional` — per-glyph palt baking into hmtx.
- `gen_font::tracking::apply_tracking`, `gen_font::glyph_spacing::apply_glyph_spacing`, `gen_font::strip_extreme::strip_extreme_glyphs`, `gen_font::x_scale::apply_x_scale` — hmtx / glyf / GSUB mutation.
- `gen_font::palt::read_palt`, `gen_font::palt::remove_prop_features` — GPOS walk + table rebuild.
- `gen_webfont::subset::build_woff2_subset` — TTF → subset WOFF2 (the planning is done; the actual subsetter wiring is not).

The orchestration in `gen_font::build::build_one` shows the intended call sequence, so finishing the stubs is mostly mechanical once the `write-fonts` mutation surface is pinned.

## License

Source code in this repository is [MIT License](LICENSE); the generated fonts (when produced) are licensed under [SIL Open Font License 1.1](https://scripts.sil.org/OFL).

The original project's vendor fonts (read from `../source/vendor/` at build time) follow their bundled licenses.

## References

- [`yamatoiizuka/gen-interface-jp`](https://github.com/yamatoiizuka/gen-interface-jp) — the original Python project this is a port of.
- [Noto Sans JP](https://github.com/notofonts/noto-cjk)
- [Inter](https://github.com/rsms/inter)
- [skrifa](https://github.com/googlefonts/fontations) / [write-fonts](https://github.com/googlefonts/fontations) — the Rust font crates that replace `fontTools`.
