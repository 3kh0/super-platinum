use super::*;
use crate::slack_lists::rename::sort as authoring;
fn add_field(s: &mut ShellState, kind: &str) {
    let loaded = s
        .slack_lists
        .view
        .as_mut()
        .unwrap()
        .loaded
        .as_mut()
        .unwrap();
    loaded.document.raw["list_metadata"]["schema"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"C_EXTRA","key":"extra","name":"Extra field","type":kind}));
    loaded.document = Document::parse(loaded.document.raw.clone()).unwrap();
    rebuild_rows(s);
}
fn select(s: &ShellState, column: &str, ascending: bool) -> bool {
    let e = entry(s).unwrap();
    let Input::Sort(rendered) = e.input else {
        panic!()
    };
    authoring::change_column(
        s,
        e.serial,
        generation(s).unwrap(),
        e.sort_choice,
        &rendered,
        column,
        ascending,
    )
}
#[test]
fn supported_field_events_capture_exact_wire_and_require_canonical_readback() {
    for kind in [
        "text",
        "select",
        "checkbox",
        "date",
        "created_time",
        "last_edited_time",
        "user",
        "channel",
        "rating",
        "email",
        "phone",
    ] {
        let (mut s, _cleanup) = sort_shell();
        add_field(&mut s, kind);
        open(&mut s);
        let before = metadata(&s, None).document;
        assert!(
            presentation(&s)
                .unwrap()
                .sort
                .unwrap()
                .choices
                .iter()
                .any(|(id, _)| id == "C_EXTRA")
        );
        assert!(select(&s, "C_EXTRA", false));
        let e = entry(&s).unwrap();
        assert_eq!(
            e.input,
            Input::Sort(ViewSortChoice::Field {
                column_id: "C_EXTRA".into(),
                ascending: false
            })
        );
        let (g, i) = prepare(&mut s, e.serial).unwrap();
        let Intent::Sort(intent) = &i else { panic!() };
        assert_eq!(intent.sorts(), &json!([{"key":"extra","ascending":false}]));
        assert!(admitted(&s, &g, &i, &metadata(&s, None)).unwrap().is_sort());
        completed(&mut s, &g, Ok(json!({"ok":true})));
        assert_eq!(metadata(&s, None).document, before);
        assert_eq!(entry(&s).unwrap().stage, Stage::Uncertain);
        assert!(prepare(&mut s, e.serial).is_none());
        let check = checking(&s);
        let mut fresh = metadata(&s, None);
        fresh.document.raw["list_metadata"]["views"][0]["sorts"] = intent.sorts().clone();
        fresh.document = Document::parse(fresh.document.raw).unwrap();
        assert!(checked(&mut s, &check, Ok(fresh)));
        assert_eq!(entry(&s).unwrap().stage, Stage::Observed);
        assert!(close_entry(&s, e.serial));
        open(&mut s);
        assert_eq!(
            presentation(&s).unwrap().sort.unwrap().original,
            ViewSortChoice::Field {
                column_id: "C_EXTRA".into(),
                ascending: false
            }
        );
        assert!(select(&s, "", true));
        let serial = entry(&s).unwrap().serial;
        let (_, intent) = prepare(&mut s, serial).unwrap();
        let Intent::Sort(intent) = intent else {
            panic!()
        };
        assert_eq!(intent.sorts(), &json!([]));
    }
}
#[test]
fn new_field_callbacks_preserve_owner_choice_aba_and_cancel() {
    let (mut s, _cleanup) = sort_shell();
    add_field(&mut s, "user");
    open(&mut s);
    let before = s.slack_lists.view.as_ref().unwrap().loaded.clone().unwrap();
    assert!(select(&s, "C_EXTRA", true));
    let old = entry(&s).unwrap();
    let Input::Sort(rendered) = old.input.clone() else {
        panic!()
    };
    assert!(select(&s, "C_N", true));
    assert!(select(&s, "C_EXTRA", true));
    assert!(!authoring::change_column(
        &s,
        old.serial,
        generation(&s).unwrap(),
        old.sort_choice,
        &rendered,
        "C_EXTRA",
        false
    ));
    let e = entry(&s).unwrap();
    let Input::Sort(rendered) = e.input.clone() else {
        panic!()
    };
    assert!(!authoring::change_column(
        &s,
        e.serial,
        generation(&s).unwrap() + 1,
        e.sort_choice,
        &rendered,
        "C_EXTRA",
        false
    ));
    assert!(!select(&s, "deleted", true));
    assert!(close_entry(&s, e.serial));
    let after = s
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .loaded
        .as_ref()
        .unwrap();
    assert_eq!(after.document, before.document);
    assert_eq!(after.records, before.records);
}
#[test]
fn unsupported_fields_and_changed_type_cannot_enter_the_staged_rule() {
    for kind in ["link", "vote", "currency", "assignee", "todo_assignee"] {
        let (mut s, _cleanup) = sort_shell();
        add_field(&mut s, kind);
        open(&mut s);
        assert!(
            !presentation(&s)
                .unwrap()
                .sort
                .unwrap()
                .choices
                .iter()
                .any(|(id, _)| id == "C_EXTRA")
        );
        assert!(!select(&s, "C_EXTRA", true));
    }
    let (mut s, _cleanup) = sort_shell();
    add_field(&mut s, "text");
    open(&mut s);
    let loaded = s
        .slack_lists
        .view
        .as_mut()
        .unwrap()
        .loaded
        .as_mut()
        .unwrap();
    let column = loaded.document.raw["list_metadata"]["schema"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|c| c["id"] == "C_EXTRA")
        .unwrap();
    column["type"] = json!("user");
    loaded.document = Document::parse(loaded.document.raw.clone()).unwrap();
    assert!(!select(&s, "C_EXTRA", true));
}
