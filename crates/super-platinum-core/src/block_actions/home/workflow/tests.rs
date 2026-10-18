use super::*;
use crate::slack::client::RequestBody;
fn workspace() -> WorkspaceSession {
    WorkspaceSession {
        team_id: "T1".into(),
        enterprise_id: None,
        user_id: "U1".into(),
        name: "Synthetic".into(),
        url: "https://example.test".into(),
        token: "synthetic".into(),
    }
}
fn view() -> View {
    let home = super::super::Home {
        channel: "D1".into(),
        app: "A1".into(),
        installed_team: "T2".into(),
        enabled: true,
        messages: true,
        view_id: Some("V1".into()),
    };
    View::decode(&home,json!({"id":"V1","type":"home","app_id":"A1","app_installed_team_id":"T2","bot_id":"B1","hash":"first","blocks":[{"type":"actions","block_id":"block","elements":[{"type":"workflow_button","action_id":"action","text":{"type":"plain_text","text":"Run workflow"},"workflow":{"trigger":{"url":"https://slack.com/shortcuts/Ft12345678"}},"unknown":{"retained":true}}]}]})).unwrap()
}
fn workflow() -> Workflow {
    Workflow::capture(&view(), "block", "action", "U1").unwrap()
}
fn fields(request: PreparedRequest) -> std::collections::BTreeMap<String, String> {
    let RequestBody::Form(fields) = request.body else {
        panic!("form")
    };
    fields.into_iter().collect()
}
fn info() -> Value {
    json!({"third_party_auths":{"is_shown":false,"is_satisfied":true,"auths":[]},"missing_context_parameters":{"is_shown":false,"context_parameters":[]},"missing_customizable_input_parameters":{"is_shown":false,"customizable_input_parameters":[]},"scopes_acknowledgements":{"is_shown":true,"bot_scopes":[{"name":"channels:history","description":"Read messages"}],"user_scopes":[]},"contains_ai_steps":false,"required_context_parameters":[],"unknown":"kept"})
}
fn preflight(raw: Value) -> Preflight {
    let TripResult::Review(p) = workflow()
        .response(json!({"ok":false,"error":"pretrip_ack_required","pre_trip_info":raw}))
        .unwrap()
    else {
        panic!("review")
    };
    p
}
fn auth_info() -> Value {
    let mut raw = info();
    raw["scopes_acknowledgements"]["is_shown"] = json!(false);
    raw["third_party_auths"] = json!({"is_shown":true,"is_satisfied":false,"auths":[{"app_id":"A2","function_id":"Fn123","provider_key":"provider","provider_name":"Provider","valid_tokens":[]}]});
    raw
}
fn metadata() -> Metadata {
    workflow().metadata(json!({"ok":true,"trigger":{"id":"Ft12345678","name":"Fixture workflow","owning_team_id":"T1","workflow":{"workflow_id":"W1","app_id":"A2"}}})).unwrap()
}
#[test]
fn workflow_modal_app_comes_from_exact_trigger_metadata_not_home_app() {
    let m = metadata();
    assert_eq!(m.app(), "A2");
    assert_eq!(view().home.app, "A1");
    let mut raw = m.raw().clone();
    raw["trigger"]["workflow"]
        .as_object_mut()
        .unwrap()
        .remove("app_id");
    assert!(workflow().metadata(raw.clone()).is_err());
    raw["trigger"]["workflow"]["app"] = json!({"id":"A3"});
    assert_eq!(workflow().metadata(raw.clone()).unwrap().app(), "A3");
    raw["trigger"]["id"] = json!("Ft87654321");
    assert!(workflow().metadata(raw).is_err());
}
#[test]
fn enterprise_user_input_is_preserved_without_becoming_a_channel() {
    let mut raw = info();
    raw["missing_customizable_input_parameters"] = json!({"is_shown":true,"customizable_input_parameters":[{"name":"person","type":"slack#/types/user_id"}]});
    let mut p = preflight(raw);
    p.set_custom("person", "W123".into()).unwrap();
    assert_eq!(p.value("person", true), Some("W123"));
    if p.variant() == Variant::Multiple && p.show_inputs() {
        p.review_inputs().unwrap();
    }
    assert!(p.ready());
    assert!(p.set_custom("person", "C123".into()).is_err());
}
#[test]
fn source_channel_context_accepts_record_channel_identity_without_user_substitution() {
    let mut raw = info();
    raw["missing_context_parameters"] =
        json!({"is_shown":true,"context_parameters":[{"name":"channel_id"}]});
    let mut p = preflight(raw);
    p.set_context("channel_id", "Z12345678".into()).unwrap();
    if p.variant() == Variant::Multiple && p.show_inputs() {
        p.review_inputs().unwrap();
    }
    assert!(p.ready());
    assert!(p.set_context("channel_id", "U12345678".into()).is_err());
    assert_eq!(p.value("channel_id", false), Some("Z12345678"));
}
#[test]
fn directory_queries_match_source_limits_and_do_not_authorize_cross_kind_selection() {
    let mut raw = info();
    raw["missing_context_parameters"] =
        json!({"is_shown":true,"context_parameters":[{"name":"channel_id"}]});
    raw["missing_customizable_input_parameters"] = json!({"is_shown":true,"customizable_input_parameters":[{"name":"person","type":"slack#/types/user_id"}]});
    let p = preflight(raw);
    let client = SlackClient::default();
    let current = view();
    let w = workspace();
    let now = std::time::Instant::now();
    let endpoint = DirectoryEndpoint::decode(&w, &json!({"ok":true,"ttl_seconds":60,"url":format!("https://edgeapi.slack.com/cache/{}",w.team_id)}), now).unwrap();
    let body = |request: PreparedRequest| {
        let RequestBody::Json(mut raw) = request.body else {
            panic!("json")
        };
        assert_eq!(
            raw.as_object_mut().unwrap().remove("token"),
            Some(json!("synthetic"))
        );
        raw
    };
    assert_eq!(
        body(
            p.directory_request(
                &client,
                &w,
                &current,
                &endpoint,
                now,
                "person",
                true,
                Directory::Users,
                "Ada"
            )
            .unwrap()
        ),
        json!({"query":"Ada","count":24,"fuzz":1,"enable_workspace_ranking":true,"filter":"NOT deactivated","include_profile_only_users":true})
    );
    assert_eq!(
        body(
            p.directory_request(
                &client,
                &w,
                &current,
                &endpoint,
                now,
                "channel_id",
                false,
                Directory::Channels,
                "project"
            )
            .unwrap()
        ),
        json!({"query":"project","count":100,"fuzz":1,"include_record_channels":true,"check_membership":true})
    );
    assert_eq!(
        body(
            p.directory_request(
                &client,
                &w,
                &current,
                &endpoint,
                now,
                "channel_id",
                false,
                Directory::GroupMessages,
                "Ada, Grace"
            )
            .unwrap()
        ),
        json!({"query":"Ada,Grace","count":100,"fuzz":1,"use_display_name":true})
    );
    assert!(
        p.directory_request(
            &client,
            &w,
            &current,
            &endpoint,
            now,
            "person",
            true,
            Directory::Channels,
            "Ada"
        )
        .is_err()
    );
    let request = p
        .directory_page_request(
            &client,
            &w,
            &current,
            &endpoint,
            now,
            "channel_id",
            false,
            Directory::Channels,
            "shared",
            Some("page2"),
            Some("T2"),
        )
        .unwrap();
    assert_eq!(
        body(request),
        json!({"query":"shared","count":100,"fuzz":1,
        "include_record_channels":true,"check_membership":true,"marker":"page2","external_team":"T2"})
    );
    assert!(
        p.directory_page_request(
            &client,
            &w,
            &current,
            &endpoint,
            now,
            "person",
            true,
            Directory::Users,
            "Ada",
            None,
            Some("T2")
        )
        .is_err()
    );
    assert!(!p.ready());
}
#[test]
fn precheck_and_acknowledged_start_match_actual_home_callback_forms() {
    let c = SlackClient::default();
    let w = workspace();
    let v = view();
    let workflow = workflow();
    let first = fields(workflow.precheck(&c, &w, &v, "fixture-1").unwrap());
    assert_eq!(first["run_precheck"], "true");
    assert_eq!(
        serde_json::from_str::<Value>(&first["context"]).unwrap(),
        json!({"action_id":"action","block_id":"block","location":"button","message_ts":"V1"})
    );
    let p = preflight(info());
    assert!(p.raw()["unknown"] == "kept");
    let second = fields(p.prepare_start(&c, &w, &v, "fixture-2").unwrap());
    assert!(!second.contains_key("run_precheck"));
    assert_eq!(first["context"], second["context"]);
    assert_eq!(second["_x_reason"], "trip-trigger-from-bk-workflow-button");
}
#[test]
fn required_inputs_preserve_literal_values_and_user_insertion_order() {
    let mut raw = info();
    raw["missing_context_parameters"] = json!({"is_shown":true,"context_parameters":{"one":{"name":"channel_id"},"two":{"name":"canvas_id"}}});
    raw["missing_customizable_input_parameters"] = json!({"is_shown":true,"customizable_input_parameters":[{"name":"earlier","type":"slack#/types/user_id"},{"name":"later","type":"string"}]});
    let mut p = preflight(raw);
    assert!(!p.ready());
    assert!(p.set_context("channel_id", "U1".into()).is_err());
    p.set_context("channel_id", "C1".into()).unwrap();
    p.set_context("canvas_id", "F1".into()).unwrap();
    p.set_custom("later", " raw ".into()).unwrap();
    p.set_custom("earlier", "U1".into()).unwrap();
    if p.variant() == Variant::Multiple && p.show_inputs() {
        p.review_inputs().unwrap();
    }
    assert!(p.ready());
    let f = fields(
        p.prepare_start(&SlackClient::default(), &workspace(), &view(), "fixture")
            .unwrap(),
    );
    let context: Value = serde_json::from_str(&f["context"]).unwrap();
    assert_eq!(
        context["customizable_input_parameters"],
        json!([{"name":"later","value":" raw "},{"name":"earlier","value":"U1"}])
    );
    assert_eq!(
        context["speedbump_input_parameters"],
        json!({"channel_id":"C1","canvas_id":"F1"})
    );
    assert_eq!(context["wrapped_context"]["message_ts"], "V1");
    assert!(context["wrapped_context"].get("channel_id").is_none());
    p.set_custom("later", String::new()).unwrap();
    assert!(!p.ready());
}
#[test]
fn foreign_view_account_duplicate_button_and_metadata_do_not_gain_authority() {
    let c = SlackClient::default();
    let mut w = workspace();
    w.user_id = "U2".into();
    assert!(workflow().precheck(&c, &w, &view(), "fixture").is_err());
    let mut raw = view().raw().clone();
    raw["hash"] = json!("replacement");
    let changed = View::decode(&view().home, raw.clone()).unwrap();
    assert!(
        workflow()
            .precheck(&c, &workspace(), &changed, "fixture")
            .is_err()
    );
    let element = raw["blocks"][0]["elements"][0].clone();
    raw["blocks"][0]["elements"]
        .as_array_mut()
        .unwrap()
        .push(element);
    let duplicate = View::decode(&view().home, raw).unwrap();
    assert!(Workflow::capture(&duplicate, "block", "action", "U1").is_err());
    assert!(workflow().metadata(json!({"ok":true,"trigger":{"id":"FtFOREIGN99","workflow":{"workflow_id":"W1"},"owning_team_id":"T1"}})).is_err());
    assert!(
        preflight(info())
            .prepare_start(&c, &workspace(), &changed, "fixture")
            .is_err()
    );
}
#[test]
fn auth_start_mapping_and_actual_event_keep_exact_function_provider_user() {
    let c = SlackClient::default();
    let w = workspace();
    let v = view();
    let m = metadata();
    let mut p = preflight(auth_info());
    let f = fields(
        p.connect_request(&c, &w, &v, &m, "Fn123", "provider")
            .unwrap(),
    );
    assert_eq!(
        f.into_iter().collect::<Vec<_>>(),
        vec![
            ("_x_reason".into(), "start_function_auth".into()),
            ("app_id".into(), "A2".into()),
            ("function_id".into(), "Fn123".into()),
            ("provider_key".into(), "provider".into()),
            ("token".into(), w.token.clone()),
            ("workflow_id".into(), "W1".into())
        ]
    );
    let event = json!({"type":"external_auth_completed","external_token_id":"xt1","external_user_id":"user@example.test","provider_key":"provider","provider_name":"Provider","app_id":"A2","function_id":"Fn123","user_id":"U1"});
    for key in ["user_id", "function_id", "app_id", "provider_key"] {
        let mut foreign = event.clone();
        foreign[key] = json!("foreign");
        assert!(!p.external_auth_completed(&foreign, &v, &w).unwrap());
        assert!(!p.auth_ready());
    }
    let mut virtual_token = event.clone();
    virtual_token["external_token_id"] = json!("sat_virtual");
    assert!(!p.external_auth_completed(&virtual_token, &v, &w).unwrap());
    assert!(p.external_auth_completed(&event, &v, &w).unwrap());
    assert!(p.auth_ready());
    let f = fields(
        p.select_connection_request(&c, &w, &v, &m, "Fn123", "provider", "xt1")
            .unwrap(),
    );
    assert_eq!(f["mapping_owner_type"], "END_USER");
    assert!(!f.contains_key("function_id"));
    assert!(
        p.select_connection_request(&c, &w, &v, &m, "Fn123", "provider", "unobserved")
            .is_err()
    );
    assert!(
        p.selected_connection("Fn123", "provider", "xt1", &json!({"ok":false}))
            .is_err()
    );
    p.selected_connection("Fn123", "provider", "xt1", &json!({"ok":true}))
        .unwrap();
    assert!(p.auth[0].tokens[0].default);
}
#[test]
fn unsupported_input_and_missing_tokens_stay_visible_without_fabricated_readiness() {
    let mut raw = info();
    raw["missing_customizable_input_parameters"] = json!({"is_shown":true,"customizable_input_parameters":[{"name":"future","type":"future#/type"}]});
    assert!(
        workflow()
            .response(json!({"ok":false,"error":"pretrip_ack_required","pre_trip_info":raw}))
            .is_err()
    );
    let mut raw = auth_info();
    raw["third_party_auths"]["auths"][0]["salesforce_org_id"] = json!("00D1");
    let p = preflight(raw);
    assert!(!p.ready());
    assert!(
        p.connect_request(
            &SlackClient::default(),
            &workspace(),
            &view(),
            &metadata(),
            "Fn123",
            "provider"
        )
        .is_ok()
    );
    assert!(
        Preflight::authorization_url(&json!({"ok":true,"authorization_url":"javascript:alert(1)"}))
            .is_err()
    );
    assert_eq!(
        Preflight::authorization_url(
            &json!({"ok":true,"authorization_url":"https://example.test/oauth"})
        )
        .unwrap(),
        "https://example.test/oauth"
    );
}
#[test]
fn salesforce_branch_applies_only_to_add_and_actual_tokens_control_readiness() {
    let c = SlackClient::default();
    let w = workspace();
    let v = view();
    let m = metadata();
    let context = |group| {
        salesforce::Context::observe(
            "https://app.slack.com/client/T1",
            &json!(group),
            &json!("on"),
        )
        .unwrap()
    };
    let mut raw = auth_info();
    raw["third_party_auths"]["auths"][0]["app_id"] = json!("A04T99UKKQE");
    raw["third_party_auths"]["auths"][0]["salesforce_org_id"] = json!("00D1");
    for token in ["ordinary-token", "sat_observed"] {
        raw["third_party_auths"]["auths"][0]["valid_tokens"] = json!([{"external_token_id":token,"external_user_id":"actual@example.test","is_default":false}]);
        let p = preflight(raw.clone());
        assert!(
            p.auth_ready(),
            "hXqI checks nonempty actual validTokens, not prefix/default/org-list"
        );
        assert!(p.ready());
        assert!(
            p.connect_request(&c, &w, &v, &m, "Fn123", "provider")
                .is_err()
        );
        assert!(
            p.connect_request_with_context(
                &c,
                &w,
                &v,
                &m,
                "Fn123",
                "provider",
                Some(&context("on"))
            )
            .is_err()
        );
        assert!(
            p.connect_request_with_context(
                &c,
                &w,
                &v,
                &m,
                "Fn123",
                "provider",
                Some(&context("off"))
            )
            .is_ok()
        );
        let actual = fields(
            p.select_connection_request(&c, &w, &v, &m, "Fn123", "provider", token)
                .unwrap(),
        );
        assert_eq!(actual["external_token_id"], token);
        assert_eq!(actual["mapping_owner_type"], "END_USER");
        assert!(!actual.contains_key("function_id"));
    }
    raw["third_party_auths"]["auths"][0]["valid_tokens"] = json!([]);
    assert!(!preflight(raw).auth_ready());
}
#[test]
fn accepted_trigger_is_distinct_from_finished_workflow_and_unshown_home_precheck_refuses() {
    assert!(matches!(
        workflow()
            .response(json!({"ok":true,"is_slow_workflow":true,"contains_ai_steps":true}))
            .unwrap(),
        TripResult::Accepted {
            slow: true,
            contains_ai: true
        }
    ));
    let mut raw = info();
    raw["scopes_acknowledgements"]["is_shown"] = json!(false);
    assert!(matches!(
        workflow()
            .response(json!({"ok":false,"error":"pretrip_ack_required","pre_trip_info":raw}))
            .unwrap(),
        TripResult::Rejected
    ));
}

