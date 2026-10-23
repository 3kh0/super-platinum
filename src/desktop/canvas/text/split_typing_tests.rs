use super::*;
use serde_json::json;

fn blocks(child: &str) -> Vec<Value> {
    vec![json!({"type":"rich_text","elements":[
        {"type":"rich_text_section","elements":[{"type":"text","text":"Keep this"}]},
        {"type":"rich_text_section","elements":if child.is_empty(){json!([])}else{json!([{"type":"text","text":child}])}}
    ]})]
}
fn entry(
    before: &str,
    after: &str,
    kind: &str,
    data: Option<&str>,
    target: (usize, usize),
    selected: (usize, usize),
    index: u64,
) -> Value {
    let empty = after.is_empty();
    let mut raw = blocks(after);
    if empty {
        raw[0]["elements"][1]["elements"] = json!([{"type":"text","text":"\n"}]);
    }
    json!({"inputType":kind,"data":data,"start":target.0,"end":target.1,
        "selectionStart":selected.0,"selectionEnd":selected.1,"beforeText":before,"afterText":after,
        "beforeRevision":index+1,"revision":index+2,"caret":target.0+data.unwrap_or("").encode_utf16().count(),
        "beforeInputTrusted":true,"inputTrusted":true,"after":blocks(after),"rawAfter":raw,
        "html":format!("<div class=\"rich-editor-block\"><div class=\"rich-editor-section\"><span>Keep this</span></div><div class=\"rich-editor-section\"><span>{}</span></div></div>",if empty {"<br>"}else{after}),
        "placeholder":if empty {json!({"blockCount":1,"sectionCount":2,"childHtml":"<span><br></span>","spanDepth":1,"caretOffset":0})} else {Value::Null}})
}
fn wire(events: Vec<Value>) -> Value {
    let last = events.last().unwrap();
    json!({"revision":last["revision"],"after":last["after"],"rawAfter":last["rawAfter"],
        "html":last["html"],"placeholder":last["placeholder"],"events":events})
}
fn correction() -> Value {
    wire(vec![
        entry("", "New", "insertText", Some("New"), (0, 0), (0, 0), 0),
        entry(
            "New",
            "New draftx",
            "insertText",
            Some(" draftx"),
            (3, 3),
            (3, 3),
            1,
        ),
        entry(
            "New draftx",
            "New draft",
            "deleteContentBackward",
            None,
            (9, 10),
            (10, 10),
            2,
        ),
        entry(
            "New draft",
            "New paragraph",
            "insertText",
            Some("paragraph"),
            (4, 9),
            (4, 9),
            3,
        ),
    ])
}
fn validate_wire(value: Value) -> Option<Vec<TextDraft>> {
    validate(
        &serde_json::from_value(value).ok()?,
        &TextDraft::from_rtml("Keep this").unwrap(),
    )
}
#[test]
fn native_correction_keeps_all_changed_reports_in_order() {
    let reports = validate_wire(correction()).unwrap();
    assert_eq!(
        reports
            .iter()
            .map(|d| d.rtml().unwrap())
            .collect::<Vec<_>>(),
        ["New", "New draftx", "New draft", "New paragraph"]
    );
}
#[test]
fn selection_target_revision_and_final_capture_are_bound() {
    for (field, value) in [
        ("start", json!(3)),
        ("selectionEnd", json!(8)),
        ("beforeText", json!("Other")),
        ("inputTrusted", json!(false)),
        ("beforeInputTrusted", json!(false)),
        ("beforeRevision", json!(99)),
        ("revision", json!(9)),
        ("caret", json!(0)),
        ("inputType", json!("insertFromPaste")),
        ("data", json!("something else")),
    ] {
        let mut changed = correction();
        changed["events"][3][field] = value;
        assert!(validate_wire(changed).is_none(), "{field}");
    }
    let mut changed = correction();
    changed["after"] = json!(blocks("New draft"));
    assert!(validate_wire(changed).is_none());
    let mut changed = correction();
    changed["events"][2]["after"][0]["elements"][0]["elements"][0]["text"] = json!("Other parent");
    assert!(validate_wire(changed).is_none());
}
#[test]
fn erased_text_retains_history_and_requires_exact_empty_projection() {
    let erased = wire(vec![
        entry("", "x", "insertText", Some("x"), (0, 0), (0, 0), 0),
        entry("x", "", "deleteContentBackward", None, (0, 1), (1, 1), 1),
    ]);
    let reports = validate_wire(erased.clone()).unwrap();
    assert_eq!(
        reports
            .iter()
            .map(|d| d.rtml().unwrap())
            .collect::<Vec<_>>(),
        ["x", ""]
    );
    let mut changed = erased;
    changed["events"][1]["placeholder"]["childHtml"] = json!("<span><br><br></span>");
    assert!(validate_wire(changed).is_none());
}
#[test]
fn unicode_target_never_slices_a_surrogate_and_forward_delete_is_local() {
    let good = wire(vec![
        entry("", "a😀b", "insertText", Some("a😀b"), (0, 0), (0, 0), 0),
        entry(
            "a😀b",
            "ab",
            "deleteContentForward",
            None,
            (1, 3),
            (1, 1),
            1,
        ),
    ]);
    assert!(validate_wire(good.clone()).is_some());
    let mut bad = good;
    bad["events"][1]["end"] = json!(2);
    assert!(validate_wire(bad).is_none());
}
#[test]
fn adjacent_noop_does_not_inflate_reports_but_revisits_do() {
    let sequence = wire(vec![
        entry("", "a", "insertText", Some("a"), (0, 0), (0, 0), 0),
        entry("a", "a", "insertText", Some("a"), (0, 1), (0, 1), 1),
        entry("a", "ab", "insertText", Some("b"), (1, 1), (1, 1), 2),
        entry("ab", "a", "deleteContentBackward", None, (1, 2), (2, 2), 3),
    ]);
    let reports = validate_wire(sequence).unwrap();
    assert_eq!(
        reports
            .iter()
            .map(|d| d.rtml().unwrap())
            .collect::<Vec<_>>(),
        ["a", "ab", "a"]
    );
}
#[test]
fn excessive_native_ledger_is_rejected_before_history_authoring() {
    let event = entry("", "a", "insertText", Some("a"), (0, 0), (0, 0), 0);
    assert!(validate_wire(wire(vec![event; 4097])).is_none());
}

