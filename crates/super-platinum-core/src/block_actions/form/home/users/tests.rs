use super::*;
use crate::block_actions::home::{Home, View};
use crate::config::WorkspaceSession;
use crate::slack::{SlackClient, client::RequestBody};

fn key() -> InputKey {
    InputKey {
        block_id: "member".into(),
        action_id: "pick".into(),
    }
}
fn raw(optional: bool) -> Value {
    json!({"type":"home","id":"V1","app_id":"A1","app_installed_team_id":"T2","bot_id":"B1","hash":"one","opaque":{"retained":true},"blocks":[
        {"type":"input","block_id":"member","optional":optional,"label":{"type":"plain_text","text":"Reviewer"},"element":{"type":"users_select","action_id":"pick","initial_user":"UINITIAL1","placeholder":{"type":"plain_text","text":"Choose a reviewer"}}},
        {"type":"actions","block_id":"buttons","elements":[{"type":"button","action_id":"review","text":{"type":"plain_text","text":"Review"}}]}
    ]})
}
fn view(raw: Value) -> View {
    View::decode(
        &Home {
            channel: "D1".into(),
            app: "A1".into(),
            installed_team: "T2".into(),
            enabled: true,
            messages: true,
            view_id: Some("V1".into()),
        },
        raw,
    )
    .unwrap()
}
fn action(view: &View, form: &HomeForm, context: Option<&str>) -> BTreeMap<String, String> {
    let workspace = WorkspaceSession {
        team_id: "T1".into(),
        enterprise_id: None,
        user_id: "U1".into(),
        name: "Fixture".into(),
        url: "https://example.test".into(),
        token: "synthetic".into(),
    };
    let request = view
        .button_with_form("buttons", "review", "U1", form)
        .unwrap()
        .prepare_with_context(
            &SlackClient::default(),
            &workspace,
            view,
            "fixture-action",
            context,
        )
        .unwrap();
    assert!(!request.retry_safe());
    let RequestBody::Form(fields) = request.body else {
        panic!("expected form")
    };
    fields.into_iter().collect()
}
fn selected(form: &HomeForm) -> Value {
    form.state().unwrap()["values"]["member"]["pick"].clone()
}

#[test]
fn quiet_member_state_only_enters_explicit_home_button_with_context() {
    let raw = raw(false);
    let view = view(raw.clone());
    let mut form = HomeForm::parse(&raw).unwrap();
    assert_eq!(form.dispatch(&key()), Some(Dispatch::Quiet));
    assert!(form.user_field(&key()).is_ok());
    assert!(form.external_field(&key()).is_err());
    assert!(view.button("buttons", "review", "U1").is_err());
    form.set(&key(), InputValue::User(Some("USELECT02".into())))
        .unwrap();
    for (context, team) in [(None, "T2"), (Some("TCONTEXT1"), "TCONTEXT1")] {
        let wire = action(&view, &form, context);
        let state: Value = serde_json::from_str(&wire["state"]).unwrap();
        assert_eq!(
            state["values"]["member"]["pick"],
            json!({"type":"users_select","selected_user":"USELECT02"})
        );
        let actions: Value = serde_json::from_str(&wire["actions"]).unwrap();
        assert_eq!(actions.as_array().unwrap().len(), 1);
        assert_eq!(actions[0]["type"], "button");
        assert_eq!(wire["service_team_id"], team);
    }
    assert_eq!(form.raw_view(), &raw);
}

#[test]
fn emitted_null_is_committed_even_when_required_but_clear_affordance_is_not_core() {
    // HvtU optional input exposes Clear; required-null is only a callback control.
    // Home state is not modal Submit validation.
    for optional in [true, false] {
        let raw = raw(optional);
        let view = view(raw.clone());
        let mut form = HomeForm::parse(&raw).unwrap();
        form.set(&key(), InputValue::User(None)).unwrap();
        assert_eq!(form.fields[&key()].error.is_some(), !optional);
        assert_eq!(
            selected(&form),
            json!({"type":"users_select","selected_user":null})
        );
        let wire = action(&view, &form, None);
        let state: Value = serde_json::from_str(&wire["state"]).unwrap();
        assert!(state["values"]["member"]["pick"]["selected_user"].is_null());
    }
}

