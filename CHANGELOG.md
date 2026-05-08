# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Initial Rust port of [`yamatoiizuka/gen-interface-jp`](https://github.com/yamatoiizuka/gen-interface-jp).
- Three workspace crates: `gen-font`, `gen-webfont`, `gen-release`.
- CLI binaries: `font_build`, `webfont_build`, `release_build`.
- 125 unit + integration tests covering classification, range math, JIS mapping,
  subset planning, googlefonts/nam-files textproto parsing, CSS generation,
  manifest serialisation, ZIP / npm / GitHub URL packaging, and version resolution.

### Implemented
- `gen_font::tracking::apply_tracking` — real `hmtx` mutation.
- `gen_font::glyph_spacing::apply_glyph_spacing` — real `cmap` + `hmtx` walk.
- `gen_font::strip_extreme::strip_extreme_glyphs` — real `glyf` + `cmap` + `GSUB`
  neutralisation.
- `gen_font::glyph::shift_glyph_x` — real `glyf` point translation.
- `gen_font::x_scale::apply_x_scale` — real `glyf` + `hmtx` + `GPOS` X-axis scale.
- `gen_font::palt::read_palt` / `remove_prop_features` — real `GPOS` walk +
  rebuild.
- `gen_font::proportional::make_proportional` — real `palt` baking.
- `gen_font::classify::{glyph_names, get_vert_alternates, get_cjk_glyphs,
  get_kana_or_punct_glyphs}` — new classification helpers.
- `gen_font::build::build_one` — Stage 2 wiring with chained byte serialisation.

### Stubbed (TODO impl)
- `gen_font::baker::bake` — variable-font instancing (passthrough fallback present).
- `gen_font::baker::merge_fonts` — sub + base font merge (base passthrough fallback
  present).
- `gen_webfont::subset::build_woff2_subset` — TTF → subset WOFF2.

### Infrastructure
- `.github/workflows/ci.yml` — fmt + clippy + test + build on push/PR.
- `.github/workflows/release.yml` — tag-driven binary releases.
- `.github/dependabot.yml` — weekly cargo + actions updates.
- `clippy.toml` + workspace `[lints.clippy]` config.
- `rustfmt.toml`, `.editorconfig`, `rust-toolchain.toml`.
- `CONTRIBUTING.md`, `docs/WRITE_FONTS_NOTES.md`, `docs/ARCHITECTURE.md`
  Implementation status table.
- 5 new integration tests against the real Noto variable font (`test_roundtrip`,
  `test_tracking_integration`, `test_strip_extreme_integration`,
  `test_palt_integration`, `test_classify_glyph_sets`).

[Unreleased]: https://github.com/nktkt/gen-interface-jp-rs/commits/main
