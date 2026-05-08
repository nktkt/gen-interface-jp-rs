//! Family / weight matrix.
//!
//! The `noto_wght_axis` column is the wght-axis location used to instantiate
//! Noto Sans JP. Inter's discrete static masters happen to live at the round
//! 100/200/.../800 positions, but Noto's variable axis is non-linear: pulling
//! the axis at 400 yields a CJK weight that visually reads lighter than Inter
//! Regular. The values below were tuned by eye-matching CJK stem density to
//! each Inter master, hence the off-grid numbers (e.g. 465 for Regular, 800
//! for Bold).

#[derive(Debug, Clone, Copy)]
pub struct WeightSpec {
    pub weight_num: u16,           // 100, 200, ..., 800
    pub weight_name: &'static str, // "Thin", ..., "ExtraBold"
    pub noto_wght_axis: i32,       // 100, 260, 355, 465, 575, 690, 800, 900
}

pub const WEIGHTS: &[WeightSpec] = &[
    WeightSpec { weight_num: 100, weight_name: "Thin",       noto_wght_axis: 100 },
    WeightSpec { weight_num: 200, weight_name: "ExtraLight", noto_wght_axis: 260 },
    WeightSpec { weight_num: 300, weight_name: "Light",      noto_wght_axis: 355 },
    WeightSpec { weight_num: 400, weight_name: "Regular",    noto_wght_axis: 465 },
    WeightSpec { weight_num: 500, weight_name: "Medium",     noto_wght_axis: 575 },
    WeightSpec { weight_num: 600, weight_name: "SemiBold",   noto_wght_axis: 690 },
    WeightSpec { weight_num: 700, weight_name: "Bold",       noto_wght_axis: 800 },
    WeightSpec { weight_num: 800, weight_name: "ExtraBold",  noto_wght_axis: 900 },
];

impl WeightSpec {
    /// Look up a weight by either its name (e.g. `"Regular"`) or its
    /// stringified numeric value (e.g. `"400"`).
    pub fn by_name_or_num(s: &str) -> Option<&'static WeightSpec> {
        WEIGHTS
            .iter()
            .find(|w| w.weight_name == s || w.weight_num.to_string() == s)
    }
}
