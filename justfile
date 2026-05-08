# justfile for the Rust workspace port of x7/ver-5.
#
# Mirrors source/Makefile target-for-target, but invokes Rust binaries via
# `cargo run -p <crate> --bin <bin>` instead of Python modules.
#
# Requires: just (https://just.systems), cargo, npm, node.

set shell := ["bash", "-cu"]

# ---------------------------------------------------------------------------
# Configurable knobs (override at the CLI: `just WEBFONT_JOBS=4 webfont`).
# ---------------------------------------------------------------------------

WEBFONT_JOBS      := env_var_or_default("WEBFONT_JOBS", "8")
NPM_PACKAGE_DIR   := "dist/release/npm"
NPM_PUBLISH_FLAGS := env_var_or_default("NPM_PUBLISH_FLAGS", "--access public")
NPM_CACHE         := env_var_or_default("NPM_CACHE", justfile_directory() + "/.npm-cache")

# Default recipe: produce everything publishable.
default: release

# Alias for the Make `all` target — same as `default`.
all: release


# ---------------------------------------------------------------------------
# gen-font  —  TTF for both families x all weights
# ---------------------------------------------------------------------------

# Outputs land under dist/ttf/<Family>/. Web delivery goes through
# `just webfont` (subset WOFF2 served via unicode-range), not full WOFF2.
font:
    cargo run -p gen-font --bin font_build --release


# ---------------------------------------------------------------------------
# gen-webfont  —  unicode-range subsetting + benchmark
# ---------------------------------------------------------------------------

webfont: font
    cargo run -p gen-webfont --bin webfont_build --release -- --all --clean --strategy google-japanese --jobs {{WEBFONT_JOBS}}

# Throttled fetch comparison of the slicing plan against the full WOFF2.
# Runs the single-Regular pipeline (webfont_build without --all) which
# generates the full WOFF2 from the Regular TTF on demand into
# dist/webfont/GenInterfaceJP-Regular/, then drives benchmark.mjs.
# Independent of `just webfont` (which is the --all multi-weight path
# whose manifest shape differs from what benchmark.mjs reads).
#
# TODO: benchmark.mjs still lives in the original source tree
# (../source/src/webfont/benchmark.mjs). Decide whether to vendor it into
# the Rust workspace (e.g. crates/gen-webfont/bench/benchmark.mjs) or keep
# pointing at the source/ copy.
webfont-benchmark: font
    cargo run -p gen-webfont --bin webfont_build --release -- --clean --strategy google-japanese
    node ../source/src/webfont/benchmark.mjs


# ---------------------------------------------------------------------------
# gen-release  —  GitHub Release zips, npm package, Pages-hosted mirror
# ---------------------------------------------------------------------------

release: webfont
    cargo run -p gen-release --bin release_build --release

# Inspect the npm package that will be published (no upload, no tarball).
npm-pack: release
    cd {{NPM_PACKAGE_DIR}} && npm_config_cache={{NPM_CACHE}} npm pack --dry-run

# Validate npm publishing without actually uploading.
npm-publish-dry-run: release
    cd {{NPM_PACKAGE_DIR}} && npm_config_cache={{NPM_CACHE}} npm publish --dry-run {{NPM_PUBLISH_FLAGS}}

# Publish the generated webfont package to npm.
npm-publish: release
    cd {{NPM_PACKAGE_DIR}} && npm_config_cache={{NPM_CACHE}} npm publish {{NPM_PUBLISH_FLAGS}}


# ---------------------------------------------------------------------------
# site  —  Vite static demo site (unchanged from original repo: TS/Vite)
# ---------------------------------------------------------------------------

# site/dist/ is also the GitHub Pages artifact. The site source is not
# being ported to Rust, so we shell out to the existing TS/Vite project
# under ../source/site.
site:
    cd ../source/site && npm run build

# Local Vite dev server.
serve:
    cd ../source/site && npm run dev


# ---------------------------------------------------------------------------
# Meta
# ---------------------------------------------------------------------------

clean:
    cargo clean
    rm -rf dist
