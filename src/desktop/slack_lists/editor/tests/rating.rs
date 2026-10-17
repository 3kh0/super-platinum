use super::*;
use super_platinum_core::slack_lists::RatingOptions;
fn rating_shell(value: Option<u8>, max: u8) -> ShellState {
    let mut s = shell();
    let data = s
        .slack_lists
        .view
        .as_mut()
        .unwrap()
        .loaded
        .as_mut()
        .unwrap();
    let mut doc = data.document.raw.clone();
    doc["list_metadata"]["schema"].as_array_mut().unwrap().push(json!({"id":"rating","key":"rating","name":"Quality","type":"rating","options":RatingOptions::capture(":heart:",max,None).unwrap().wire()}));
    data.document = Document::parse(doc).unwrap();
    if let Some(n) = value {
        let mut record = data.records.records[0].raw.clone();
        record["fields"]
            .as_array_mut()
            .unwrap()
            .push(json!({"key":"rating","rating":[n]}));
        data.records = RecordsPage::parse(json!({"ok":true,"records":[record]})).unwrap();
    }
    super::super::super::rebuild_rows(&mut s);
    s
}
fn stored(mut data: Loaded, value: Option<u8>) -> Loaded {
    let mut row = data.records.records[0].raw.clone();
    row["fields"]
        .as_array_mut()
        .unwrap()
        .retain(|v| v["key"] != "rating");
    row["fields"]
        .as_array_mut()
        .unwrap()
        .push(json!({"key":"rating","rating":value.into_iter().collect::<Vec<_>>()}));
    data.records = RecordsPage::parse(json!({"ok":true,"records":[row]})).unwrap();
    data
}
#[test]
fn rating_first_set_ack_only_readback_and_explicit_clear_use_existing_worker_contract() {
    for max in 1..=5 {
        let mut s = rating_shell(None, max);
        let data = loaded(&s);
        let generation = open(&mut s, "rating");
        let view = presentation(&s).unwrap();
        assert_eq!(view.kind, EditorKind::Rating);
        assert_eq!(view.original.as_ref(), &CellEditValue::Rating(None));
        assert!(begin_save(&mut s, generation, CellEditValue::Rating(None)).is_err());
        let before = s.slack_lists.view.as_ref().unwrap().rows.clone();
        let attempt = begin_save(&mut s, generation, CellEditValue::Rating(Some(max))).unwrap();
        validate_edit(&mut s, &attempt, data.clone()).unwrap();
        assert_eq!(attempt.edit.cells()[0]["rating"], json!([max]));
        finish_edit(&mut s, &attempt, Ok(json!({"ok":true})));
        assert!(Arc::ptr_eq(
            &before,
            &s.slack_lists.view.as_ref().unwrap().rows
        ));
        assert_eq!(s.slack_lists.operations.len(), 1);
        let read = begin_read(&mut s).unwrap();
        let positive = stored(data, Some(max));
        assert!(!finish_read(&mut s, &read, Ok(positive.clone())));
        assert!(presentation(&s).is_none() && s.slack_lists.operations.is_empty());
        let generation = open(&mut s, "rating");
        assert!(begin_save(&mut s, generation, CellEditValue::Rating(Some(max))).is_err());
        let clear = begin_save(&mut s, generation, CellEditValue::Rating(None)).unwrap();
        validate_edit(&mut s, &clear, positive.clone()).unwrap();
        finish_edit(
            &mut s,
            &clear,
            Err(Error::Transport("fixture lost reply".into())),
        );
        let read = begin_read(&mut s).unwrap();
        let empty = stored(positive, None);
        assert!(!finish_read(&mut s, &read, Ok(empty.clone())));
        assert!(presentation(&s).is_none() && s.slack_lists.operations.is_empty());
        assert_eq!(loaded(&s).records, empty.records);
        assert_eq!(fixture::attempt_count(&s), 0); // reducer hooks, no network executor
    }
}
#[test]
fn known_empty_different_read_retires_receipt_and_keeps_stale_draft_without_later_resolution() {
    let mut s = rating_shell(None, 5);
    let data = loaded(&s);
    let generation = open(&mut s, "rating");
    let attempt = begin_save(&mut s, generation, CellEditValue::Rating(Some(3))).unwrap();
    validate_edit(&mut s, &attempt, data.clone()).unwrap();
    finish_edit(
        &mut s,
        &attempt,
        Err(Error::Transport("fixture lost reply".into())),
    );
    let read = begin_read(&mut s).unwrap();
    assert!(!finish_read(&mut s, &read, Ok(data.clone())));
    assert!(s.slack_lists.operations.is_empty());
    assert!(presentation(&s).is_some_and(|p| p.stale && p.disabled));
    assert!(
        s.slack_lists
            .view
            .as_ref()
            .unwrap()
            .error
            .as_deref()
            .unwrap()
            .contains("different value")
    );
    let read = begin_read(&mut s).unwrap();
    assert!(!finish_read(&mut s, &read, Ok(stored(data, Some(3)))));
    assert!(presentation(&s).is_some());
    assert!(begin_save(&mut s, generation, CellEditValue::Rating(None)).is_err());
    assert!(cancel_shell(&mut s, generation));
    let next = open(&mut s, "rating");
    assert_ne!(next, generation);
}
