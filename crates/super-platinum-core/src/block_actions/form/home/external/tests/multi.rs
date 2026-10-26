//! Retained Home JQ84/yIlz/hvzX/gEn+ contract: local widget drafts are not
//! committed form state. These tests exercise only emitted/initial state.
use super::*;
use crate::slack::api::{SuggestionsContext, block_suggestions};

fn multi_option(value: &str) -> Value {
    let mut raw = option(value);
    raw["value"] = json!(value);
    raw
}
fn multi(max: usize) -> Value {
    let mut raw = raw();
    raw["blocks"][0]["element"] = json!({
        "type":"multi_external_select", "action_id":"pick", "min_query_length":0,
        "max_selected_items":max, "initial_options":[multi_option("a"), multi_option("b")]
    });
    raw
}
fn options(ids: &[&str]) -> Vec<ExternalOption> {
    ids.iter()
        .map(|id| ExternalOption::parse(&multi_option(id)).unwrap())
        .collect()
}
fn selected(form: &HomeForm) -> Value {
    form.state().unwrap()["values"]["choice"]["pick"]["selected_options"].clone()
}
fn workspace() -> WorkspaceSession {
    WorkspaceSession {
        team_id: "T1".into(),
        enterprise_id: None,
        user_id: "U1".into(),
        name: "Fixture".into(),
        url: "https://example.test".into(),
        token: "synthetic".into(),
    }
}
fn fields(request: crate::slack::client::PreparedRequest) -> BTreeMap<String, String> {
    let RequestBody::Form(fields) = request.body else {
        panic!("expected form")
    };
    fields.into_iter().collect()
}
fn action(view: &View, form: &HomeForm, team: Option<&str>) -> BTreeMap<String, String> {
    let button = view.button_with_form("buttons", "go", "U1", form).unwrap();
    let request = button
        .prepare_with_context(
            &SlackClient::default(),
            &workspace(),
            view,
            "fixture-action",
            team,
        )
        .unwrap();
    assert!(!request.retry_safe());
    fields(request)
}

#[test]
fn ordered_opaque_multi_state_uses_home_button_and_same_suggestions_context() {
    let raw = multi(3);
    let view = view(raw.clone());
    let mut form = HomeForm::parse(&raw).unwrap();
    assert_eq!(form.dispatch(&key()), Some(Dispatch::Quiet));
    assert!(form.external_field(&key()).is_ok());
    assert!(view.button("buttons", "go", "U1").is_err());
    let chosen = options(&["b", "a"]);
    form.set(&key(), InputValue::ExternalOptions(chosen.clone()))
        .unwrap();
    let expected = json!([chosen[0].raw(), chosen[1].raw()]);
    assert_eq!(selected(&form), expected);
    for (team, expected_team) in [
        (None, "T2"),
        (Some(""), "T2"),
        (Some("TCONTEXT"), "TCONTEXT"),
    ] {
        let context = SuggestionsContext::home("T1", "U1", team, &view, &key()).unwrap();
        let query = fields(
            block_suggestions(
                &SlackClient::default(),
                &workspace(),
                &context,
                " exact query ",
            )
            .unwrap(),
        );
        let wire = action(&view, &form, team);
        assert_eq!(query["service_team_id"], expected_team);
        assert_eq!(wire["service_team_id"], expected_team);
        assert_eq!(query["container"], wire["container"]);
        assert_eq!(query["value"], " exact query ");
        assert!(!query.contains_key("function_execution_id"));
        let state: Value = serde_json::from_str(&wire["state"]).unwrap();
        assert_eq!(state, form.state().unwrap());
        let actions: Value = serde_json::from_str(&wire["actions"]).unwrap();
        assert_eq!(actions.as_array().unwrap().len(), 1);
        assert_eq!(actions[0]["type"], "button");
    }
    let prior = view.button_with_form("buttons", "go", "U1", &form).unwrap();
    let mut changed = multi_option("b");
    changed["opaque"] = json!({"replacement":true});
    form.set(
        &key(),
        InputValue::ExternalOptions(vec![ExternalOption::parse(&changed).unwrap()]),
    )
    .unwrap();
    assert_eq!(selected(&form), json!([changed]));
    assert!(!prior.same_form(&form));
}

#[test]
fn required_empty_is_committed_and_serialized_without_relaxing_message_apply() {
    let raw = multi(2);
    let view = view(raw.clone());
    let mut form = HomeForm::parse(&raw).unwrap();
    form.set(&key(), InputValue::ExternalOptions(vec![]))
        .unwrap();
    assert!(!form.fields[&key()].optional);
    assert!(form.fields[&key()].error.is_some());
    assert_eq!(selected(&form), json!([]));
    let wire = action(&view, &form, None);
    let state: Value = serde_json::from_str(&wire["state"]).unwrap();
    assert_eq!(
        state["values"]["choice"]["pick"],
        json!({"type":"multi_external_select","selected_options":[]})
    );
    assert!(validate_message_external_options(&form.fields[&key()], &[]).is_err());
    form.set_native_error(
        &key(),
        "uncommitted native text".into(),
        "Invalid native edit".into(),
    )
    .unwrap();
    assert!(form.state().is_err());
    assert!(view.button_with_form("buttons", "go", "U1", &form).is_err());
}

