//! Actual trusted controller captures; creation order differs from visual order.
use super::*;
use serde_json::json;

fn cases() -> Vec<Value> {
    serde_json::from_str(include_str!("split_fixture/second-middle-native.json")).unwrap()
}
fn captured(value: Value) -> Option<Captured> {
    super::super::super::parse(serde_json::from_value(value).unwrap(), 1, 0)
}
fn groups(value: &Value) -> Vec<Vec<String>> {
    serde_json::from_value(value.clone()).unwrap()
}
fn rows(witness: &SplitWitness) -> Vec<String> {
    witness.member_drafts().into_iter().map(text).collect()
}
#[test]
fn actual_second_middle_keeps_creation_identity_and_ordered_histories() {
    let cases = cases();
    assert_eq!(cases.len(), 8);
    for case in cases {
        let Some(Captured::Split(witness)) = captured(case["capture"].clone()) else {
            panic!("second split refused: {}", case["name"])
        };
        let expected: Vec<String> =
            serde_json::from_value(case["expectedFinalTexts"].clone()).unwrap();
        let order: Vec<usize> =
            serde_json::from_value(case["expectedVisualOrder"].clone()).unwrap();
        assert_eq!(rows(&witness), expected, "{}", case["name"]);
        assert_eq!(witness.visual_order(), order);
        assert_eq!(
            vec![
                witness
                    .parent_updates()
                    .iter()
                    .map(text)
                    .collect::<Vec<_>>(),
                witness.child_updates().iter().map(text).collect::<Vec<_>>()
            ],
            groups(&case["expectedBetweenReports"])
        );
        assert_eq!(
            witness
                .final_updates()
                .unwrap()
                .iter()
                .map(|r| r.iter().map(text).collect::<Vec<_>>())
                .collect::<Vec<_>>(),
            groups(&case["expectedFinalReports"])
        );
        assert!(witness.pre_enter_updates().is_empty());
        let event = case["capture"]["split"]["sequence"]["events"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["kind"] == "second-middle")
            .unwrap();
        assert_eq!(
            witness.second_offset_utf16(),
            Some(event["start"].as_u64().unwrap() as usize)
        );
        assert_eq!(
            text(witness.second_enter_draft().unwrap()),
            event["beforeText"]
        );
        assert_eq!(
            text(witness.second_initial_child_draft().unwrap()),
            event["afterText"]
        );
        assert_eq!(
            witness.second_split_target(),
            Some(if order == [0, 2, 1] {
                SplitSequenceTarget::Original
            } else {
                SplitSequenceTarget::FirstChild
            })
        );
        let offset = event["start"].as_u64().unwrap() as usize;
        let live = text(witness.second_enter_draft().unwrap());
        assert_eq!(
            text(witness.second_initial_parent_draft().unwrap()),
            &live[..super::super::offset(&live, offset).unwrap()]
        );
    }
}

