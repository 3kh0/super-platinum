use super::*;

fn owner() -> WorkspaceSession {
    serde_json::from_value(
        json!({"team_id":"TINIT0001","user_id":"UINIT0001", "name":"Completion fixture",
        "url":"https://example.test", "token":"synthetic-completion-token"}),
    )
    .unwrap()
}
fn plan(query: &str) -> PreparedRequest {
    let owner = owner();
    let now = Instant::now();
    let endpoint = DirectoryEndpoint::decode(&owner, &discovery(&owner), now).unwrap();
    channels::Query::capture(
        &owner,
        query,
        channels::RequestFacts {
            top_channels: Some(json!([])),
            default_workspace: Some(owner.team_id.clone()),
        },
    )
    .unwrap()
    .request(&SlackClient::default(), &owner, &endpoint, now, None)
    .unwrap()
}
fn search(model: &Model, query: &str) -> Start {
    let owner = owner();
    let accepted = validate(&SlackClient::default(), &owner, &plan(query)).unwrap();
    start(model, &owner, accepted).unwrap()
}

#[test]
fn exact_factories_cover_only_actual_completion_reads_and_redact_auth() {
    let c = SlackClient::default();
    let w = owner();
    let requests = [
        name_policy::request::user(&c, &w),
        api::list_experiments(&c, &w),
        grid::discovery(&c, &w),
        DirectoryEndpoint::discovery(&c, &w).unwrap(),
        presence::active_request(&c, &w, None),
        presence::active_ids_request(&c, &w, None),
        directory_observations::model_request(&c, &w),
        directory_observations::offline_request(&c, &w),
        plan("project"),
    ];
    let model = Model::default();
    for request in requests {
        let accepted = validate(&c, &w, &request).unwrap();
        assert_eq!(accepted.reply.0["ok"], true);
        record(&model, &accepted);
    }
    let snapshot = model.snapshot();
    assert_eq!(snapshot["requests"].as_array().unwrap().len(), 9);
    assert!(!snapshot.to_string().contains(&w.token));
    assert!(!snapshot.to_string().contains("headers"));
    assert_eq!(snapshot["networkCalls"], 0);
    assert!(validate(&c, &w, &c.rest_form(&w, "chat.postMessage", vec![])).is_err());
    assert!(validate(&c, &w, &c.rest_form(&w, "experiments.getByUser", vec![])).is_err());
}

#[test]
fn actual_decoders_accept_complete_preferences_membership_drafts_and_cache_replies() {
    let c = SlackClient::default();
    let w = owner();
    let reply = |p: PreparedRequest| validate(&c, &w, &p).unwrap().reply.0;
    let prefs = reply(name_policy::request::user(&c, &w));
    name_policy::UserPrefs::decode(&prefs).unwrap();
    super_platinum_core::composer_emoji::history::History::from_pref(&prefs["prefs"]["frecency"])
        .unwrap();
    let gantry = reply(grid::discovery(&c, &w));
    grid::Membership::decode(gantry.clone(), &w).unwrap();
    assert_eq!(gantry["default_workspace"], w.team_id);
    DirectoryEndpoint::decode(
        &w,
        &reply(DirectoryEndpoint::discovery(&c, &w).unwrap()),
        Instant::now(),
    )
    .unwrap();
    assert!(
        directory_observations::decode_model(&reply(directory_observations::model_request(&c, &w)))
            .unwrap()
            .is_none()
    );
    let offline = directory_observations::Offline::decode_ordered(
        &reply(directory_observations::offline_request(&c, &w)),
        None,
    )
    .unwrap();
    assert!(offline.processed_top_channels().unwrap().is_empty());
    let mut store = presence::Presence::default();
    let mut read = presence::Read::begin(&mut store);
    assert_eq!(
        read.active(&mut store, &reply(presence::active_request(&c, &w, None)))
            .unwrap(),
        presence::Step::Done
    );
    assert_eq!(
        read.active_ids(
            &mut store,
            &reply(presence::active_ids_request(&c, &w, None))
        )
        .unwrap(),
        presence::Step::Done
    );
}

#[test]
fn owner_endpoint_body_and_request_hints_cannot_escape_the_closed_plan() {
    let c = SlackClient::default();
    let w = owner();
    for mutate in 0..9 {
        let mut p = plan("project");
        match mutate {
            0 => p.method = "GET",
            1 => p.url = p.url.replace("edgeapi.slack.com", "evil.test"),
            2 => p.url = p.url.replace("TINIT0001", "TOTHER001"),
            3 => p.headers.clear(),
            n => {
                let RequestBody::Json(ref mut body) = p.body else {
                    unreachable!()
                };
                match n {
                    4 => body["token"] = json!("foreign-token"),
                    5 => body["top_channels"] = json!([REMOTE]),
                    6 => body["default_workspace"] = json!("TOTHER001"),
                    7 => body["marker"] = json!("unsourced-page"),
                    _ => body["count"] = json!(100),
                }
            }
        }
        assert!(validate(&c, &w, &p).is_err(), "mutation {mutate}");
    }
}