#[test]
fn saved_null_wins_initial_and_dirty_parent_survives_compatible_refresh() {
    let mut raw = raw(false);
    raw["state"] =
        json!({"values":{"member":{"pick":{"type":"users_select","selected_user":null}}}});
    let mut form = HomeForm::parse(&raw).unwrap();
    assert_eq!(form.fields[&key()].value, InputValue::User(None));
    form.set(&key(), InputValue::User(Some("USELECT02".into())))
        .unwrap();
    raw["hash"] = json!("two");
    raw["blocks"][0]["element"]["initial_user"] = json!("UNEWINIT1");
    raw["state"]["values"]["member"]["pick"]["selected_user"] = json!("USAVED001");
    form.reconcile(&raw).unwrap();
    assert_eq!(selected(&form)["selected_user"], "USELECT02");
    assert_eq!(
        form.user_field(&key()).unwrap().raw["initial_user"],
        "UNEWINIT1"
    );
    // Desktop owns the independent HvtU local initial-prop effect; no callback
    // may be inferred here from a different displayed initial value.
    let mut pristine = HomeForm::parse(&raw).unwrap();
    assert_eq!(selected(&pristine)["selected_user"], "USAVED001");
    raw["state"]["values"]["member"]["pick"]["selected_user"] = Value::Null;
    pristine.reconcile(&raw).unwrap();
    assert!(selected(&pristine)["selected_user"].is_null());
}

#[test]
fn other_lookup_and_semantic_flags_remain_refused() {
    let base = raw(true);
    for kind in ["future_lookup_select"] {
        let mut raw = base.clone();
        raw["blocks"][0]["element"] = json!({"type":kind,"action_id":"pick"});
        let form = HomeForm::parse(&raw).unwrap();
        assert!(form.user_field(&key()).is_err());
        assert!(form.state().is_err());
    }
    for flag in ["confirm", "dispatch_action_config", "visible_to_user_ids"] {
        let mut raw = base.clone();
        raw["blocks"][0]["element"][flag] = json!({});
        let form = HomeForm::parse(&raw).unwrap();
        assert!(form.user_field(&key()).is_err());
    }
    let mut raw = base.clone();
    raw["blocks"][0]["dispatch_action"] = json!(true);
    assert!(HomeForm::parse(&raw).unwrap().user_field(&key()).is_ok());
    let mut raw = base;
    raw["blocks"][0]["type"] = json!("section");
    raw["blocks"][0]["accessory"] = raw["blocks"][0]["element"].clone();
    assert!(HomeForm::parse(&raw).unwrap().user_field(&key()).is_err());
}

mod multiple;

#[test]
fn malformed_selection_foreign_key_and_raw_mutation_cannot_become_member_authority() {
    for id in ["", "CCHANNEL1", "Ubad", "U1"] {
        let mut form = HomeForm::parse(&raw(true)).unwrap();
        form.set(&key(), InputValue::User(Some(id.into()))).unwrap();
        assert!(form.state().is_err());
    }
    let mut form = HomeForm::parse(&raw(true)).unwrap();
    assert!(
        form.set(&key(), InputValue::Channel(Some("CCHANNEL1".into())))
            .is_err()
    );
    assert!(
        form.user_field(&InputKey {
            block_id: "other".into(),
            action_id: "pick".into()
        })
        .is_err()
    );
    form.fields.get_mut(&key()).unwrap().raw["initial_user"] = json!("UOTHER001");
    assert!(form.user_field(&key()).is_err());
    let mut raw = raw(true);
    raw["blocks"][0]["element"]["initial_user"] = json!(123);
    assert!(HomeForm::parse(&raw).is_err());
}