#[test]
fn custom_parameter_array_matches_javascript_numeric_key_order() {
    let mut raw = info();
    raw["missing_customizable_input_parameters"] = json!({"is_shown":true,"customizable_input_parameters":[{"name":"tail","type":"string"},{"name":"9","type":"string"},{"name":"2","type":"string"}]});
    let mut p = preflight(raw);
    for name in ["tail", "9", "2"] {
        p.set_custom(name, name.into()).unwrap();
    }
    assert!(p.variant() == Variant::Multiple);
    assert!(p.inputs_ready());
    assert!(
        p.prepare_start(&SlackClient::default(), &workspace(), &view(), "fixture")
            .is_err()
    );
    p.review_inputs().unwrap();
    let f = fields(
        p.prepare_start(&SlackClient::default(), &workspace(), &view(), "fixture")
            .unwrap(),
    );
    let context: Value = serde_json::from_str(&f["context"]).unwrap();
    assert_eq!(
        context["customizable_input_parameters"],
        json!([{"name":"2","value":"2"},{"name":"9","value":"9"},{"name":"tail","value":"tail"}])
    );
}

#[test]
fn actual_canvas_picker_request_and_raw_choice_remain_home_bound() {
    let mut raw = info();
    raw["missing_context_parameters"] =
        json!({"is_shown":true,"context_parameters":[{"name":"canvas_id"}]});
    let mut p = preflight(raw);
    let f = fields(
        p.canvas_request(
            &SlackClient::default(),
            &workspace(),
            &view(),
            "project notes",
        )
        .unwrap(),
    );
    assert_eq!(
        f,
        std::collections::BTreeMap::from([
            ("include_shares".to_owned(), "true".to_owned()),
            ("query".to_owned(), "type:quip project notes".to_owned()),
            ("token".to_owned(), workspace().token),
        ])
    );
    let payload = json!({"ok":true,"files":[{"id":"F1","mode":"quip","title":"Project notes","unknown":{"kept":73}},{"id":"F2","filetype":"quip","title":"Suggestion","is_ai_suggested":true},{"id":"F3","filetype":"pdf","title":"Unrelated"}]});
    let choices = p.canvas_choices(&payload).unwrap();
    assert_eq!(choices.len(), 1);
    assert_eq!(choices[0].raw(), &payload["files"][0]);
    p.select_canvas("canvas_id", &choices[0], &view(), &workspace())
        .unwrap();
    assert_eq!(p.value("canvas_id", false), Some("F1"));
    let mut w = workspace();
    w.user_id = "U2".into();
    assert!(
        p.select_canvas("canvas_id", &choices[0], &view(), &w)
            .is_err()
    );
    assert!(
        p.select_canvas("unobserved", &choices[0], &view(), &workspace())
            .is_err()
    );
}