#[test]
fn factory_nonce_variation_is_allowed_but_fixed_query_and_nonce_shapes_are_closed() {
    let c = SlackClient::default();
    let w = owner();
    for original in [name_policy::request::user(&c, &w), plan("project")] {
        let accepted = validate(&c, &w, &original).unwrap();
        assert!(!accepted.method.is_empty());
        for alteration in 0..6 {
            let mut changed = original.clone();
            let mut url = url::Url::parse(&changed.url).unwrap();
            let mut pairs = url
                .query_pairs()
                .map(|(k, v)| (k.into_owned(), v.into_owned()))
                .collect::<Vec<_>>();
            match alteration {
                0 => pairs.retain(|(k, _)| k != "fp"),
                1 => pairs.push(("fp".into(), "a".repeat(32))),
                2 => pairs.push(("extra".into(), "true".into())),
                3 => {
                    pairs
                        .iter_mut()
                        .find(|(k, _)| k == "_x_num_retries")
                        .unwrap()
                        .1 = "1".into()
                }
                4 => pairs.iter_mut().find(|(k, _)| k == "fp").unwrap().1 = "a".repeat(31),
                _ => pairs.iter_mut().find(|(k, _)| k == "fp").unwrap().1 = "z".repeat(32),
            }
            url.query_pairs_mut().clear().extend_pairs(pairs);
            changed.url = url.into();
            assert!(
                validate(&c, &w, &changed).is_err(),
                "alteration {alteration}"
            );
        }
    }
}

