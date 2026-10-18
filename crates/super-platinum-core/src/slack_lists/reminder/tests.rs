use super::*;
use serde_json::json;
fn fixture(saved: Option<Value>) -> (Document, RecordsPage) {
    let doc=Document::parse(json!({"id":"F_TASK","mode":"list","access":"read","list_metadata":{"schema":[{"id":"C","key":"title","name":"Name","type":"text"}],"views":[{"id":"V","columns":["C"]}]}})).unwrap();
    let mut row = json!({"id":"ROW","list_id":"F_TASK","date_created":1,"created_by":"U00000001","fields":[{"key":"title","text":"Keep this"}]});
    if let Some(saved) = saved {
        row["saved"] = saved;
    }
    (
        doc,
        RecordsPage::parse(
            json!({"ok":true,"records":[row],"response_metadata":{"next_cursor":"keep"}}),
        )
        .unwrap(),
    )
}
fn item(todo: &str) -> SavedItem {
    serde_json::from_value(json!({"item_type":"list_record","item_id":"F_TASK","ts":"ROW","date_created":100,"date_due":200,"date_completed":0,"is_archived":false,"state":"in_progress","todo_state":todo})).unwrap()
}
fn compact(i: &SavedItem) -> Value {
    json!({"is_archived":i.is_archived,"state":i.state,"date_due":i.date_due,"date_completed":i.date_completed,"todo_state":i.todo_state})
}
#[test]
fn new_reminder_uses_saved_identity_not_date_cell_and_does_not_change_rows() {
    let (d, p) = fixture(None);
    let before = p.clone();
    let e =
        ListReminderEdit::capture(&d, &p, "ROW", true, None, ListReminderChange::Set(300)).unwrap();
    assert_eq!(e.method(), "saved.add");
    assert_eq!(
        e.fields(),
        vec![
            ("item_type", "list_record".into()),
            ("item_id", "F_TASK".into()),
            ("ts", "ROW".into()),
            ("date_due", "300".into()),
            ("_x_reason", "saved-api/saveListRecord".into())
        ]
    );
    assert_eq!(p, before);
    assert!(!e.readback_matches(&d, &p));
    assert!(e.revalidate(&d, &p, true, None).is_ok());
    assert!(
        ListReminderEdit::capture(&d, &p, "ROW", false, None, ListReminderChange::Set(300))
            .is_err()
    );
}
#[test]
fn clear_to_do_uses_creation_sentinel_while_clear_saved_removes_membership() {
    for todo in ["to_do", "saved"] {
        let i = item(todo);
        let (d, p) = fixture(Some(compact(&i)));
        let e = ListReminderEdit::capture(&d, &p, "ROW", true, Some(&i), ListReminderChange::Clear)
            .unwrap();
        if todo == "to_do" {
            assert_eq!(e.method(), "saved.update");
            assert!(e.fields().contains(&("date_due", "100".into())));
            let mut after = i.clone();
            after.date_due = 100;
            let (_, p2) = fixture(Some(compact(&after)));
            assert!(e.readback_matches(&d, &p2));
            assert!(!e.readback_matches(&d, &fixture(None).1));
        } else {
            assert_eq!(e.method(), "saved.delete");
            assert!(!e.fields().iter().any(|(k, _)| *k == "date_due"));
            assert!(e.readback_matches(&d, &fixture(None).1));
        }
    }
}
#[test]
fn reset_reminder_restores_archived_or_completed_and_requires_matching_full_saved_item() {
    for archived in [false, true] {
        let mut i = item("saved");
        i.is_archived = archived;
        i.date_completed = 150;
        let (d, p) = fixture(Some(compact(&i)));
        let e =
            ListReminderEdit::capture(&d, &p, "ROW", true, Some(&i), ListReminderChange::Set(300))
                .unwrap();
        assert!(e.fields().contains(&("mark", "uncompleted".into())));
        assert!(
            ListReminderEdit::capture(&d, &p, "ROW", true, None, ListReminderChange::Clear)
                .is_err()
        );
        let mut wrong = i.clone();
        wrong.ts = "OTHER".into();
        assert!(e.revalidate(&d, &p, true, Some(&wrong)).is_err());
    }
}
#[test]
fn malformed_marker_and_partial_lookup_do_not_mean_absence_or_clear() {
    for marker in [
        json!(null),
        json!({}),
        json!([]),
        json!({"is_archived":false,"state":"in_progress","date_due":false}),
    ] {
        let (d, p) = fixture(Some(marker));
        assert!(
            ListReminderEdit::capture(&d, &p, "ROW", true, None, ListReminderChange::Set(300))
                .is_err()
        );
    }
    let (d, p) = fixture(None);
    assert!(
        ListReminderEdit::capture(
            &d,
            &p,
            "ROW",
            true,
            Some(&item("saved")),
            ListReminderChange::Set(300)
        )
        .is_err()
    );
    for due in [0, -1, 253_402_300_800] {
        assert!(
            ListReminderEdit::capture(&d, &p, "ROW", true, None, ListReminderChange::Set(due))
                .is_err()
        );
    }
}
#[test]
fn exact_raw_preflight_and_known_readback_preserve_cursors_and_guard_replaced_row() {
    let i = item("saved");
    let (d, p) = fixture(Some(compact(&i)));
    let e = ListReminderEdit::capture(&d, &p, "ROW", true, Some(&i), ListReminderChange::Set(300))
        .unwrap();
    let mut changed = p.clone();
    changed.records[0].raw["future"] = json!(true);
    assert!(e.revalidate(&d, &changed, true, Some(&i)).is_err());
    let mut after = i.clone();
    after.date_due = 300;
    let (_, mut p2) = fixture(Some(compact(&after)));
    assert!(e.readback_matches(&d, &p2));
    p2.records[0].raw["date_created"] = json!(2);
    assert!(!e.readback_matches(&d, &p2));
    assert_eq!(p.raw["response_metadata"]["next_cursor"], "keep");
}

#[test]
fn preflight_rejects_saved_membership_aba_and_full_observation_changes() {
    let observed = item("saved");
    let (document, page) = fixture(Some(compact(&observed)));
    for change in [ListReminderChange::Set(300), ListReminderChange::Clear] {
        let intent =
            ListReminderEdit::capture(&document, &page, "ROW", true, Some(&observed), change)
                .unwrap();
        assert!(
            intent
                .revalidate(&document, &page, true, Some(&observed))
                .is_ok()
        );
        let mut recreated = observed.clone();
        recreated.date_created += 1;
        let mut updated = observed.clone();
        updated.date_updated += 1;
        let mut future = observed.clone();
        future
            .extra
            .insert("future_membership_revision".into(), json!({"version": 2}));
        for newer in [recreated, updated, future] {
            assert_eq!(compact(&newer), compact(&observed));
            // Target identity and derived request are unchanged: only the full
            // saved observation distinguishes this old intent from a new one.
            let fresh =
                ListReminderEdit::capture(&document, &page, "ROW", true, Some(&newer), change)
                    .unwrap();
            assert_eq!(fresh.fields(), intent.fields());
            assert!(
                intent
                    .revalidate(&document, &page, true, Some(&newer))
                    .is_err()
            );
        }
    }
}