#[test]
fn middle_typing_starts_from_retained_tail_and_keeps_empty_restore_history() {
    let initial = TextDraft::from_rtml("Second paragraph").unwrap();
    let parent = TextDraft::from_rtml("Keep this").unwrap();
    for (events, expected) in [
        (
            vec![entry(
                "Second paragraph",
                "New",
                "insertText",
                Some("New"),
                (0, 16),
                (0, 16),
                0,
            )],
            vec!["New"],
        ),
        (
            vec![
                entry(
                    "Second paragraph",
                    "Second paragraphx",
                    "insertText",
                    Some("x"),
                    (16, 16),
                    (16, 16),
                    0,
                ),
                entry(
                    "Second paragraphx",
                    "",
                    "deleteContentBackward",
                    None,
                    (0, 17),
                    (0, 17),
                    1,
                ),
            ],
            vec!["Second paragraphx", ""],
        ),
        (
            vec![
                entry(
                    "Second paragraph",
                    "Second paragraphx",
                    "insertText",
                    Some("x"),
                    (16, 16),
                    (16, 16),
                    0,
                ),
                entry(
                    "Second paragraphx",
                    "Second paragraph",
                    "deleteContentBackward",
                    None,
                    (16, 17),
                    (17, 17),
                    1,
                ),
            ],
            vec!["Second paragraphx", "Second paragraph"],
        ),
        (
            vec![entry(
                "Second paragraph",
                "Second paragraph",
                "insertText",
                Some("Second paragraph"),
                (0, 16),
                (0, 16),
                0,
            )],
            vec![],
        ),
    ] {
        let value: Typing = serde_json::from_value(wire(events)).unwrap();
        let reports = validate_from(&value, &parent, &initial).unwrap();
        assert_eq!(
            reports
                .iter()
                .map(|d| d.rtml().unwrap())
                .collect::<Vec<_>>(),
            expected
        );
        assert!(validate(&value, &parent).is_none());
        assert!(
            validate_from(
                &value,
                &parent,
                &TextDraft::from_rtml("Other tail").unwrap()
            )
            .is_none()
        );
    }
}
