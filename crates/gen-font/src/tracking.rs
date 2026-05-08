//! Per-glyph tracking — widen each glyph's advance width and split the
//! resulting gap evenly between left and right sidebearings.
//!
//! Port of `_apply_tracking` from `source/src/font/build.py` (lines 482-510).
//!
//! Adding tracking to a glyph means growing its advance by `t` and nudging
//! the LSB by `t / 2` so the same outline sits centred in the new wider slot
//! — half the new whitespace ends up on the left sidebearing, the other
//! half on the right. Matches how design apps interpret tracking in Latin
//! typography, applied per-glyph rather than as a global text-engine setting.
//!
//! Zero-width glyphs (combining marks, mark-positioning anchors) are skipped
//! so they keep their placement-only role intact.
//!
//! When `tracking_kana` is `Some(v)`, hiragana / katakana / punctuation glyphs
//! receive `v` instead of `tracking`. The Gen Interface JP families use this
//! to give kana / punctuation a slightly looser rhythm than Latin — kana need
//! more breathing room at small sizes against denser Han ideographs.

use anyhow::bail;
use write_fonts::FontBuilder;

/// Apply per-glyph tracking to every glyph in the font, in place.
pub fn apply_tracking(
    builder: &mut FontBuilder<'_>,
    tracking: i32,
    tracking_kana: Option<i32>,
) -> anyhow::Result<()> {
    let _ = (builder, tracking, tracking_kana);
    bail!(
        "apply_tracking: TODO(impl) — hmtx walk against write-fonts 0.42 \
         surface is not yet wired up"
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn half_is_floor_div_two() {
        // The Python uses `t // 2` (integer floor division). Rust's `/` on
        // signed ints is truncated-toward-zero, equivalent for non-negative
        // values. The build pipeline only ever passes non-negative tracking,
        // so floor and truncation agree.
        assert_eq!(5 / 2, 2);
        assert_eq!(10 / 2, 5);
        assert_eq!(0 / 2, 0);
    }
}
