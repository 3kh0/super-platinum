//! Retained control update and exact batch validation regressions.
use super::control_batch::ControlBatch;
use super::control_update::ControlUpdate;
use super::{F, RawMessage, Section};
use std::collections::HashMap;

fn n(tag: u32, value: u64) -> F {
    F::uint(tag, value).unwrap()
}
fn t(tag: u32, value: &str) -> F {
    F::bytes(tag, value.as_bytes()).unwrap()
}
fn m(tag: u32, value: &RawMessage) -> F {
    F::message(tag, value).unwrap()
}
fn raw(fields: &[F]) -> RawMessage {
    RawMessage::from_fields(fields).unwrap()
}
fn changed(s: &Section, field: F) -> Section {
    Section::from_raw(
        s.raw()
            .merge_partial(&RawMessage::decode(field.raw()).unwrap())
            .unwrap(),
    )
    .unwrap()
}
fn anchor() -> Section {
    Section::from_raw(raw(&[
        t(1, "a"),
        n(2, 7),
        t(6, "thread"),
        t(7, "document"),
        n(9, 0),
    ]))
    .unwrap()
}
fn original(id: &str) -> Section {
    let highlight = raw(&[
        t(1, "creator"),
        t(3, "a"),
        t(3, "b"),
        t(3, "missing"),
        n(4, 1),
        t(6, "copied annotation"),
        t(7, "copied thread"),
    ]);
    Section::from_raw(raw(&[
        t(1, id),
        n(2, 37),
        t(6, "thread"),
        t(7, "document"),
        n(9, 9),
        n(11, 1),
        m(12, &raw(&[m(7, &highlight)])),
        t(21, "old-path"),
        t(91, "retained section"),
    ]))
    .unwrap()
}
fn capture(s: &Section) -> ControlUpdate {
    ControlUpdate::capture(s, None, &anchor(), "zzzzzz-orphaned-m".into()).unwrap()
}
fn accepted(s: &Section, update: &ControlUpdate, sequence: u64) -> Section {
    let request = Section::from_raw(update.request("0123456789ab").unwrap()).unwrap();
    changed(&s.merge_partial(&request).unwrap(), n(2, sequence))
}

#[test]
fn complete_section_associations_clear_without_deleting_old_control_or_unknown_fields() {
    let s = original("old");
    let update = capture(&s);
    let request = update.request("0123456789ab").unwrap();
    assert_eq!(
        request.fields().iter().map(|f| f.tag()).collect::<Vec<_>>(),
        vec![1, 2, 9, 12, 14, 19, 20, 21, 33]
    );
    assert!(!request.has(3));
    let highlight = request
        .message(12)
        .unwrap()
        .unwrap()
        .message(7)
        .unwrap()
        .unwrap();
    assert!(!highlight.has(3));
    assert_eq!(highlight.string(6).unwrap(), Some("copied annotation"));
    assert_eq!(highlight.string(7).unwrap(), Some("copied thread"));
    assert_eq!(highlight.uint(5).unwrap(), Some(0));
    assert_eq!(highlight.uint(9).unwrap(), Some(2));
    assert_eq!(highlight.string(10).unwrap(), Some(""));
    assert!(highlight.message(11).unwrap().unwrap().fields().is_empty());
    assert_eq!(highlight.uint(13).unwrap(), Some(0));
    assert_eq!(request.string(21).unwrap(), Some("zzzzzz-orphaned-m"));
    assert_eq!(update.original().unwrap(), s);
    let ack = accepted(&s, &update, 38);
    update.check_received(&ack, true).unwrap();
    update.check_received(&ack, false).unwrap();
    assert_eq!(ack.raw().string(91).unwrap(), Some("retained section"));
    assert!(
        update
            .check_received(&changed(&ack, t(91, "changed")), false)
            .is_err()
    );
}

