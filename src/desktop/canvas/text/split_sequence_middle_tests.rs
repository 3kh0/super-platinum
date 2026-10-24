//! Actual trusted-browser prelude/middle evidence and boundary forgeries.
use super::*;
use serde_json::json;

fn cases() -> Vec<Value> {
    serde_json::from_str(include_str!("split_fixture/premiddle-native.json")).unwrap()
}
fn parse_capture(value: Value) -> Option<Captured> {
    super::super::parse(serde_json::from_value(value).unwrap(), 1, 0)
}
fn strings(value: &Value) -> Vec<String> {
    serde_json::from_value(value.clone()).unwrap()
}
fn blocks(value: &str) -> Value {
    json!([{"type":"rich_text","elements":[{"type":"rich_text_section",
        "elements":[{"type":"text","text":value}]}]}])
}
#[test]
fn browser_pre_middle_preserves_ordered_prefix_and_child_reports() {
    let cases = cases();
    assert_eq!(cases.len(), 4);
    for case in cases {
        let Some(Captured::Split(witness)) = parse_capture(case["capture"].clone()) else {
            panic!("middle refused: {}", case["name"])
        };
        assert_eq!(
            witness
                .pre_enter_updates()
                .iter()
                .map(text)
                .collect::<Vec<_>>(),
            strings(&case["expectedPreEnterReports"])
        );
        assert_eq!(
            witness.child_updates().iter().map(text).collect::<Vec<_>>(),
            strings(&case["expectedChildReports"])
        );
        assert_eq!(text(witness.enter_draft()), case["expectedEnterText"]);
        assert_eq!(
            text(witness.initial_child_draft()),
            case["expectedInitialChild"]
        );
        let child = strings(&case["expectedChildReports"]);
        assert_eq!(
            text(witness.child_draft()),
            child
                .last()
                .map(String::as_str)
                .unwrap_or(case["expectedInitialChild"].as_str().unwrap())
        );
        if case["name"] == "noop" {
            assert!(witness.pre_enter_updates().is_empty());
            assert_eq!(case["capture"]["split"]["revision"], 2);
        }
    }
}
#[test]
fn pre_middle_refuses_original_live_boundary_and_chronology_substitution() {
    for case in cases() {
        let original = case["capture"].clone();
        let middle = original["split"]["sequence"]["events"]
            .as_array()
            .unwrap()
            .iter()
            .position(|e| e["kind"] == "middle")
            .unwrap();
        for variant in 0..13 {
            // Removing a semantic no-op and renumbering produces an allowed
            // direct-middle history; changed prelude history remains mandatory.
            if variant == 11 && case["name"] == "noop" {
                continue;
            }
            let mut bad = original.clone();
            match variant {
                0 => bad["split"]["before"] = blocks("Unobserved original"),
                1 => bad["split"]["before"] = bad["split"]["after"].clone(),
                2 => bad["split"]["offset"] = json!(0),
                3 => bad["split"]["offset"] = json!(10), // Inside the source emoji.
                4 => bad["split"]["revision"] = json!(1),
                5 => bad["split"]["sequence"]["events"][0]["beforeText"] = json!("Forged seed"),
                6 => bad["split"]["sequence"]["events"][0]["data"] = json!("Forged report"),
                7 => {
                    bad["split"]["sequence"]["events"][middle]["beforeText"] =
                        json!("Forged live boundary")
                }
                8 => bad["split"]["sequence"]["events"][middle]["start"] = json!(0),
                9 => bad["split"]["sequence"]["events"][middle]["kind"] = json!("enter"),
                10 => bad["split"]["kind"] = json!("sequence"),
                11 => {
                    let events = bad["split"]["sequence"]["events"].as_array_mut().unwrap();
                    events.remove(0);
                    for (i, event) in events.iter_mut().enumerate() {
                        event["beforeRevision"] = json!(i);
                        event["revision"] = json!(i + 1);
                    }
                    let len = events.len();
                    bad["split"]["revision"] = json!(middle);
                    bad["split"]["sequence"]["revision"] = json!(len);
                }
                12 => bad["blocks"] = bad["split"]["before"].clone(),
                _ => unreachable!(),
            }
            assert!(
                parse_capture(bad).is_none(),
                "{} variant {variant}",
                case["name"]
            );
        }
        if case["name"] == "correction" {
            let mut bad = original;
            bad["split"]["before"] = blocks(case["expectedEnterText"].as_str().unwrap());
            assert!(
                parse_capture(bad).is_none(),
                "live cannot replace original seed"
            );
        }
    }
}
#[test]
fn pre_middle_refuses_consistent_non_target_parent_mutation_or_deletion() {
    for case in cases().into_iter().filter(|c| c["name"] != "noop") {
        for erase in [false, true] {
            let mut bad = case["capture"].clone();
            let sequence = &mut bad["split"]["sequence"];
            let event = sequence["events"]
                .as_array_mut()
                .unwrap()
                .last_mut()
                .unwrap();
            assert_eq!(event["kind"], "edit");
            assert_eq!(event["activeIndex"], 1);
            event["sections"][0] = if erase {
                json!({"html":"<br>","text":"","placeholder":{"html":"<br>","spanDepth":0}})
            } else {
                json!({"html":"Unreported","text":"Unreported","placeholder":null})
            };
            let section = if erase {
                json!({"type":"rich_text_section","elements":[]})
            } else {
                blocks("Unreported")[0]["elements"][0].clone()
            };
            event["after"][0]["elements"][0] = section.clone();
            event["rawAfter"][0]["elements"][0] = section;
            let mut html = String::from("<div class=\"rich-editor-block\">");
            for section in event["sections"].as_array().unwrap() {
                html.push_str("<div class=\"rich-editor-section\">");
                html.push_str(section["html"].as_str().unwrap());
                html.push_str("</div>");
            }
            html.push_str("</div>");
            event["html"] = json!(html);
            let final_event = event.clone();
            for key in ["after", "rawAfter", "html"] {
                sequence[key] = final_event[key].clone();
            }
            bad["blocks"] = final_event["after"].clone();
            assert!(
                parse_capture(bad).is_none(),
                "{} erase {erase}",
                case["name"]
            );
        }
    }
}

