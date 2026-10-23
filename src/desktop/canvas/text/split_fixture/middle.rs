//! Finite middle-typing fixture bodies and retained original parent histories.
use super::*;

pub(super) const UNICODE_TYPED: Corpus = Corpus {
    seed: "Caf\u{e9} & \u{65e5}\u{672c}\u{1f600}Second \u{65e5}\u{672c}\u{1f600} & paragraph",
    seed_rtml: "Caf\u{e9} &amp; \u{65e5}\u{672c}\u{1f600}Second \u{65e5}\u{672c}\u{1f600} &amp; paragraph",
    right: "Second \u{65e5}\u{672c}\u{1f600} & paragraph",
    html: "<div class='quip-canvas-content'><h1>Paragraph splitting</h1><p id='paragraph'>Caf\u{e9} &amp; \u{65e5}\u{672c}\u{1f600}Second \u{65e5}\u{672c}\u{1f600} &amp; paragraph</p><p id='neighbor'>Another paragraph</p></div>",
    ..UNICODE
};
pub(in crate::canvas::text) fn is_typed() -> bool {
    !is_end() && !is_start() && typing() != "none"
}
pub(in crate::canvas::text) fn parent_mode() -> &'static str {
    match std::env::var("SUPER_PLATINUM_CANVAS_SPLIT_PARENT").as_deref() {
        Ok("retained") => "retained",
        _ => "bootstrap",
    }
}
pub(in crate::canvas::text) fn parent_valid() -> bool {
    match std::env::var("SUPER_PLATINUM_CANVAS_SPLIT_PARENT") {
        Ok(value) => value == "bootstrap" || (!is_end() && value == "retained"),
        Err(std::env::VarError::NotPresent) => true,
        Err(_) => false,
    }
}
pub(super) fn parent_source(raw: &RawMessage) -> RawMessage {
    if parent_mode() == "bootstrap" {
        return raw.remove(38).unwrap();
    }
    raw.replace_fields(38, &[m(38, &retained_wire(corpus().seed_rtml))])
        .unwrap()
}
fn retained_wire(source_rtml: &str) -> RawMessage {
    let seeds: Value = serde_json::from_str(include_str!("middle-original.json")).unwrap();
    let legacy: Value = serde_json::from_str(include_str!("middle-legacy-unicode.json")).unwrap();
    let seed = seeds["seeds"]
        .as_array()
        .unwrap()
        .iter()
        .chain(std::iter::once(&legacy))
        .find(|seed| seed["parentMode"] == "retained" && seed["sourceRtml"] == source_rtml)
        .expect("retained parent must match the exact selected source");
    RawMessage::decode(
        &STANDARD
            .decode(seed["canonicalSourceSessionBase64"].as_str().unwrap())
            .unwrap(),
    )
    .unwrap()
    .replace_fields(4, &[n(4, 8_000_000_000_000_000)])
    .unwrap()
}
pub(super) fn reports(selector: &str, unicode: bool) -> &'static [&'static str] {
    match (selector, unicode) {
        ("correction", false) => &["New", "New draftx", "New draft", "New paragraph"],
        ("correction", true) => &[
            "New",
            "New \u{65e5}\u{672c}\u{1f600} draftx",
            "New \u{65e5}\u{672c}\u{1f600} draft",
            "New \u{65e5}\u{672c}\u{1f600} paragraph",
        ],
        ("erase", false) => &["Second paragraphx", ""],
        ("erase", true) => &["Second \u{65e5}\u{672c}\u{1f600} & paragraphx", ""],
        ("restore", false) => &["Second paragraphx", "Second paragraph"],
        ("restore", true) => &[
            "Second \u{65e5}\u{672c}\u{1f600} & paragraphx",
            "Second \u{65e5}\u{672c}\u{1f600} & paragraph",
        ],
        _ => &[],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super_platinum_core::canvas::edit::{structure::SplitWitness, text::TextDraft};
    #[test]
    fn retained_original_seed_binding_changes_only_independent_lease() {
        let seeds: Value = serde_json::from_str(include_str!("middle-original.json")).unwrap();
        let legacy: Value =
            serde_json::from_str(include_str!("middle-legacy-unicode.json")).unwrap();
        for seed in seeds["seeds"]
            .as_array()
            .unwrap()
            .iter()
            .chain(std::iter::once(&legacy))
        {
            if seed["parentMode"] == "bootstrap" {
                assert!(seed["canonicalSourceSessionBase64"].is_null());
                continue;
            }
            let raw = RawMessage::decode(
                &STANDARD
                    .decode(seed["canonicalSourceSessionBase64"].as_str().unwrap())
                    .unwrap(),
            )
            .unwrap();
            let bound = retained_wire(seed["sourceRtml"].as_str().unwrap());
            assert_eq!(bound.remove(4).unwrap(), raw.remove(4).unwrap());
            assert_eq!(bound.uint(2).unwrap(), Some(1000));
            assert_eq!(bound.uint(3).unwrap(), Some(1000));
            assert_eq!(bound.string(1).unwrap(), Some("paragraph"));
            assert!(!bound.has(10) && !bound.has(12));
        }
    }
    #[test]
    fn typed_fixture_preserves_original_tail_and_all_changed_reports() {
        for c in [&ASCII, &UNICODE_TYPED] {
            let before = TextDraft::from_rtml(c.seed_rtml).unwrap();
            let parent = TextDraft::from_rtml(c.left_rtml).unwrap();
            let tail = TextDraft::from_rtml(&typing::escaped(c.right)).unwrap();
            for kind in ["correction", "erase", "restore"] {
                let updates = reports(kind, c.name == "unicode")
                    .iter()
                    .map(|text| TextDraft::from_rtml(&typing::escaped(text)).unwrap())
                    .collect::<Vec<_>>();
                let witness = SplitWitness::new(&before, c.offset, &parent, &tail)
                    .unwrap()
                    .with_child_updates(&updates)
                    .unwrap();
                assert_eq!(
                    witness.initial_child_draft().rtml().unwrap(),
                    tail.rtml().unwrap()
                );
                assert_eq!(
                    witness.child_draft().rtml().unwrap(),
                    updates.last().unwrap().rtml().unwrap()
                );
                assert_eq!(witness.child_updates().len(), updates.len());
            }
        }
    }
}
