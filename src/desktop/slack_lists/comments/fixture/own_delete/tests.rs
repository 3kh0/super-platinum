use super::*;
#[test]
fn explicit_future_deletion_preserves_original_response_and_other_rows() {
    let original = json!({"ok":true,"messages":[{"ts":ROOT},own_edit::received_message(),{"ts":"1781524802.000001","text":"Other row"}],"has_more":true,"response_metadata":{"next_cursor":"original"}});
    let mut state = Some(State {
        future_deleted: Some(RECEIVED_TS.into()),
        ..Default::default()
    });
    let mut before = original.clone();
    decorate(&state, ROOT, &mut before);
    assert_eq!(before, original);
    state.as_mut().unwrap().published = Some(RECEIVED_TS.into());
    let mut after = original.clone();
    decorate(&state, ROOT, &mut after);
    assert_eq!(after["deleted"], json!([RECEIVED_TS]));
    assert_eq!(
        after["messages"],
        json!([original["messages"][0], original["messages"][2]])
    );
    assert_eq!(after["response_metadata"], original["response_metadata"]);
    assert_eq!(after["has_more"], true);
    let once = after.clone();
    decorate(&state, ROOT, &mut after);
    assert_eq!(after, once);
    assert_eq!(original["messages"][1], own_edit::received_message());
}
#[test]
fn deletion_readback_does_not_affect_another_root_or_failure_response() {
    let state = Some(State {
        published: Some(RECEIVED_TS.into()),
        ..Default::default()
    });
    for (root, raw) in [
        (
            SYNTHETIC_ROOT,
            json!({"ok":true,"messages":[own_edit::received_message()]}),
        ),
        (
            ROOT,
            json!({"ok":false,"messages":[own_edit::received_message()]}),
        ),
    ] {
        let mut after = raw.clone();
        decorate(&state, root, &mut after);
        assert_eq!(after, raw);
    }
}
