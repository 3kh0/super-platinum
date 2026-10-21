//! Exact known wire names, not a parser or an action/family resolver.
#[path = "tones_data.rs"]
mod tones_data;

pub(super) fn glyph(name: &str) -> Option<&'static str> {
    let (base, tone) = name.split_once("::skin-tone-")?;
    let index = tones_data::BASES
        .binary_search_by_key(&base, |(name, _)| *name)
        .ok()?;
    tones_data::GROUPS[tones_data::BASES[index].1]
        .iter()
        .find_map(|(suffix, glyph)| (*suffix == tone).then_some(*glyph))
}