#[test]
fn array_source_order_and_complete_raw_rows_survive_actual_channel_page_decode() {
    let (raw, keys) = search_reply(&owner(), Mode::Success).unwrap();
    assert!(keys.is_none());
    let page = channels::Page::decode_ordered(raw, keys.as_deref()).unwrap();
    assert_eq!(
        page.rows()
            .iter()
            .map(|r| r["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec![REMOTE, SECOND]
    );
    for row in page.rows() {
        let native: super_platinum_core::slack::models::Channel =
            serde_json::from_value(row.clone()).unwrap();
        assert!(native.is_channel);
        assert_eq!(row["is_member"], false);
        assert_eq!(row["updated"], 101);
        assert_eq!(row["fixture_opaque"]["preserve"], row["id"]);
    }
    assert!(
        channels::Page::decode_ordered(search_reply(&owner(), Mode::Malformed).unwrap().0, None)
            .is_err()
    );
    assert!(
        channels::Page::decode_ordered(search_reply(&owner(), Mode::Empty).unwrap().0, None)
            .unwrap()
            .rows()
            .is_empty()
    );
    assert!(matches!(
        search_reply(&owner(), Mode::Failure),
        Err(Error::Offline(_))
    ));
}

#[tokio::test]
async fn held_completion_and_cancellation_have_distinct_receipts() {
    let model = Model::default();
    model.0.lock().unwrap().mode = Mode::Held;
    let Start::Held(receiver) = search(&model, "original") else {
        panic!("held")
    };
    let mut future = Box::pin(wait(model.clone(), receiver));
    tokio::select! { biased; _ = &mut future => panic!("unexpected completion"), _ = tokio::task::yield_now() => {} }
    drop(future);
    assert_eq!(model.snapshot()["cancelled"], 1);
    assert_eq!(model.snapshot()["active"], 0);
    let Start::Ready(next) = search(&model, "successor") else {
        panic!("successor must not inherit held mode")
    };
    assert!(next.is_ok());
    let late = model.release().unwrap();
    assert_eq!(late["query"], "original");
    assert_eq!(late["delivered"], false);
    assert_eq!(late["cancelledBeforeReply"], true);
    assert_eq!(model.snapshot()["completed"], 2);

    model.0.lock().unwrap().mode = Mode::Held;
    let Start::Held(receiver) = search(&model, "current") else {
        panic!("held")
    };
    assert_eq!(model.release().unwrap()["delivered"], true);
    assert!(wait(model.clone(), receiver).await.is_ok());
    assert_eq!(model.snapshot()["completed"], 3);
    assert_eq!(model.snapshot()["cancelled"], 1);
}

#[test]
fn an_installed_ledger_alone_grants_neither_fixture_identity_nor_request_access() {
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    install_interaction(&mut shell);
    shell
        .profile_edit
        .sections
        .fixture
        .as_mut()
        .unwrap()
        .completion = Some(Model::default());
    assert!(!enabled(&shell));
    assert!(request(&shell, &plan("project")).is_err());
    assert!(ranking_request(&shell, &plan("project")).is_err());
}

fn member_plan(query: &str) -> PreparedRequest {
    use super_platinum_core::slack::api::custom::texty::members as wire;
    let owner = owner();
    let now = Instant::now();
    let endpoint = DirectoryEndpoint::decode(&owner, &discovery(&owner), now).unwrap();
    wire::Query::capture(
        &owner,
        query,
        wire::RequestFacts {
            top_users: Some(json!([])),
            default_workspace: Some(owner.team_id.clone()),
        },
    )
    .unwrap()
    .request(&SlackClient::default(), &owner, &endpoint, now, None)
    .unwrap()
}
#[test]
fn member_factory_exact_raw_query_hints_and_no_invented_filter() {
    let c = SlackClient::default();
    let w = owner();
    let p = member_plan(" Alex \u{65e5} ");
    let a = validate(&c, &w, &p).unwrap();
    assert_eq!(a.method, "users/search");
    assert!(a.search);
    assert_eq!(a.args["query"], " Alex \u{65e5} ");
    assert_eq!(a.args["count"], 25);
    assert_eq!(a.args["fuzz"], 1);
    assert_eq!(a.args["top_users"], json!([]));
    assert_eq!(a.args["default_workspace"], w.team_id);
    assert!(a.args.get("filter").is_none());
    for mutation in 0..7 {
        let mut changed = p.clone();
        let RequestBody::Json(body) = &mut changed.body else {
            panic!("json")
        };
        match mutation {
            0 => body["count"] = json!(100),
            1 => body["top_users"] = json!([members::FIRST]),
            2 => body["default_workspace"] = json!("TOTHER001"),
            3 => body["filter"] = json!("active"),
            4 => body["token"] = json!("foreign"),
            5 => body["marker"] = json!("unknown"),
            _ => {
                body.as_object_mut().unwrap().remove("top_users");
            }
        }
        assert!(validate(&c, &w, &changed).is_err(), "mutation {mutation}");
    }
}
#[test]
fn member_payload_preserves_duplicate_labels_distinct_ids_opaque_and_full_profile_policy() {
    use super_platinum_core::slack::api::custom::texty::{member_policy, members as wire};
    let (raw, keys) = members::reply(&owner(), Mode::Success).unwrap();
    let page = wire::Page::decode_ordered(raw, keys.as_deref()).unwrap();
    let rows = page.rows();
    assert_eq!(rows.len(), 5);
    assert_eq!(
        rows[0]["profile"]["display_name"],
        rows[1]["profile"]["display_name"]
    );
    assert_ne!(rows[0]["id"], rows[1]["id"]);
    let policy = member_policy::Policy::decode(&json!({"ok":true,"assignments":{}})).unwrap();
    let allowed = rows
        .iter()
        .map(|raw| {
            assert_eq!(raw["fixture_opaque"]["preserve"], raw["id"]);
            policy
                .allows(
                    &serde_json::from_value(raw.clone()).unwrap(),
                    member_policy::Facts::default(),
                )
                .unwrap()
        })
        .collect::<Vec<_>>();
    assert_eq!(allowed, vec![true, true, true, false, true]);
    assert!(
        wire::Page::decode_ordered(members::reply(&owner(), Mode::Malformed).unwrap().0, None)
            .is_err()
    );
    assert!(
        wire::Page::decode_ordered(members::reply(&owner(), Mode::Empty).unwrap().0, None)
            .unwrap()
            .rows()
            .is_empty()
    );
    assert!(matches!(
        members::reply(&owner(), Mode::Failure),
        Err(Error::Offline(_))
    ));
}
#[tokio::test]
async fn member_held_reply_and_next_failure_use_member_endpoint_without_channel_fallback() {
    let c = SlackClient::default();
    let w = owner();
    let model = Model::default();
    model.0.lock().unwrap().mode = Mode::Held;
    let accepted = validate(&c, &w, &member_plan("Alex")).unwrap();
    let Start::Held(receiver) = start(&model, &w, accepted).unwrap() else {
        panic!("held")
    };
    assert_eq!(model.snapshot()["memberSearches"], 1);
    assert_eq!(model.snapshot()["channelSearches"], 0);
    assert_eq!(model.release().unwrap()["delivered"], true);
    let (reply, _) = wait(model.clone(), receiver).await.unwrap();
    assert_eq!(reply["results"][0]["id"], members::FIRST);
    model.0.lock().unwrap().mode = Mode::Failure;
    let accepted = validate(&c, &w, &member_plan("Alex retry")).unwrap();
    let Start::Ready(reply) = start(&model, &w, accepted).unwrap() else {
        panic!("ready")
    };
    assert!(matches!(reply, Err(Error::Offline(_))));
    assert_eq!(model.snapshot()["memberSearches"], 2);
    assert_eq!(model.snapshot()["searches"], 2);
    assert_eq!(model.snapshot()["active"], 0);
    let Start::Ready(reply) = search(&model, "project") else {
        panic!("channel ready")
    };
    assert_eq!(reply.unwrap().0["results"][0]["id"], REMOTE);
    assert_eq!(model.snapshot()["channelSearches"], 1);
}
#[test]
fn optional_boot_factory_establishes_actual_bound_authority_then_gantry_supplies_complete_policy() {
    use super_platinum_core::state::posting_metadata::authority::{Authority, BootRead};
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    crate::app_home::channel_fetches::fixture::install(&mut shell);
    let ctx = crate::scheduled::Context::capture(&shell).unwrap();
    let read = BootRead::begin(&mut shell.core, &ctx.transport, &ctx.session).unwrap();
    let request = read.request(&shell.core).unwrap();
    let accepted = validate(&shell.core.client, &ctx.session, &request).unwrap();
    assert_eq!(accepted.method, "client.userBoot");
    assert!(
        read.apply(&mut shell.core, accepted.reply.0, false)
            .unwrap()
    );
    let authority = Authority::capture(&shell.core, &ctx.session.team_id).unwrap();
    let (membership, enterprise) = authority
        .name_policy_membership(&shell.core, name_policy::OrdinaryClientV2)
        .unwrap();
    assert!(enterprise.is_none());
    let c = &shell.core.client;
    let w = &ctx.session;
    let gantry = validate(c, w, &grid::discovery(c, w)).unwrap().reply.0;
    assert_eq!(gantry["self"]["id"], w.user_id);
    assert_eq!(gantry["self"]["team_id"], w.team_id);
    let prefs = validate(c, w, &name_policy::request::user(c, w))
        .unwrap()
        .reply
        .0;
    let mut policy = name_policy::Snapshot::new(
        name_policy::OrdinaryClientV2,
        membership,
        name_policy::UserPrefs::decode(&prefs).unwrap(),
        None,
    )
    .unwrap();
    policy
        .observe_team(
            name_policy::TeamPrefs::from_complete(
                &w.team_id,
                gantry["workspaces"][0]["prefs"].as_object().unwrap(),
            )
            .unwrap(),
        )
        .unwrap();
    assert!(!policy.should_display_real_names().unwrap());
    assert_eq!(
        policy
            .member_name(&members::rows(w)[0], "Name hidden")
            .unwrap(),
        members::LABEL
    );
}

#[test]
fn mixed_search_modes_are_bound_to_the_requested_leg() {
    let c = SlackClient::default();
    let w = owner();
    let now = Instant::now();
    let endpoint = DirectoryEndpoint::decode(&w, &discovery(&w), now).unwrap();
    let group =
        super_platinum_core::slack::api::custom::texty::groups::Query::capture(&w, "alex", true)
            .unwrap()
            .request(&c, &w, &endpoint, now, None)
            .unwrap();
    let member = super_platinum_core::slack::api::custom::texty::members::Query::capture(
        &w,
        "alex",
        super_platinum_core::slack::api::custom::texty::members::RequestFacts {
            top_users: Some(json!([])),
            default_workspace: Some(w.team_id.clone()),
        },
    )
    .unwrap()
    .request(&c, &w, &endpoint, now, None)
    .unwrap();
    let model = Model::default();
    {
        let mut ledger = model.0.lock().unwrap();
        ledger.mode = Mode::Held;
        ledger.mode_method = Some("usergroups/search");
    }
    assert!(matches!(
        start(&model, &w, validate(&c, &w, &member).unwrap()).unwrap(),
        Start::Ready(Ok(_))
    ));
    let Start::Held(receiver) = start(&model, &w, validate(&c, &w, &group).unwrap()).unwrap()
    else {
        panic!("group leg was not held")
    };
    let counts = model.snapshot();
    assert_eq!(counts["memberSearches"], 1);
    assert_eq!(counts["groupSearches"], 1);
    assert_eq!(counts["active"], 1);
    drop(receiver);
    assert_eq!(model.release().unwrap()["delivered"], false);
    let mut wrong = group;
    if let RequestBody::Json(body) = &mut wrong.body {
        body["org_wide"] = json!(false);
    }
    assert!(validate(&c, &w, &wrong).is_err());
}
