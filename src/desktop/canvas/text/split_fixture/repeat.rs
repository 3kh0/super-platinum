//! Finite repeat-End fixtures, bound to the original reference source sessions.
use super::*;

pub(in crate::canvas::text) fn history() -> String {
    std::env::var("SUPER_PLATINUM_CANVAS_REPEAT_HISTORY").unwrap_or_else(|_| "bootstrap".into())
}
pub(in crate::canvas::text) fn layout() -> String {
    std::env::var("SUPER_PLATINUM_CANVAS_REPEAT_LAYOUT").unwrap_or_else(|_| "no-following".into())
}
pub(in crate::canvas::text) fn valid() -> bool {
    if !is_repeat() {
        return std::env::var_os("SUPER_PLATINUM_CANVAS_REPEAT_HISTORY").is_none()
            && std::env::var_os("SUPER_PLATINUM_CANVAS_REPEAT_LAYOUT").is_none();
    }
    matches!(
        history().as_str(),
        "bootstrap" | "retained-positive" | "retained-zero"
    ) && matches!(
        layout().as_str(),
        "no-following" | "text-following" | "empty-final-following" | "empty-nonfinal-following"
    ) && typing_valid()
        && parent_mode() == "bootstrap"
        && reuse_base() == "positive"
        && reuse_content() == "empty"
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
const ASCII_REPEAT: Corpus = Corpus {
    edited: "Keep this edited",
    edited_rtml: "Keep this edited",
    ..END_ASCII
};
const UNICODE_REPEAT: Corpus = Corpus {
    edited: "Caf\u{e9} & \u{65e5}\u{672c}\u{1f600} edited",
    edited_rtml: "Caf\u{e9} &amp; \u{65e5}\u{672c}\u{1f600} edited",
    ..END_UNICODE
};
pub(super) fn corpus() -> &'static Corpus {
    if std::env::var("SUPER_PLATINUM_CANVAS_SPLIT_CORPUS").as_deref() == Ok("unicode") {
        &UNICODE_REPEAT
    } else {
        &ASCII_REPEAT
    }
}
fn session(mode: &str, corpus: &str, source: &str, id: &str) -> Option<RawMessage> {
    let data: Value = serde_json::from_str(include_str!("repeat-original.json")).unwrap();
    let seed = data["seeds"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["mode"] == mode && s["corpus"] == corpus && s["id"] == source)
        .unwrap();
    seed["session"].as_str().map(|s| {
        RawMessage::decode(&STANDARD.decode(s).unwrap())
            .unwrap()
            .replace_fields(1, &[t(1, id)])
            .unwrap()
            .replace_fields(4, &[n(4, 8_000_000_000_000_000)])
            .unwrap()
    })
}
pub(in crate::canvas::text) fn snapshot(
    previous: &CanvasCollabSnapshot,
) -> Arc<CanvasCollabSnapshot> {
    snapshot_with(previous, &history(), &layout(), corpus())
}
fn snapshot_with(
    previous: &CanvasCollabSnapshot,
    mode: &str,
    placement: &str,
    corpus: &Corpus,
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
    let row = |id: &str, position: &str, text: &str, language: bool, source: Option<&str>| {
        let mut raw = template
            .replace_fields(1, &[t(1, id)])
            .unwrap()
            .replace_fields(8, &[t(8, position)])
            .unwrap()
            .replace_fields(12, &[m(12, &body(text, language))])
            .unwrap()
            .remove(38)
            .unwrap();
        if let Some(wire) = source.and_then(|name| session(&mode, corpus.name, name, id)) {
            raw = raw.replace_fields(38, &[m(38, &wire)]).unwrap();
        }
        raw
    };
    let mut rows = vec![
        previous.section("title").unwrap().raw().clone(),
        row(parent, "aac", corpus.seed_rtml, true, Some("paragraph")),
    ];
    if placement != "no-following" {
        let text = if placement == "text-following" {
            "Following paragraph"
        } else {
            ""
        };
        rows.push(row(
            following,
            "aae",
            text,
            false,
            (placement == "empty-final-following").then_some("following"),
        ));
    }
    if placement == "empty-nonfinal-following" {
        rows.push(row("last-text", "aag", "Last paragraph", false, None));
    }
    rebuild(previous, rows)
}
pub(in crate::canvas::text) fn checkpoint()
-> super_platinum_core::canvas::edit::structure::recovery::Checkpoint {
    use super_platinum_core::canvas::edit::{structure::EndWitness, text::TextDraft};
    let source = snapshot(&super::super::structure_fixture::snapshot());
    let now = checklist::save::now_usec().unwrap();
    let intent = EndIntent::capture_repeat(&source, parent(), now).unwrap();
    let witness = EndWitness::repeat(
        intent.seed(),
        corpus().offset,
        intent.seed(),
        &TextDraft::default(),
        &TextDraft::default(),
    )
    .unwrap();
    let witness = if typing() == "none" {
        witness
    } else {
        witness
            .with_second_child_updates(&typing_updates())
            .unwrap()
    };
    intent
        .prepare(&source, &witness, now)
        .unwrap()
        .checkpoint()
        .unwrap()
}
pub(in crate::canvas::text) fn matches(
    checkpoint: &super_platinum_core::canvas::edit::structure::recovery::Checkpoint,
) -> bool {
    let Ok(value) = serde_json::to_value(checkpoint) else {
        return false;
    };
    let source = snapshot(&super::super::structure_fixture::snapshot());
    let typed = typing() != "none";
    let action = &value["action"][if typed {
        "EndRepeatTyping"
    } else {
        "EndRepeat"
    }];
    let reports = typing_updates()
        .iter()
        .map(|d| d.rtml().unwrap())
        .collect::<Vec<_>>();
    value["version"] == (if typed { 13 } else { 12 })
        && action["reused"] == is_reuse()
        && (!typed || action["second_child_updates"] == serde_json::json!(reports))
        && value["sections"].as_array().is_some_and(|rows| {
            rows.len() == source.sections.len()
                && source.sections.iter().all(|s| {
                    rows.iter().any(|r| {
                        serde_json::from_value::<Vec<u8>>(r.clone()).ok().as_deref()
                            == Some(s.raw().raw())
                    })
                })
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use super_platinum_core::canvas::edit::{structure::EndWitness, text::TextDraft};
    #[test]
    fn repeat_reference_fixture_histories_prepare_three_distinct_sections() {
        let previous = super::super::super::structure_fixture::snapshot();
        for mode in ["bootstrap", "retained-positive", "retained-zero"] {
            for corpus in [&ASCII_REPEAT, &UNICODE_REPEAT] {
                for placement in [
                    "no-following",
                    "text-following",
                    "empty-final-following",
                    "empty-nonfinal-following",
                ] {
                    let source = snapshot_with(&previous, mode, placement, corpus);
                    let id = if mode == "retained-zero" {
                        "temp:C:Dte0000000000000000000000001"
                    } else {
                        "paragraph"
                    };
                    let original = source.section(id).unwrap();
                    assert_eq!(
                        original.raw().message(38).unwrap().is_none(),
                        mode == "bootstrap"
                    );
                    let intent = EndIntent::capture_repeat(&source, id, 1_000_000).unwrap();
                    let witness = EndWitness::repeat(
                        intent.seed(),
                        corpus.offset,
                        intent.seed(),
                        &TextDraft::default(),
                        &TextDraft::default(),
                    )
                    .unwrap();
                    let prepared = intent.prepare(&source, &witness, 1_000_000).unwrap();
                    assert!(prepared.is_repeat());
                    assert_eq!(
                        prepared.reuses_child(),
                        placement == "empty-final-following"
                    );
                    assert_ne!(
                        prepared.child_section(),
                        prepared.second_child_section().unwrap()
                    );
                    assert_ne!(
                        prepared.target_section(),
                        prepared.second_child_section().unwrap()
                    );
                    assert_eq!(
                        prepared
                            .document_data()
                            .fields()
                            .iter()
                            .filter(|f| f.tag() == 1)
                            .count(),
                        3
                    );
                    let checkpoint = prepared.checkpoint().unwrap();
                    assert!(checkpoint.is_repeat());
                    assert_eq!(checkpoint.draft_rtml(), corpus.seed_rtml);
                }
            }
        }
    }
    #[test]
    fn repeat_typing_fixture_keeps_first_empty_and_all_third_reports() {
        let previous = super::super::super::structure_fixture::snapshot();
        for mode in ["bootstrap", "retained-positive", "retained-zero"] {
            for corpus in [&ASCII_REPEAT, &UNICODE_REPEAT] {
                for placement in ["no-following", "empty-final-following"] {
                    for cadence in ["correction", "erase", "restore"] {
                        let source = snapshot_with(&previous, mode, placement, corpus);
                        let id = if mode == "retained-zero" {
                            "temp:C:Dte0000000000000000000000001"
                        } else {
                            "paragraph"
                        };
                        let intent = EndIntent::capture_repeat(&source, id, 1_000_000).unwrap();
                        let reports = empty::reports(cadence, corpus.name == "unicode")
                            .iter()
                            .map(|s| TextDraft::from_rtml(&typing::escaped(s)).unwrap())
                            .collect::<Vec<_>>();
                        let witness = EndWitness::repeat(
                            intent.seed(),
                            corpus.offset,
                            intent.seed(),
                            &TextDraft::default(),
                            &TextDraft::default(),
                        )
                        .unwrap()
                        .with_second_child_updates(&reports)
                        .unwrap();
                        let prepared = intent.prepare(&source, &witness, 1_000_000).unwrap();
                        assert!(prepared.is_repeat_typing());
                        assert!(prepared.child_draft().rtml().unwrap().is_empty());
                        assert!(prepared.second_child_updates() == reports.as_slice());
                        assert!(prepared.second_child_draft() == reports.last());
                        let checkpoint = prepared.checkpoint().unwrap();
                        assert!(checkpoint.is_repeat_typing());
                        assert_eq!(serde_json::to_value(checkpoint).unwrap()["version"], 13);
                    }
                }
            }
        }
    }
}