#[test]
fn initial_over_max_and_compatible_max_reduction_keep_committed_array() {
    let raw = multi(1);
    let view = view(raw.clone());
    let form = HomeForm::parse(&raw).unwrap();
    assert!(form.fields[&key()].error.is_some());
    assert_eq!(
        selected(&form),
        json!([multi_option("a"), multi_option("b")])
    );
    assert!(action(&view, &form, None).contains_key("state"));
    // Message/Modal validation still rejects over-limit selection.
    assert!(
        validate_message_external_options(&form.fields[&key()], &options(&["a", "b"])).is_err()
    );

    let mut form = HomeForm::parse(&multi(3)).unwrap();
    form.set(&key(), InputValue::ExternalOptions(options(&["b", "a"])))
        .unwrap();
    let before = form.state().unwrap();
    let mut next = multi(1);
    next["hash"] = json!("reduced-maximum");
    form.reconcile(&next).unwrap();
    assert!(form.external_field(&key()).is_ok());
    assert!(form.fields[&key()].error.is_some());
    assert_eq!(form.state().unwrap(), before);
    let next_view = self::view(next);
    assert!(action(&next_view, &form, Some("TCONTEXT")).contains_key("state"));
    form.set(&key(), InputValue::ExternalOptions(options(&["b"])))
        .unwrap();
    assert_eq!(selected(&form), json!([multi_option("b")]));
    assert!(form.fields[&key()].error.is_none());
}

#[test]
fn saved_empty_and_ordered_saved_over_max_take_precedence_over_initial() {
    for saved in [json!([]), json!([multi_option("b"), multi_option("a")])] {
        let mut raw = multi(1);
        raw["state"] = json!({"values":{"choice":{"pick":{"type":"multi_external_select","selected_options":saved}}}});
        let form = HomeForm::parse(&raw).unwrap();
        assert_eq!(selected(&form), saved);
        assert!(!form.dirty());
        assert!(action(&view(raw), &form, None).contains_key("state"));
    }
}

#[test]
fn multi_state_retains_type_uniqueness_and_local_size_guards() {
    let mut form = HomeForm::parse(&multi(1)).unwrap();
    let before = form.clone();
    assert!(form.set(&key(), InputValue::External(None)).is_err());
    assert_eq!(form, before);
    form.set(&key(), InputValue::ExternalOptions(options(&["a", "a"])))
        .unwrap();
    assert!(form.state().is_err());
    assert!(
        view(multi(1))
            .button_with_form("buttons", "go", "U1", &form)
            .is_err()
    );
    let too_many: Vec<_> = (0..1001)
        .map(|n| ExternalOption::parse(&multi_option(&n.to_string())).unwrap())
        .collect();
    form.set(&key(), InputValue::ExternalOptions(too_many))
        .unwrap();
    assert!(form.state().is_err());
    form.set(&key(), InputValue::ExternalOptions(options(&["b"])))
        .unwrap();
    assert_eq!(selected(&form), json!([multi_option("b")]));
    // Public field tampering cannot turn mismatched typed values into wire state.
    let f = form.fields.get_mut(&key()).unwrap();
    f.value = InputValue::Selected(vec![]);
    f.error = values::error(f);
    assert!(form.state().is_err());
}

#[test]
fn multi_form_and_suggestions_share_narrow_shape_and_identity_refusals() {
    for mode in 0..12 {
        let mut raw = multi(2);
        match mode {
            0 => raw["blocks"][0]["dispatch_action"] = json!("true"),
            1 => raw["blocks"][0]["element"]["initial_option"] = multi_option("single"),
            2 => raw["blocks"][0]["element"]["confirm"] = Value::Null,
            3 => raw["blocks"][0]["element"]["dispatch_action_config"] = json!({}),
            4 => raw["blocks"][0]["element"]["visible_to_user_ids"] = json!(["U1"]),
            5 => raw["blocks"][0]["element"]["service_team_id"] = json!("TFOREIGN"),
            6 => {
                raw["blocks"][0]["element"]["initial_options"] =
                    json!([multi_option("a"), multi_option("a")])
            }
            7 => {
                raw["blocks"][0]["element"]["initial_options"][0]["text"]["type"] = json!("mrkdwn")
            }
            8 => raw["blocks"][0]["element"]["max_selected_items"] = json!(0),
            9 => raw["blocks"][0]["element"]["min_query_length"] = json!(-1),
            10 => {
                let e = raw["blocks"][0]["element"].clone();
                raw["blocks"][0] = json!({"type":"actions","block_id":"choice","elements":[e]});
            }
            _ => {
                raw["state"] = json!({"values":{"foreign":{"pick":{"type":"multi_external_select","selected_options":[]}}}})
            }
        }
        assert!(
            HomeForm::parse(&raw)
                .and_then(|form| form.external_field(&key()).map(|_| ()))
                .is_err(),
            "mode {mode}"
        );
        let view = view(raw);
        assert!(
            SuggestionsContext::home("T1", "U1", None, &view, &key()).is_err(),
            "mode {mode}"
        );
    }
    let view = view(multi(2));
    let foreign = InputKey {
        block_id: "choice".into(),
        action_id: "other".into(),
    };
    assert!(SuggestionsContext::home("T1", "U1", None, &view, &foreign).is_err());
    let mut form = HomeForm::parse(view.raw()).unwrap();
    form.fields.get_mut(&key()).unwrap().raw["max_selected_items"] = json!(9);
    assert!(form.external_field(&key()).is_err());
}