#[test]
fn multi_input_done_is_required_and_later_edits_retire_that_confirmation() {
    let mut raw = info();
    raw["missing_customizable_input_parameters"] =
        json!({"is_shown":true,"customizable_input_parameters":[{"name":"text","type":"string"}]});
    let mut p = preflight(raw);
    assert_eq!(p.variant() == Variant::Multiple, true);
    p.set_custom("text", "first".into()).unwrap();
    assert!(p.inputs_ready());
    assert!(!p.ready());
    assert!(
        p.prepare_start(
            &SlackClient::default(),
            &workspace(),
            &view(),
            "before-done"
        )
        .is_err()
    );
    p.review_inputs().unwrap();
    assert!(p.ready());
    p.set_custom("text", "second".into()).unwrap();
    assert!(!p.ready());
    p.review_inputs().unwrap();
    assert!(p.ready());
}

#[test]
fn directory_pages_keep_membership_separate_from_raw_objects_and_marker() {
    let raw = json!({"results":{"group":[{"id":"C1","unknown":"keep"},{"id":"C2"}]},"member_channels":["C1"],"next_marker":"next"});
    let p = DirectoryPage::decode(raw.clone()).unwrap();
    assert_eq!(p.raw(), &raw);
    assert_eq!(p.rows().len(), 2);
    assert_eq!(p.member_channels(), &["C1".to_owned()]);
    assert_eq!(p.next_marker(), Some("next"));
    let mpims = DirectoryPage::decode(
        json!({"mpims":[{"id":"G1","members":["U1","U2"],"details":{"id":"G1","is_mpim":true}}]}),
    )
    .unwrap();
    assert_eq!(mpims.rows()[0]["details"]["is_mpim"], true);
    assert!(mpims.member_channels().is_empty());
    assert!(
        DirectoryPage::decode(json!({"results":[{"id":"C1"}],"member_channels":[false]})).is_err()
    );
    assert!(DirectoryPage::decode(json!({"results":[{"id":"C1"}],"next_marker":42})).is_err());
}

