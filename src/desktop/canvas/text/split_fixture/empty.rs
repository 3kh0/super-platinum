//! Finite original-runtime empty source histories and following-row layouts.
use super::*;
pub(in crate::canvas::text) fn history() -> &'static str {
    match std::env::var("SUPER_PLATINUM_CANVAS_EMPTY_HISTORY").as_deref() {
        Ok("retained-positive") => "retained-positive",
        Ok("retained-zero") => "retained-zero",
        _ => "bootstrap",
    }
}
pub(in crate::canvas::text) fn layout() -> &'static str {
    match std::env::var("SUPER_PLATINUM_CANVAS_EMPTY_LAYOUT").as_deref() {
        Ok("text-following-ascii") => "text-following-ascii",
        Ok("text-following-unicode") => "text-following-unicode",
        Ok("empty-final-following") => "empty-final-following",
        Ok("empty-nonfinal-following") => "empty-nonfinal-following",
        _ => "no-following",
    }
}
pub(in crate::canvas::text) fn valid() -> bool {
    for (name, allowed) in [
        (
            "SUPER_PLATINUM_CANVAS_EMPTY_HISTORY",
            &["bootstrap", "retained-positive", "retained-zero"][..],
        ),
        (
            "SUPER_PLATINUM_CANVAS_EMPTY_LAYOUT",
            &[
                "no-following",
                "text-following-ascii",
                "text-following-unicode",
                "empty-final-following",
                "empty-nonfinal-following",
            ][..],
        ),
    ] {
        match std::env::var(name) {
            Ok(v) if is_empty() && allowed.contains(&v.as_str()) => {}
            Err(std::env::VarError::NotPresent) => {}
            _ => return false,
        }
    }
    !is_empty()
        || ((typing() != "none"
            || std::env::var("SUPER_PLATINUM_CANVAS_SPLIT_CORPUS").map_or(true, |v| v == "ascii"))
            && parent_mode() == "bootstrap"
            && reuse_base() == "positive"
            && reuse_content() == "empty")
}
pub(super) fn parent() -> &'static str {
    if history() == "retained-zero" {
        "temp:C:Dte0000000000000000000000001"
    } else {
        "paragraph"
    }
}
pub(super) fn following() -> &'static str {
    if history() == "retained-zero" {
        "temp:C:Dte0000000000000000000000000"
    } else {
        "following"
    }
}
pub(super) const CORPUS: Corpus = Corpus {
    name: "ascii",
    seed: "",
    left: "",
    right: "",
    edited: "Original paragraph edited",
    offset: 0,
    seed_rtml: "",
    left_rtml: "",
    edited_rtml: "Original paragraph edited",
    html: "<div class='quip-canvas-content'><h1>Paragraph structure verification</h1><p id='paragraph'></p></div>",
};
const UNICODE: Corpus = Corpus {
    name: "unicode",
    ..CORPUS
};
pub(super) fn corpus() -> &'static Corpus {
    if std::env::var("SUPER_PLATINUM_CANVAS_SPLIT_CORPUS").as_deref() == Ok("unicode") {
        &UNICODE
    } else {
        &CORPUS
    }
}
pub(super) fn reports(selector: &str, unicode: bool) -> &'static [&'static str] {
    match (selector, unicode) {
        ("correction", false) => &["New", "New draftx", "New draft", "New paragraph"],
        ("correction", true) => &[
            "New \u{65e5}\u{672c}\u{1f600} &",
            "New \u{65e5}\u{672c}\u{1f600} & draftx",
            "New \u{65e5}\u{672c}\u{1f600} & draft",
            "New \u{65e5}\u{672c}\u{1f600} & paragraph",
        ],
        ("erase", false) => &["New", ""],
        ("erase", true) => &["New \u{65e5}\u{672c}\u{1f600} &", ""],
        ("restore", false) => &["New", "New draftx", "New", ""],
        ("restore", true) => &[
            "New \u{65e5}\u{672c}\u{1f600} &",
            "New \u{65e5}\u{672c}\u{1f600} & draftx",
            "New \u{65e5}\u{672c}\u{1f600} &",
            "",
        ],
        _ => &[],
    }
}
fn source_session(mode: &str, id: &str) -> Option<RawMessage> {
    let seeds: Value = serde_json::from_str(include_str!("empty-original.json")).unwrap();
    let seed = seeds["seeds"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["mode"] == mode)
        .unwrap();
    seed["canonicalSession"].as_str().map(|v| {
        RawMessage::decode(&STANDARD.decode(v).unwrap())
            .unwrap()
            .replace_fields(1, &[t(1, id)])
            .unwrap()
            .replace_fields(4, &[n(4, 8_000_000_000_000_000)])
            .unwrap()
    })
}
fn row(
    mode: &str,
    template: &RawMessage,
    id: &str,
    position: &str,
    text: &str,
    language: bool,
    empty_history: bool,
) -> RawMessage {
    let mut raw = template
        .replace_fields(1, &[t(1, id)])
        .unwrap()
        .replace_fields(8, &[t(8, position)])
        .unwrap()
        .replace_fields(12, &[m(12, &body(text, language))])
        .unwrap()
        .remove(38)
        .unwrap();
    if empty_history {
        if let Some(wire) = source_session(mode, id) {
            raw = raw.replace_fields(38, &[m(38, &wire)]).unwrap();
        }
    }
    raw
}
pub(super) fn snapshot(previous: &CanvasCollabSnapshot) -> Arc<CanvasCollabSnapshot> {
    snapshot_with(previous, history(), layout())
}
fn snapshot_with(
    previous: &CanvasCollabSnapshot,
    mode: &str,
    placement: &str,
) -> Arc<CanvasCollabSnapshot> {
    let parent = if mode == "retained-zero" {
        "temp:C:Dte0000000000000000000000001"
    } else {
        "paragraph"
    };
    let following = if mode == "retained-zero" {
        "temp:C:Dte0000000000000000000000000"
    } else {
        "following"
    };
    let template = previous.section("paragraph").unwrap().raw();
    let mut rows = vec![
        previous.section("title").unwrap().raw().clone(),
        row(mode, template, parent, "aac", "", true, true),
    ];
    if placement != "no-following" {
        let text = match placement {
            "text-following-ascii" => "Another paragraph",
            "text-following-unicode" => "Caf\u{e9} &amp; \u{65e5}\u{672c}\u{1f600}Second paragraph",
            _ => "",
        };
        rows.push(row(
            mode,
            template,
            following,
            "aae",
            text,
            false,
            text.is_empty(),
        ));
    }
    if placement == "empty-nonfinal-following" {
        rows.push(row(
            mode,
            template,
            "last-text",
            "aag",
            "Last paragraph",
            false,
            false,
        ));
    }
    rebuild(previous, rows)
}
pub(in crate::canvas::text) fn matches(
    checkpoint: &super_platinum_core::canvas::edit::structure::recovery::Checkpoint,
) -> bool {
    let Ok(value) = serde_json::to_value(checkpoint) else {
        return false;
    };
    let typed = typing() != "none";
    let action = &value["action"][if typed { "EmptyTyping" } else { "EmptyEnter" }];
    let updates = typing_updates()
        .iter()
        .map(|draft| draft.rtml().unwrap())
        .collect::<Vec<_>>();
    if value["version"] != if typed { 11 } else { 10 }
        || !checkpoint.is_empty_enter()
        || action["reused"] != is_reuse()
        || (typed && action["child_updates"] != json!(updates))
    {
        return false;
    }
    let expected = snapshot(&super::super::structure_fixture::snapshot());
    let Some(rows) = value["sections"].as_array() else {
        return false;
    };
    rows.len() == expected.sections.len()
        && expected.sections.iter().all(|s| {
            rows.iter().any(|v| {
                serde_json::from_value::<Vec<u8>>(v.clone()).ok().as_deref() == Some(s.raw().raw())
            })
        })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn finite_empty_snapshots_keep_source_histories_and_reference_order() {
        let previous = super::super::super::structure_fixture::snapshot();
        for mode in ["bootstrap", "retained-positive", "retained-zero"] {
            for layout in [
                "no-following",
                "text-following-ascii",
                "text-following-unicode",
                "empty-final-following",
                "empty-nonfinal-following",
            ] {
                let snapshot = snapshot_with(&previous, mode, layout);
                let id = if mode == "retained-zero" {
                    "temp:C:Dte0000000000000000000000001"
                } else {
                    "paragraph"
                };
                let original = snapshot.section(id).unwrap();
                assert_eq!(original.raw().uint(2).unwrap(), Some(1000));
                assert_eq!(original.raw().string(8).unwrap(), Some("aac"));
                let wire = original.raw().message(38).unwrap();
                assert_eq!(wire.is_none(), mode == "bootstrap");
                if let Some(wire) = wire {
                    assert_eq!(
                        wire.uint(2).unwrap(),
                        Some(if mode == "retained-zero" { 0 } else { 1000 })
                    );
                }
                assert_eq!(
                    snapshot.sections.len(),
                    if layout == "no-following" {
                        2
                    } else if layout == "empty-nonfinal-following" {
                        4
                    } else {
                        3
                    }
                );
                let now = 1_792_000_000_000_000;
                let intent = EndIntent::capture_empty(&snapshot, id, now).unwrap();
                let prepared = intent
                    .prepare(
                        &snapshot,
                        &super_platinum_core::canvas::edit::structure::EndWitness::empty(),
                        now,
                    )
                    .unwrap();
                assert!(prepared.is_empty_enter());
                assert_eq!(prepared.reuses_child(), layout == "empty-final-following");
            }
        }
    }
    #[test]
    fn original_empty_history_manifest_is_not_a_base_flip() {
        let value: Value = serde_json::from_str(include_str!("empty-original.json")).unwrap();
        let seeds = value["seeds"].as_array().unwrap();
        assert_eq!(seeds.len(), 3);
        assert!(seeds[0]["canonicalSession"].is_null());
        for (index, base, count) in [(1, 1000, 2), (2, 0, 1)] {
            let wire = RawMessage::decode(
                &STANDARD
                    .decode(seeds[index]["canonicalSession"].as_str().unwrap())
                    .unwrap(),
            )
            .unwrap();
            assert_eq!(wire.uint(2).unwrap(), Some(base));
            assert_eq!(wire.uint(3).unwrap(), Some(1000));
            assert_eq!(
                seeds[index]["initialized"]["history"]
                    .as_array()
                    .unwrap()
                    .len(),
                count
            );
        }
    }

    #[test]
    fn fixed_empty_typing_reports_preserve_unicode_and_distinct_empty_returns() {
        for unicode in [false, true] {
            let prefix = if unicode {
                "New \u{65e5}\u{672c}\u{1f600} &"
            } else {
                "New"
            };
            assert_eq!(
                reports("correction", unicode),
                &[
                    prefix,
                    &format!("{prefix} draftx"),
                    &format!("{prefix} draft"),
                    &format!("{prefix} paragraph")
                ]
            );
            assert_eq!(reports("erase", unicode), &[prefix, ""]);
            assert_eq!(
                reports("restore", unicode),
                &[prefix, &format!("{prefix} draftx"), prefix, ""]
            );
            for selector in ["correction", "erase", "restore"] {
                for text in reports(selector, unicode) {
                    let wire = typing::escaped(text);
                    assert_eq!(
                        super_platinum_core::canvas::edit::text::TextDraft::from_rtml(&wire)
                            .unwrap()
                            .rtml()
                            .unwrap(),
                        wire
                    );
                }
            }
        }
    }

    #[test]
    fn typed_empty_fixture_prepares_whole_pair_for_all_source_histories() {
        use super_platinum_core::canvas::edit::{structure::EndWitness, text::TextDraft};
        let previous = super::super::super::structure_fixture::snapshot();
        for mode in ["bootstrap", "retained-positive", "retained-zero"] {
            for layout in ["no-following", "empty-final-following"] {
                let snapshot = snapshot_with(&previous, mode, layout);
                let id = if mode == "retained-zero" {
                    "temp:C:Dte0000000000000000000000001"
                } else {
                    "paragraph"
                };
                for selector in ["correction", "erase", "restore"] {
                    let updates = reports(selector, true)
                        .iter()
                        .map(|t| TextDraft::from_rtml(&typing::escaped(t)).unwrap())
                        .collect::<Vec<_>>();
                    let witness = EndWitness::empty().with_child_updates(&updates).unwrap();
                    let now = 1_792_000_000_000_000;
                    let prepared = EndIntent::capture_empty(&snapshot, id, now)
                        .unwrap()
                        .prepare(&snapshot, &witness, now)
                        .unwrap();
                    assert!(prepared.is_empty_enter() && prepared.is_empty_typing());
                    assert!(prepared.child_updates() == updates);
                    assert!(prepared.child_draft() == updates.last().unwrap());
                    assert_eq!(prepared.reuses_child(), layout == "empty-final-following");
                    let checkpoint = serde_json::to_value(prepared.checkpoint().unwrap()).unwrap();
                    assert_eq!(checkpoint["version"], 11);
                    assert_eq!(
                        checkpoint["action"]["EmptyTyping"]["child_updates"]
                            .as_array()
                            .unwrap()
                            .len(),
                        updates.len()
                    );
                }
            }
        }
    }
}
