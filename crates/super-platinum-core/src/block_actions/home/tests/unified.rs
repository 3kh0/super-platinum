use super::*;
use crate::block_actions::{
    external::ExternalOption,
    form::{InputKey, InputValue, home::HomeForm},
};
use crate::slack::api::{SuggestionsContext, block_suggestions};

#[test]
fn home_open_omits_missing_context_while_legacy_wrapper_keeps_installed_default() {
    let (home, _) = Home::decode("D1", &payload()).unwrap();
    let client = SlackClient::default();
    let w = workspace();
    let legacy = fields(opened(&client, &w, &home).unwrap());
    assert_eq!(legacy["service_team_id"], "T2");
    for context in [None, Some("")] {
        let request = opened_with_context(&client, &w, &home, context).unwrap();
        assert!(request.url.contains("/api/apps.home.dispatchOpenEvent"));
        let actual = fields(request);
        assert!(!actual.contains_key("service_team_id"));
        let mut expected = legacy.clone();
        expected.remove("service_team_id");
        assert_eq!(actual, expected);
    }
    let mut expected = legacy;
    expected.insert("service_team_id".into(), "TWORK0003".into());
    assert_eq!(
        fields(opened_with_context(&client, &w, &home, Some("TWORK0003")).unwrap()),
        expected
    );
    assert!(opened_with_context(&client, &w, &home, Some("T BAD")).is_err());
    let mut disabled = home;
    disabled.enabled = false;
    assert!(opened_with_context(&client, &w, &disabled, Some("TWORK0003")).is_err());
}

#[test]
fn explicit_context_changes_only_service_team_and_preserves_confirmation_and_policy() {
    let mut p = payload();
    let confirm = json!({"title":{"type":"plain_text","text":"Review?"},"text":{"type":"plain_text","text":"Continue?"},"confirm":{"type":"plain_text","text":"Yes"},"deny":{"type":"plain_text","text":"No"}});
    p["home_view"]["blocks"][0]["elements"][0]["confirm"] = confirm.clone();
    let view = Home::decode("D1", &p).unwrap().1.unwrap();
    let button = view.button("b", "go", "U1").unwrap();
    let client = SlackClient::default();
    let w = workspace();
    let legacy = button.prepare(&client, &w, &view, "same-token").unwrap();
    assert!(!legacy.retry_safe());
    let baseline = fields(legacy);
    for (context, expected) in [
        (None, "T2"),
        (Some(""), "T2"),
        (Some("TWORK0003"), "TWORK0003"),
    ] {
        let request = button
            .prepare_with_context(&client, &w, &view, "same-token", context)
            .unwrap();
        assert!(!request.retry_safe());
        assert!(request.url.contains("/api/blocks.actions"));
        let mut actual = fields(request);
        assert_eq!(actual["service_team_id"], expected);
        actual.insert("service_team_id".into(), "T2".into());
        assert_eq!(actual, baseline);
        let action: Value = serde_json::from_str(&actual["actions"]).unwrap();
        assert_eq!(action[0]["confirm"], confirm);
    }
    for malformed in [" ", "T\nOTHER", &"T".repeat(256)] {
        assert!(
            button
                .prepare_with_context(&client, &w, &view, "same-token", Some(malformed))
                .is_err()
        );
    }
    let mut foreign = w.clone();
    foreign.user_id = "UOTHER".into();
    assert!(
        button
            .prepare_with_context(&client, &foreign, &view, "same-token", Some("TWORK0003"))
            .is_err()
    );
    let mut changed = view.clone();
    changed.home.installed_team = "TOTHER".into();
    assert!(
        button
            .prepare_with_context(&client, &w, &changed, "same-token", Some("TWORK0003"))
            .is_err()
    );
    assert!(
        button
            .prepare_with_context(&client, &w, &view, "bad token", Some("TWORK0003"))
            .is_err()
    );
}

#[test]
fn same_selected_context_drives_suggestions_and_button_without_changing_raw_draft() {
    let mut p = payload();
    let selected = json!({"text":{"type":"plain_text","text":"Kept draft"},"value":"selected","opaque":{"keep":1},"url":"https://example.test/not-a-navigation"});
    p["home_view"]["blocks"].as_array_mut().unwrap().insert(0,json!({"type":"input","block_id":"choice","optional":true,"label":{"type":"plain_text","text":"Release"},"element":{"type":"external_select","action_id":"pick","min_query_length":0}}));
    let view = Home::decode("D1", &p).unwrap().1.unwrap();
    let key = InputKey {
        block_id: "choice".into(),
        action_id: "pick".into(),
    };
    let mut form = HomeForm::parse(view.raw()).unwrap();
    form.set(
        &key,
        InputValue::External(Some(ExternalOption::parse(&selected).unwrap())),
    )
    .unwrap();
    let before = form.clone();
    let client = SlackClient::default();
    let w = workspace();
    let internal = vec!["TWORK0002".into(), "TWORK0003".into()];
    for override_id in [Some("TWORK0002"), Some("TWORK0003"), None] {
        let route = context::select(&context::Inputs {
            unified: Some(true),
            channel_override: override_id,
            internal_workspace_ids: Some(if override_id.is_none() {
                &[]
            } else {
                &internal
            }),
            bot_enterprise_teams: None,
            active_team_id: None,
            workspace_filter_selected: None,
            default_workspace_id: None,
        })
        .unwrap();
        let expected = override_id.unwrap_or("T2");
        let suggestions =
            SuggestionsContext::home(&w.team_id, &w.user_id, route.team_id(), &view, &key).unwrap();
        let query = fields(block_suggestions(&client, &w, &suggestions, " raw query ").unwrap());
        let button = view.button_with_form("b", "go", &w.user_id, &form).unwrap();
        let action = fields(
            button
                .prepare_with_context(&client, &w, &view, "token", route.team_id())
                .unwrap(),
        );
        assert_eq!(query["service_team_id"], expected);
        assert_eq!(action["service_team_id"], expected);
        assert_eq!(query["container"], action["container"]);
        assert_eq!(query["value"], " raw query ");
        let state: Value = serde_json::from_str(&action["state"]).unwrap();
        assert_eq!(
            state["values"]["choice"]["pick"]["selected_option"],
            selected
        );
        assert_eq!(form, before);
        assert!(button.same_form(&form));
    }
}