#[test]
fn directory_results_complete_exact_count_and_refuse_repeated_marker_without_adopting_page() {
    let mut results = DirectoryResults::new(Directory::Users);
    results
        .append(
            DirectoryPage::decode(json!({"results":[{"id":"U1"}],"next_marker":"next"})).unwrap(),
        )
        .unwrap();
    assert!(!results.complete());
    assert_eq!(results.request_marker(), Some("next"));
    let repeated =
        DirectoryPage::decode(json!({"results":[{"id":"U2"}],"next_marker":"next"})).unwrap();
    assert!(results.append(repeated).is_err());
    assert_eq!(results.pages().len(), 1);
    assert_eq!(results.rows().len(), 1);
    let rows: Vec<_> = (2..=26).map(|n| json!({"id":format!("U{n}")})).collect();
    results
        .append(DirectoryPage::decode(json!({"results":rows,"next_marker":"server"})).unwrap())
        .unwrap();
    assert!(results.complete());
    assert_eq!(results.rows().len(), 24);
    assert_eq!(results.rows()[23]["id"], "U24");
    assert_eq!(results.request_marker(), Some("U25"));
    assert!(
        results
            .append(DirectoryPage::decode(json!({"results":[]})).unwrap())
            .is_err()
    );
}

#[test]
fn directory_channel_membership_is_source_normalization_not_posting_authority() {
    let mut raw = DirectoryResults::new(Directory::Channels);
    raw.append(DirectoryPage::decode(json!({"results":[{"id":"C1","is_member":true}]})).unwrap())
        .unwrap();
    assert_eq!(raw.conversation(0, None).unwrap()["is_member"], true);
    let mut known = DirectoryResults::new(Directory::Channels);
    known
        .append(
            DirectoryPage::decode(json!({"results":[
        {"id":"C1","is_archived":true},
        {"id":"G1","is_archived":true,"is_private":true,"unknown":"keep"},
        {"id":"G2","is_archived":true,"is_mpim":true},
        {"id":"C2","is_member":true}
    ],"member_channels":["C1","G1","G2"]}))
            .unwrap(),
        )
        .unwrap();
    assert_eq!(known.conversation(0, None).unwrap()["is_member"], false);
    let private = known.conversation(1, None).unwrap();
    assert_eq!(private["is_member"], true);
    assert_eq!(private["is_archived"], true);
    assert_eq!(private["unknown"], "keep");
    assert_eq!(known.conversation(2, None).unwrap()["is_member"], false);
    assert_eq!(known.conversation(3, None).unwrap()["is_member"], false);
    assert_eq!(known.pages()[0].raw()["results"][3]["is_member"], true);
}

