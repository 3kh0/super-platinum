//! Bounded sequence fixtures; source sessions are copied from the frozen original reference.
use super::*;
use super_platinum_core::canvas::edit::{structure::EndWitness, text::TextDraft};
pub(in crate::canvas::text) fn count() -> usize {
    std::env::var("SUPER_PLATINUM_CANVAS_SEQUENCE_COUNT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(2)
}
pub(in crate::canvas::text) fn history() -> String {
    std::env::var("SUPER_PLATINUM_CANVAS_SEQUENCE_HISTORY").unwrap_or_else(|_| "bootstrap".into())
}
pub(in crate::canvas::text) fn layout() -> String {
    std::env::var("SUPER_PLATINUM_CANVAS_SEQUENCE_LAYOUT").unwrap_or_else(|_| "no-following".into())
}
pub(in crate::canvas::text) fn cadence() -> String {
    std::env::var("SUPER_PLATINUM_CANVAS_SEQUENCE_CADENCE").unwrap_or_else(|_| "straight".into())
}
pub(in crate::canvas::text) fn valid() -> bool {
    if !is_sequence() {
        return ["COUNT", "HISTORY", "LAYOUT", "CADENCE"]
            .iter()
            .all(|s| std::env::var_os(format!("SUPER_PLATINUM_CANVAS_SEQUENCE_{s}")).is_none());
    }
    (1..=16).contains(&count())
        && matches!(
            history().as_str(),
            "bootstrap" | "retained-positive" | "retained-zero"
        )
        && matches!(
            layout().as_str(),
            "no-following"
                | "text-following"
                | "empty-final-following"
                | "empty-nonfinal-following"
        )
        && matches!(
            cadence().as_str(),
            "straight"
                | "correction-before-enter"
                | "erase-before-enter"
                | "revert-before-enter"
                | "trailing-empty"
        )
        && (count() >= 2 || matches!(cadence().as_str(), "straight" | "trailing-empty"))
        && std::env::var("SUPER_PLATINUM_CANVAS_SEQUENCE_COUNT")
            .map_or(true, |v| (1..=16).any(|n| v == n.to_string()))
        && typing() == "none"
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
pub(super) fn corpus() -> &'static Corpus {
    repeat::corpus()
}
pub(in crate::canvas::text) fn reports_for(
    count: usize,
    cadence: &str,
    unicode: bool,
) -> Vec<Vec<String>> {
    let target = match count {
        8 => 5,
        16 => 14,
        _ => 0,
    };
    (0..count)
        .map(|i| {
            let name = match i {
                0 => "New".to_owned(),
                1 => "Next".to_owned(),
                2 => "Last".to_owned(),
                _ => format!("Row {}", i + 1),
            };
            let prefix = if unicode {
                format!("{name} \u{65e5}\u{672c}\u{1f600} &")
            } else {
                name.clone()
            };
            if i + 1 == count && cadence == "trailing-empty" {
                return Vec::new();
            }
            if i == target {
                match cadence {
                    "correction-before-enter" => {
                        return vec![
                            prefix.clone(),
                            format!("{prefix} draftx"),
                            format!("{prefix} draft"),
                            format!("{prefix} paragraph"),
                        ];
                    }
                    "erase-before-enter" => return vec![prefix.clone(), String::new()],
                    "revert-before-enter" => {
                        return vec![
                            prefix.clone(),
                            format!("{prefix} draftx"),
                            prefix.clone(),
                            String::new(),
                        ];
                    }
                    _ => {}
                }
            }
            vec![if unicode {
                format!("{name} \u{65e5}\u{672c}\u{1f600} & paragraph")
            } else {
                format!("{name} paragraph")
            }]
        })
        .collect()
}
pub(in crate::canvas::text) fn reports() -> Vec<Vec<String>> {
    reports_for(count(), &cadence(), corpus().name == "unicode")
}
pub(in crate::canvas::text) fn updates() -> Vec<Vec<TextDraft>> {
    reports()
        .iter()
        .map(|r| {
            r.iter()
                .map(|s| TextDraft::from_rtml(&typing::escaped(s)).unwrap())
                .collect()
        })
        .collect()
}
pub(in crate::canvas::text) fn final_text(index: usize) -> String {
    reports()
        .get(index)
        .and_then(|r| r.last())
        .cloned()
        .unwrap_or_default()
}
pub(in crate::canvas::text) fn edited(index: usize) -> String {
    format!("Appended paragraph {} edited", index + 1)
}
fn session(mode: &str, corpus: &str, source: &str, id: &str) -> Option<RawMessage> {
    let data: Value = serde_json::from_str(include_str!("sequence-original.json")).unwrap();
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
pub(super) fn snapshot_with(
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
    let source = snapshot(&super::super::structure_fixture::snapshot());
    let now = checklist::save::now_usec().unwrap();
    let intent = EndIntent::capture_sequence(&source, parent(), count(), now).unwrap();
    let witness = EndWitness::sequence(intent.seed(), corpus().offset, &updates()).unwrap();
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
    let reports = updates();
    value["version"] == 14
        && value["action"]["EndSequence"]["members"]
            .as_array()
            .is_some_and(|members| {
                members.len() == count()
                    && members.iter().enumerate().all(|(i, m)| {
                        m["reused"] == (i == 0 && is_reuse())
                            && m["updates"]
                                == json!(
                                    reports[i]
                                        .iter()
                                        .map(|d| d.rtml().unwrap())
                                        .collect::<Vec<_>>()
                                )
                    })
            })
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
    #[test]
    fn reference_cohorts_bind_exact_source_and_ordered_reports() {
        let mut vectors: Vec<Value> =
            serde_json::from_str(include_str!("sequence-reference.json")).unwrap();
        assert_eq!(vectors.len(), 48);
        let long: Vec<Value> =
            serde_json::from_str(include_str!("sequence-long-reference.json")).unwrap();
        assert_eq!(long.len(), 18);
        vectors.extend(long);
        for v in vectors {
            let count = v["appendCount"].as_u64().unwrap() as usize;
            let corpus = if v["corpus"] == "unicode" {
                &Corpus {
                    edited: "Caf\u{e9} & \u{65e5}\u{672c}\u{1f600} edited",
                    edited_rtml: "Caf\u{e9} &amp; \u{65e5}\u{672c}\u{1f600} edited",
                    ..END_UNICODE
                }
            } else {
                &Corpus {
                    edited: "Keep this edited",
                    edited_rtml: "Keep this edited",
                    ..END_ASCII
                }
            };
            let mode = v["mode"].as_str().unwrap();
            let layout = v["layout"].as_str().unwrap();
            let source = snapshot_with(
                &super::super::super::structure_fixture::snapshot(),
                mode,
                layout,
                corpus,
            );
            let parent = if mode == "retained-zero" {
                "temp:C:Dte0000000000000000000000001"
            } else {
                "paragraph"
            };
            let reports = reports_for(
                count,
                v["cadence"].as_str().unwrap(),
                corpus.name == "unicode",
            );
            let updates = reports
                .iter()
                .map(|r| {
                    r.iter()
                        .map(|s| TextDraft::from_rtml(&typing::escaped(s)).unwrap())
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>();
            assert_eq!(
                json!(
                    updates
                        .iter()
                        .map(|r| r.iter().map(|d| d.rtml().unwrap()).collect::<Vec<_>>())
                        .collect::<Vec<_>>()
                ),
                v["updates"]
            );
            let intent = EndIntent::capture_sequence(&source, parent, count, 1_000_000).unwrap();
            let witness = EndWitness::sequence(intent.seed(), corpus.offset, &updates).unwrap();
            let prepared = intent.prepare(&source, &witness, 1_000_000).unwrap();
            assert!(prepared.is_sequence());
            assert_eq!(prepared.appended_members().len(), count);
            for (i, member) in prepared.appended_members().enumerate() {
                assert!(member.updates() == updates[i].as_slice());
                assert_eq!(member.reused(), i == 0 && layout == "empty-final-following");
                assert_eq!(
                    member.draft().rtml().unwrap(),
                    updates[i]
                        .last()
                        .map(|d| d.rtml().unwrap())
                        .unwrap_or_default()
                );
            }
            let checkpoint = serde_json::to_value(prepared.checkpoint().unwrap()).unwrap();
            assert_eq!(checkpoint["version"], 14);
            assert_eq!(
                checkpoint["action"]["EndSequence"]["members"]
                    .as_array()
                    .unwrap()
                    .len(),
                count
            );
        }
    }
}