fn parent_cases() -> Vec<Value> {
    serde_json::from_str(include_str!("split_fixture/middle-parent-native.json")).unwrap()
}

#[test]
fn browser_middle_parent_keeps_initial_halves_and_independent_ordered_reports() {
    let cases = parent_cases();
    assert_eq!(cases.len(), 4);
    for case in cases {
        let Some(Captured::Split(witness)) = parse_capture(case["capture"].clone()) else {
            panic!("parent-middle refused: {}", case["name"])
        };
        assert_eq!(
            text(witness.initial_parent_draft()),
            case["expectedInitialParent"]
        );
        assert_eq!(
            text(witness.initial_child_draft()),
            case["expectedInitialChild"]
        );
        assert_eq!(text(witness.enter_draft()), case["expectedEnterText"]);
        for (actual, expected) in [
            (
                witness.pre_enter_updates(),
                &case["expectedPreEnterReports"],
            ),
            (witness.parent_updates(), &case["expectedParentReports"]),
            (witness.child_updates(), &case["expectedChildReports"]),
        ] {
            assert_eq!(
                actual.iter().map(text).collect::<Vec<_>>(),
                strings(expected)
            );
        }
        let parents = strings(&case["expectedParentReports"]);
        assert_eq!(text(witness.parent_draft()), *parents.last().unwrap());
        let children = strings(&case["expectedChildReports"]);
        assert_eq!(text(witness.child_draft()), *children.last().unwrap());
        if case["name"] == "none-erase" {
            assert!(witness.parent_draft() == &TextDraft::default());
            assert!(witness.parent_draft() != witness.initial_parent_draft());
        }
        if case["name"] == "none-revert" {
            assert!(witness.parent_draft() == witness.initial_parent_draft());
            assert_eq!(witness.parent_updates().len(), 2);
        }
        if strings(&case["expectedPreEnterReports"]).is_empty() {
            assert_eq!(case["capture"]["split"]["revision"], 1);
        }
    }
}

#[test]
fn middle_parent_refuses_rebased_split_headers_stale_owners_and_missing_history() {
    for case in parent_cases() {
        for variant in 0..9 {
            let mut bad = case["capture"].clone();
            match variant {
                0 => bad["split"]["after"] = bad["blocks"].clone(),
                1 => bad["split"]["before"] = blocks("Different original"),
                2 => bad["split"]["offset"] = json!(10),
                3 => bad["split"]["generation"] = json!("2"),
                4 => bad["split"]["visit"] = json!("1"),
                5 => bad["split"]["sequence"]["events"][0]["beforeInputTrusted"] = json!(false),
                6 => {
                    let events = bad["split"]["sequence"]["events"].as_array_mut().unwrap();
                    let middle = events.iter().position(|e| e["kind"] == "middle").unwrap();
                    let parent = events
                        .iter()
                        .enumerate()
                        .skip(middle + 1)
                        .find(|(_, e)| e["activeIndex"] == 0)
                        .unwrap()
                        .0;
                    events.remove(parent);
                }
                7 => {
                    let event = bad["split"]["sequence"]["events"]
                        .as_array_mut()
                        .unwrap()
                        .last_mut()
                        .unwrap();
                    event["kind"] = json!("middle");
                    event["inputType"] = json!("insertParagraph");
                }
                8 => bad["split"]["sequence"]["rawAfter"] = json!([]),
                _ => unreachable!(),
            }
            assert!(
                parse_capture(bad).is_none(),
                "{} variant {variant}",
                case["name"]
            );
        }
    }
}

#[test]
fn middle_parent_refuses_consistent_unreported_opposite_row_mutations() {
    for case in parent_cases() {
        // Check both directions: a parent input cannot mutate the child, and a
        // child input cannot mutate the parent, even with consistent snapshots.
        for target in [0, 1] {
            let mut bad = case["capture"].clone();
            let events = bad["split"]["sequence"]["events"].as_array_mut().unwrap();
            let middle = events.iter().position(|e| e["kind"] == "middle").unwrap();
            let event = events
                .iter_mut()
                .skip(middle + 1)
                .find(|e| e["kind"] == "edit" && e["activeIndex"] == target)
                .unwrap();
            let victim = 1 - target;
            event["sections"][victim] =
                json!({"html":"Unobserved", "text":"Unobserved", "placeholder":null});
            let changed = blocks("Unobserved")[0]["elements"][0].clone();
            event["after"][0]["elements"][victim] = changed.clone();
            event["rawAfter"][0]["elements"][victim] = changed;
            let html = event["sections"]
                .as_array()
                .unwrap()
                .iter()
                .map(|s| {
                    format!(
                        "<div class=\"rich-editor-section\">{}</div>",
                        s["html"].as_str().unwrap()
                    )
                })
                .collect::<String>();
            event["html"] = json!(format!("<div class=\"rich-editor-block\">{html}</div>"));
            assert!(
                parse_capture(bad).is_none(),
                "{} target {target}",
                case["name"]
            );
        }
    }
}
