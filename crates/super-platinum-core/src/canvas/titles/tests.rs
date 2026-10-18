use super::*;
use crate::canvas::wire::{EncodedField as F, LoadDataResponse};
fn msg(fields: &[F]) -> RawMessage {
    RawMessage::from_fields(fields).unwrap()
}
fn text(tag: u32, s: &str) -> F {
    F::bytes(tag, s.as_bytes()).unwrap()
}
fn nested(tag: u32, fields: &[F]) -> F {
    F::message(tag, &msg(fields)).unwrap()
}
fn snapshot(rich: Option<&str>) -> CanvasCollabSnapshot {
    let section = Section::from_raw(msg(&[
        text(1, "card"),
        F::uint(2, 1).unwrap(),
        text(6, "ThTest"),
        text(7, "Dtest"),
        F::uint(9, 56).unwrap(),
        // No edit style/permissions/position/CRDT requirement is introduced.
        nested(
            12,
            &[nested(50, &[nested(3, &[nested(4, &[text(1, "sd:F2")])])])],
        ),
    ]))
    .unwrap();
    let mut snapshot = crate::slack::api::canvas_collab_test_snapshot(
        vec![section],
        msg(&[text(1, "Dtest"), F::uint(2, 1).unwrap(), text(6, "ThTest")]),
    );
    let mut fields = vec![
        text(1, "sd:F2"),
        text(2, "plain fallback"),
        F::uint(9, 1).unwrap(),
    ];
    if let Some(raw) = rich {
        fields.push(text(10, raw));
    }
    let payload = snapshot
        .load
        .payload()
        .unwrap()
        .unwrap()
        .replace_fields(62, &[nested(62, &fields)])
        .unwrap();
    replace_payload(&mut snapshot, &payload);
    snapshot
}
fn replace_payload(snapshot: &mut CanvasCollabSnapshot, payload: &RawMessage) {
    let response = snapshot
        .load
        .response()
        .unwrap()
        .replace_fields(2, &[F::message(2, payload).unwrap()])
        .unwrap();
    let wrapper = snapshot
        .load
        .raw()
        .replace_fields(2, &[F::message(2, &response).unwrap()])
        .unwrap();
    snapshot.load = LoadDataResponse::decode(wrapper.raw()).unwrap();
}
#[test]
fn capture_is_read_only_and_retires_for_context_unknown_field_or_section_change() {
    let source = Arc::new(snapshot(None));
    let bytes = source.load.raw().raw().to_vec();
    let captured = capture(source.clone(), "card").unwrap();
    captured.current(&source).unwrap();
    assert!(matches!(
        captured.presentation(false),
        Presentation::Anchor(_)
    ));
    assert_eq!(source.load.raw().raw(), bytes);
    let mut changed = (*source).clone();
    let payload = changed
        .load
        .payload()
        .unwrap()
        .unwrap()
        .replace_fields(999, &[text(999, "changed unknown source")])
        .unwrap();
    replace_payload(&mut changed, &payload);
    assert_eq!(captured.current(&changed), Err(Error::Changed));
    let mut changed = (*source).clone();
    changed.sections[0] = Section::from_raw(
        changed.sections[0]
            .raw()
            .replace_fields(80, &[text(80, "changed")])
            .unwrap(),
    )
    .unwrap();
    assert_eq!(captured.current(&changed), Err(Error::Changed));
    let mut changed = (*source).clone();
    changed.sections.push(changed.sections[0].clone());
    assert!(capture(Arc::new(changed), "card").is_err());
}
#[test]
fn missing_model_retains_exact_reference_and_does_not_fabricate_private_data() {
    let mut source = snapshot(None);
    let payload = source.load.payload().unwrap().unwrap().remove(62).unwrap();
    replace_payload(&mut source, &payload);
    let captured = capture(Arc::new(source), "card").unwrap();
    assert_eq!(captured.reference().unwrap().lookup(), "sd:F2");
    assert!(captured.data().is_none());
    assert_eq!(captured.presentation(false), Presentation::Placeholder);
}
#[test]
fn rich_completion_is_bound_to_original_plan_and_keeps_no_plain_fallback() {
    let captured = capture(Arc::new(snapshot(Some("malformed"))), "card").unwrap();
    let plan = captured.rich_plan().unwrap().unwrap();
    assert_eq!(
        captured.finish_rich(&plan, &[], false).unwrap(),
        Presentation::Placeholder
    );
    assert!(
        captured
            .finish_rich(&plan, &["injected".into()], false)
            .is_err()
    );
    let raw = r#"[{"elements":[{"type":"rich_text_section","elements":[{"type":"text","text":"a b c d e f g h i j k"}]}]}]"#;
    let next = capture(Arc::new(snapshot(Some(raw))), "card").unwrap();
    assert!(next.finish_rich(&plan, &[], false).is_err());
    let plan = next.rich_plan().unwrap().unwrap();
    let Presentation::Anchor(anchor) = next
        .finish_rich(&plan, &["a b c d e f g h i j k".into()], false)
        .unwrap()
    else {
        panic!("anchor")
    };
    assert_eq!(anchor.label, Label::Text("a b c d e f g h i j…".into()));
    assert_eq!(
        next.finish_rich(&plan, &["Title".into()], true).unwrap(),
        Presentation::Placeholder
    );
}