#[test]
fn old_control_content_that_source_would_drop_refuses_before_any_update() {
    let s = original("old");
    let content = s.raw().message(12).unwrap().unwrap();
    let highlight = content.message(7).unwrap().unwrap();
    for changed_highlight in [
        highlight
            .merge_partial(&raw(&[t(90, "unknown retained evidence")]))
            .unwrap(),
        highlight
            .merge_partial(&raw(&[t(8, "copy-secret retained evidence")]))
            .unwrap(),
        highlight.remove(6).unwrap(),
    ] {
        let value = changed(&s, m(12, &raw(&[m(7, &changed_highlight)])));
        assert!(
            ControlUpdate::capture(&value, None, &anchor(), "zzzzzz-orphaned-m".into()).is_err()
        );
        assert_eq!(
            value
                .raw()
                .message(12)
                .unwrap()
                .unwrap()
                .message(7)
                .unwrap()
                .unwrap(),
            changed_highlight
        );
    }
}

#[test]
fn multi_control_ack_is_exact_and_failed_read_retains_all_original_response_bytes() {
    let old = original("old");
    let second = original("second");
    let one = capture(&old);
    let two = capture(&second);
    let a = accepted(&old, &one, 41);
    let b = accepted(&second, &two, 42);
    let batch = ControlBatch::capture(vec![one, two]).unwrap();
    for malformed in [
        vec![a.clone()],
        vec![a.clone(), a.clone()],
        vec![a.clone(), changed(&b, t(7, "foreign"))],
    ] {
        assert!(batch.acknowledge(&malformed).is_err());
    }
    let retained = batch.acknowledge(&[b.clone(), a.clone()]).unwrap();
    let bytes = retained.actual_bytes();
    assert!(
        retained
            .check_confirmation(&[a.clone(), second], &HashMap::new())
            .is_err()
    );
    assert_eq!(retained.actual_bytes(), bytes);
    let confirmed = retained.confirm(&[b, a], &HashMap::new()).unwrap();
    assert_eq!(confirmed.actual_bytes(), bytes);
    assert_eq!(confirmed.batch().requests("0123456789ab").unwrap().len(), 2);
    assert_eq!(retained.actual_bytes(), bytes);
}

#[test]
fn fresh_read_must_retain_actual_ack_sequence_and_original_metadata() {
    let s = original("old");
    let meta = raw(&[t(1, "old"), n(2, 5), n(8, 1), t(99, "retained metadata")]);
    let update =
        ControlUpdate::capture(&s, Some(&meta), &anchor(), "zzzzzz-orphaned-m".into()).unwrap();
    let received = accepted(&s, &update, 42);
    let batch = ControlBatch::capture(vec![update]).unwrap();
    let retained = batch.acknowledge(&[received.clone()]).unwrap();
    assert!(
        retained
            .check_confirmation(&[received.clone()], &HashMap::new())
            .is_err()
    );
    let mut metadata = HashMap::new();
    metadata.insert("old".into(), meta);
    assert!(
        retained
            .check_confirmation(&[changed(&received, n(2, 41))], &metadata)
            .is_err()
    );
    retained
        .check_confirmation(&[received.clone()], &metadata)
        .unwrap();
    metadata.insert("old".into(), raw(&[t(1, "old"), n(2, 6), n(8, 1)]));
    assert!(retained.check_confirmation(&[received], &metadata).is_err());
}

#[test]
fn inert_batches_refuse_duplicates_and_cannot_relax_control_or_placement_identity() {
    let original = original("old");
    let update = capture(&original);
    assert!(ControlBatch::capture(vec![update.clone(), update.clone()]).is_err());
    assert!(ControlUpdate::capture(&original, None, &anchor(), "old-path".into()).is_err());
    let raw = serde_json::to_value(&update).unwrap();
    let mut forged = raw.clone();
    forged["cleared"] = serde_json::json!([]);
    assert!(
        serde_json::from_value::<ControlUpdate>(forged)
            .unwrap()
            .validate()
            .is_err()
    );
    let mut forged = raw;
    forged["original"] = serde_json::json!(changed(&original, n(9, 0)).raw().raw());
    assert!(
        serde_json::from_value::<ControlUpdate>(forged)
            .unwrap()
            .validate()
            .is_err()
    );
}