#[test]
fn directory_mpim_requires_actual_details_or_matching_cached_mpim_and_preserves_raw() {
    let mut results = DirectoryResults::new(Directory::GroupMessages);
    results.append(DirectoryPage::decode(json!({"mpims":[
        {"id":"G1","name":"outer","members":["U1","U2"],"details":{"id":"G1","is_mpim":true,"name":"inner","unknown":7}},
        {"id":"G2","members":["U1"],"details":{"id":"G2"}},
        {"id":"G3","members":["U1"],"details":{"id":"G4","is_mpim":true}},
        {"id":"G5","members":["U1","U2","U3","U4","U5","U6","U7","U8","U9","U10"],"details":{"id":"G5","is_mpim":true}}
    ]})).unwrap()).unwrap();
    let actual = results.conversation(0, None).unwrap();
    assert_eq!(actual["name"], "inner");
    assert_eq!(actual["unknown"], 7);
    assert_eq!(actual["is_member"], true);
    assert_eq!(actual["is_private"], true);
    assert_eq!(actual["is_open"], false);
    assert_eq!(results.rows()[0]["name"], "outer");
    assert!(
        results
            .conversation(0, Some(&json!({"id":"G1","is_mpim":false})))
            .is_err()
    );
    assert!(results.conversation(1, None).is_err());
    let cached = results
        .conversation(
            1,
            Some(&json!({"id":"G2","is_mpim":true,"name":"cached","is_open":true})),
        )
        .unwrap();
    assert_eq!(cached["name"], "cached");
    assert_eq!(cached["is_open"], true);
    assert!(results.conversation(2, None).is_err());
    assert!(results.conversation(3, None).is_err());
}