#[test]
fn second_middle_refuses_envelope_boundary_and_chronology_forgery() {
    for case in cases() {
        let original = case["capture"].clone();
        let boundary = original["split"]["sequence"]["events"]
            .as_array()
            .unwrap()
            .iter()
            .position(|e| e["kind"] == "second-middle")
            .unwrap();
        for defect in 0..28 {
            let mut bad = original.clone();
            match defect {
                0 => bad["split"]["offset"] = json!(0),
                1 => bad["split"]["revision"] = json!(0),
                2 => bad["split"]["before"] = bad["blocks"].clone(),
                3 => bad["split"]["after"] = bad["blocks"].clone(),
                4 => bad["split"]["rawAfter"] = bad["blocks"].clone(),
                5 => bad["split"]["kind"] = json!("middle-sequence"),
                6 => bad["split"]["sequence"]["events"][boundary]["start"] = json!(0),
                7 => bad["split"]["sequence"]["events"][boundary]["activeIndex"] = json!(0),
                8 => {
                    bad["split"]["sequence"]["events"][boundary]["beforeInputTrusted"] =
                        json!(false)
                }
                9 => bad["split"]["sequence"]["events"][boundary]["inputTrusted"] = json!(false),
                10 => bad["split"]["sequence"]["events"][boundary]["revision"] = json!(999),
                11 => bad["split"]["sequence"]["events"][boundary]["data"] = json!("x"),
                12 => bad["split"]["sequence"]["events"][boundary]["caret"] = json!(1),
                13 => {
                    bad["split"]["sequence"]["events"][boundary]["beforeText"] =
                        json!("Unreported target")
                }
                14 => {
                    bad["split"]["sequence"]["events"][boundary]["afterText"] =
                        json!("Unreported child")
                }
                15 => {
                    bad["split"]["sequence"]["events"][boundary]["sections"]
                        .as_array_mut()
                        .unwrap()
                        .remove(0);
                }
                16 => bad["split"]["sequence"]["events"][boundary]["selectionEnd"] = json!(999),
                17 => bad["split"]["sequence"]["events"][boundary]["kind"] = json!("middle"),
                18 => bad["split"]["generation"] = json!("2"),
                19 => bad["split"]["visit"] = json!("1"),
                20 => bad["split"]["sequence"]["rawAfter"] = json!([]),
                21 => bad["blocks"] = original["split"]["after"].clone(),
                22 => bad["split"]["sequence"]["html"] = json!(""),
                23 => {
                    bad["split"]["sequence"]["events"][0]["beforeText"] =
                        json!("Substituted original")
                }
                24 => {
                    bad["split"]["sequence"]["events"][boundary]["inputType"] =
                        json!("insertFromPaste")
                }
                25 => {
                    bad["split"]["sequence"]["events"]
                        .as_array_mut()
                        .unwrap()
                        .truncate(boundary);
                }
                26 => {
                    let event = bad["split"]["sequence"]["events"][boundary].clone();
                    bad["split"]["sequence"]["events"]
                        .as_array_mut()
                        .unwrap()
                        .push(event);
                }
                27 => bad["split"]["sequence"]["events"][boundary]["rawAfter"][0]["elements"]
                    .as_array_mut()
                    .unwrap()
                    .swap(0, 2),
                _ => unreachable!(),
            }
            assert!(captured(bad).is_none(), "{} defect {defect}", case["name"]);
        }
    }
}

fn change_row(event: &mut Value, index: usize, value: &str) {
    let html = format!("<span>{value}</span>");
    event["sections"][index] = json!({"html":html,"text":value,"placeholder":null});
    for key in ["after", "rawAfter"] {
        event[key][0]["elements"][index] =
            json!({"type":"rich_text_section","elements":[{"type":"text","text":value}]});
    }
    event["html"] = json!(format!(
        "<div class=\"rich-editor-block\">{}</div>",
        event["sections"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| format!(
                "<div class=\"rich-editor-section\">{}</div>",
                s["html"].as_str().unwrap()
            ))
            .collect::<String>()
    ));
}
fn final_snapshot(capture: &mut Value) {
    let last = capture["split"]["sequence"]["events"]
        .as_array()
        .unwrap()
        .last()
        .unwrap()
        .clone();
    for key in ["after", "rawAfter", "html", "revision"] {
        capture["split"]["sequence"][key] = last[key].clone();
    }
    capture["blocks"] = last["after"].clone();
}
#[test]
fn second_middle_refuses_consistent_non_target_mutation_and_deleted_reports() {
    for case in cases() {
        let original = case["capture"].clone();
        let boundary = original["split"]["sequence"]["events"]
            .as_array()
            .unwrap()
            .iter()
            .position(|e| e["kind"] == "second-middle")
            .unwrap();
        let target = original["split"]["sequence"]["events"][boundary]["activeIndex"]
            .as_u64()
            .unwrap() as usize
            - 1;
        let mut bad = original.clone();
        change_row(
            &mut bad["split"]["sequence"]["events"][boundary],
            if target == 0 { 2 } else { 0 },
            "Unreported other row",
        );
        final_snapshot(&mut bad);
        assert!(captured(bad).is_none(), "non-target {}", case["name"]);
        if boundary > 1 {
            let mut bad = original.clone();
            let events = bad["split"]["sequence"]["events"].as_array_mut().unwrap();
            events.remove(1);
            for (index, event) in events.iter_mut().enumerate() {
                event["beforeRevision"] = json!(index);
                event["revision"] = json!(index + 1);
            }
            final_snapshot(&mut bad);
            assert!(captured(bad).is_none(), "missing report {}", case["name"]);
            let mut bad = original.clone();
            let last = bad["split"]["sequence"]["events"]
                .as_array_mut()
                .unwrap()
                .last_mut()
                .unwrap();
            let active = last["activeIndex"].as_u64().unwrap() as usize;
            change_row(last, (active + 1) % 3, "Unreported post-split row");
            final_snapshot(&mut bad);
            assert!(captured(bad).is_none(), "final non-target {}", case["name"]);
        }
    }
}
