//! Smoke test: confirm `build_one` orchestration runs through Stage 1
//! without panicking.
//!
//! Stage 1 (`baker::bake`) and Stage 3 (`baker::merge_fonts`) both bail
//! with a TODO(impl) error on the current `skrifa` / `write-fonts` 0.42
//! surface (see `crates/gen-font/src/baker.rs`). We don't expect a clean
//! `Ok(_)` until those land — we *do* expect a clean, well-formed
//! `Err(_)` whose message names the TODO. A panic, a `parse failed` from
//! a downstream stage, or a "file not found" on the prop-path artefact
//! would all signal the orchestration itself regressed.
//!
//! Per the workspace's "no vendored fonts" policy (CLAUDE.md), the
//! Noto / Inter masters live next door at `../source/vendor/`. If the
//! vendor tree is absent (e.g. CI without `../source/`), the test
//! self-skips via `eprintln!` + early return rather than failing.

use std::path::Path;

use gen_font::build::{build_one, BuildPaths};
use gen_font::families::FamilyConfig;
use gen_font::weights::WEIGHTS;

#[test]
fn build_one_runs_without_panicking() {
    let paths = BuildPaths::default_for_rust_workspace();

    // Self-skip if the upstream vendor tree isn't present. We probe the
    // canonical Noto path; if it's missing the rest of the vendor
    // directory likely is too, and there's nothing to smoke-test.
    if !Path::new(&paths.noto_variable).is_file() {
        eprintln!(
            "skipping build_one_runs_without_panicking: {} not present",
            paths.noto_variable.display()
        );
        return;
    }

    let family = FamilyConfig::lookup("normal").expect("'normal' family must exist in FAMILIES");
    // WEIGHTS[3] is Regular (400 / noto_wght_axis 465).
    let weight = &WEIGHTS[3];
    assert_eq!(
        weight.weight_name, "Regular",
        "weight matrix shifted; update the index in this test"
    );

    let result = build_one(&paths, family, weight);

    match result {
        Ok(_) => {
            // Full pipeline ran end-to-end. Unexpected today but the
            // success case once Stage 1 + Stage 3 land — accept it.
        }
        Err(e) => {
            // Walk the anyhow chain so we catch the TODO message even
            // if it's wrapped by a `.context(...)` layer above.
            let chain: String = e
                .chain()
                .map(std::string::ToString::to_string)
                .collect::<Vec<_>>()
                .join(" / ");
            assert!(
                chain.contains("TODO") || chain.contains("not yet implemented"),
                "expected a Stage 1 or Stage 3 TODO bail, got: {chain}"
            );
        }
    }
}
