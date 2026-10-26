use super::*;
fn setup_multi() -> (View, HomeForm, InputKey, WorkspaceSession) {
    let (old, _, key, ws) = setup();
    let mut raw = old.raw().clone();
    raw["blocks"][0]["element"] = json!({"type":"multi_external_select","action_id":"a","max_selected_items":1,
        "initial_options":[],"placeholder":{"type":"plain_text","text":"Find","emoji":true}});
    let form = HomeForm::parse(&raw).unwrap();
    (View::decode(&old.home, raw).unwrap(), form, key, ws)
}

#[test]
fn queued_full_multi_external_action_sends_latest_required_empty_and_sibling_state() {
    let (view, mut form, key, ws) = setup_multi();
    let event = form
        .multi_external_input(&key, vec![option("Queued")])
        .unwrap()
        .unwrap();
    let action = view.static_action(&event, "U1").unwrap();
    assert!(form.multi_external_input(&key, vec![]).unwrap().is_none());
    form.set(
        &InputKey {
            block_id: "notes".into(),
            action_id: "a".into(),
        },
        InputValue::Text(" latest ".into()),
    )
    .unwrap();
    let sent = fields(
        action
            .prepare_with_context(
                &SlackClient::default(),
                &ws,
                &view,
                &form,
                "event-token",
                Some("TCONTEXT"),
            )
            .unwrap(),
    );
    let actions: Value = serde_json::from_str(&sent["actions"]).unwrap();
    let state: Value = serde_json::from_str(&sent["state"]).unwrap();
    assert_eq!(
        actions,
        json!([{"type":"multi_external_select","block_id":"pick","action_id":"a",
        "selected_options":[option("Queued").raw()],"initial_options":[],"placeholder":{"type":"plain_text","text":"Find","emoji":true}}])
    );
    assert_eq!(state, form.state().unwrap());
    assert_eq!(
        state["values"]["pick"]["a"],
        json!({"type":"multi_external_select","selected_options":[]})
    );
    assert_eq!(state["values"]["notes"]["a"]["value"], "latest");
    assert_eq!(sent["service_team_id"], "TCONTEXT");
    assert_eq!(sent["_x_reason"], "dispatch_action_to_developer");
}

#[test]
fn local_overmax_does_not_replace_queued_or_button_state_and_retired_source_refuses() {
    let (view, mut form, key, ws) = setup_multi();
    let event = form
        .multi_external_input(&key, vec![option("Queued")])
        .unwrap()
        .unwrap();
    let action = view.static_action(&event, "U1").unwrap();
    let before = form.clone();
    let mut other = option("Other").raw().clone();
    other["value"] = json!("other");
    assert!(
        form.multi_external_input(
            &key,
            vec![option("Queued"), ExternalOption::parse(&other).unwrap()]
        )
        .unwrap()
        .is_none()
    );
    assert_eq!(form, before);
    let sent = fields(
        view.button_with_form("buttons", "send", "U1", &form)
            .unwrap()
            .prepare(&SlackClient::default(), &ws, &view, "button-token")
            .unwrap(),
    );
    let state: Value = serde_json::from_str(&sent["state"]).unwrap();
    assert_eq!(
        state["values"]["pick"]["a"]["selected_options"],
        json!([option("Queued").raw()])
    );
    let mut changed = view.raw().clone();
    changed["blocks"][0]["element"]["placeholder"]["text"] = json!("Changed");
    let changed = View::decode(&view.home, changed).unwrap();
    assert!(changed.static_action(&event, "U1").is_err());
    assert!(
        action
            .prepare_with_context(&SlackClient::default(), &ws, &changed, &form, "token", None)
            .is_err()
    );
    let mut corrupt = form.clone();
    corrupt.fields.get_mut(&key).unwrap().value = InputValue::ExternalOptions(vec![]);
    assert!(
        action
            .prepare_with_context(&SlackClient::default(), &ws, &view, &corrupt, "token", None)
            .is_err()
    );
}
