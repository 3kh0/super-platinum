//! Home conversation widget/parent custody and actual shared directory preparation.
//! Pure authored regressions; no network, native window, or live credentials.
use super::super::*;
use super_platinum_core::slack::{client::RequestBody, models::Channel};
const INITIAL: &str = "CINITIAL1";
const CHOSEN: &str = "CSELECT02";
const REFRESHED: &str = "CNEWINIT1";
fn key() -> InputKey {
    InputKey {
        block_id: "destination".into(),
        action_id: "pick".into(),
    }
}
fn channel(id: &str, team: &str) -> Channel {
    serde_json::from_value(json!({"id":id,"name":id,"is_channel":true,"is_member":true,"updated":1,"internal_team_ids":[team]})).unwrap()
}
fn setup(optional: bool) -> ShellState {
    setup_source(optional, true)
}
fn setup_source(optional: bool, initial: bool) -> ShellState {
    let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
    crate::app_home::fixture::install_inputs(&mut s);
    let team = s.core.active_team.clone().unwrap();
    // Enterprise directory route is explicit; conversations do not use channel mode.
    s.core
        .session
        .as_mut()
        .unwrap()
        .workspaces
        .get_mut(&team)
        .unwrap()
        .enterprise_id = Some("EHOME0001".into());
    for id in [INITIAL, CHOSEN, REFRESHED] {
        s.core
            .workspaces
            .get_mut(&team)
            .unwrap()
            .channels
            .insert(id.into(), channel(id, &team));
    }
    let owner = Owner::capture(&s).unwrap();
    let h = s.app_home.surface.as_mut().unwrap();
    h.id = owner.home_visit;
    h.owner = owner;
    let mut raw = h.view.as_ref().unwrap().raw().clone();
    raw["blocks"] = json!([
      {"type":"input","block_id":"destination","optional":optional,"label":{"type":"plain_text","text":"Destination"},"element":{"type":"conversations_select","action_id":"pick","initial_conversation":INITIAL,"response_url_enabled":true}},
      {"type":"input","block_id":"notes","optional":true,"label":{"type":"plain_text","text":"Notes"},"element":{"type":"plain_text_input","action_id":"edit","initial_value":"Retained draft"}},
      {"type":"actions","block_id":"entry","elements":[{"type":"button","action_id":"review","text":{"type":"plain_text","text":"Review"}}]}
    ]);
    if !initial {
        raw["blocks"][0]["element"]
            .as_object_mut()
            .unwrap()
            .remove("initial_conversation");
        raw["blocks"][0]["element"]["default_to_current_conversation"] = json!(true);
    }
    h.view = Some(View::decode(h.home.as_ref().unwrap(), raw).unwrap());
    h.revision = next();
    s.app_home.forms.clear();
    super::super::super::hydrate(&mut s);
    s
}
fn committed(s: &ShellState) -> Option<String> {
    let InputValue::Conversation(v) = &super::super::super::form(s).unwrap().fields[&key()].value
    else {
        panic!("channel")
    };
    v.clone()
}
fn refresh(s: &mut ShellState, edit: impl FnOnce(&mut Value)) {
    let h = s.app_home.surface.as_mut().unwrap();
    let mut raw = h.view.as_ref().unwrap().raw().clone();
    edit(&mut raw);
    h.view = Some(View::decode(h.home.as_ref().unwrap(), raw).unwrap());
    h.revision = next();
    super::super::super::hydrate(s);
}
fn picker(s: &mut ShellState) -> (Ticket, u64, u64) {
    let t = Ticket::capture(s, &key()).unwrap();
    assert!(open_local(s, t.clone()));
    let p = s.app_home.channels.picker.as_ref().unwrap();
    (t, p.serial, p.query_revision)
}
fn choose(s: &mut ShellState, id: &str) {
    let (t, n, r) = picker(s);
    assert!(choose_local(s, &t, n, r, id));
}
fn ids(s: &ShellState) -> Vec<String> {
    display(s, &key())
        .unwrap()
        .into_iter()
        .map(|v| v.0)
        .collect()
}

#[test]
fn initial_changes_and_removal_change_display_but_saved_refresh_preserves_initialized_parent() {
    let mut s = setup(true);
    choose(&mut s, CHOSEN);
    let old = Ticket::capture(&s, &key()).unwrap();
    refresh(&mut s, |raw| {
        raw["blocks"][0]["element"]["initial_conversation"] = json!(REFRESHED)
    });
    assert!(!old.current(&s));
    assert_eq!(ids(&s), vec![REFRESHED]);
    assert_eq!(committed(&s).as_deref(), Some(CHOSEN));
    refresh(&mut s, |raw| {
        raw["blocks"][0]["element"]
            .as_object_mut()
            .unwrap()
            .remove("initial_conversation");
    });
    assert!(ids(&s).is_empty());
    assert_eq!(committed(&s).as_deref(), Some(CHOSEN));
    assert!(s.app_home.fixture_forms.is_empty());
    let mut s = setup(true);
    refresh(
        &mut s,
        |raw| raw["state"] = json!({"values":{"destination":{"pick":{"type":"conversations_select","selected_conversation":CHOSEN}}}}),
    );
    assert_eq!(ids(&s), vec![INITIAL]);
    assert_eq!(committed(&s).as_deref(), Some(INITIAL));
    refresh(&mut s, |raw| {
        raw["state"]["values"]["destination"]["pick"]["selected_conversation"] = Value::Null
    });
    assert_eq!(ids(&s), vec![INITIAL]);
    assert_eq!(committed(&s).as_deref(), Some(INITIAL));
    let owner = Owner::capture(&s).unwrap();
    let h = s.app_home.surface.as_mut().unwrap();
    h.id = owner.home_visit;
    h.owner = owner;
    hydrate(&mut s);
    assert_eq!(ids(&s), vec![INITIAL]);
}

#[test]
fn clear_is_optional_only_and_explicit_button_keeps_response_url_null_state() {
    let mut s = setup(true);
    let t = Ticket::capture(&s, &key()).unwrap();
    assert!(!remove_local(&mut s, &t, CHOSEN));
    assert!(remove_local(&mut s, &t, INITIAL));
    assert!(!remove_local(&mut s, &t, INITIAL));
    assert_eq!(committed(&s), None);
    let form = super::super::super::form(&s).unwrap();
    assert_eq!(
        form.state().unwrap()["values"]["destination"]["pick"],
        json!({"type":"conversations_select","selected_conversation":null,"response_url_enabled":true})
    );
    let h = s.app_home.surface.as_ref().unwrap();
    let view = h.view.as_ref().unwrap();
    let b = view
        .button_with_form("entry", "review", &h.owner.ctx.session.user_id, form)
        .unwrap();
    let req = b
        .prepare_with_context(
            &h.owner.ctx.client,
            &h.owner.ctx.session,
            view,
            "synthetic-review",
            None,
        )
        .unwrap();
    let RequestBody::Form(fields) = req.body else {
        panic!("form")
    };
    let fields: BTreeMap<_, _> = fields.into_iter().collect();
    let state: Value = serde_json::from_str(&fields["state"]).unwrap();
    assert_eq!(state, form.state().unwrap());
    assert!(s.app_home.fixture_forms.is_empty());
    let mut s = setup(false);
    let t = Ticket::capture(&s, &key()).unwrap();
    assert!(!remove_local(&mut s, &t, INITIAL));
    assert_eq!(committed(&s).as_deref(), Some(INITIAL));
}

fn remote_user() -> super_platinum_core::slack::models::User {
    serde_json::from_value(json!({"id":"UREMOTE01","name":"remote-member","profile":{"display_name":"Remote member"},"updated":1})).unwrap()
}
fn remote_mpim() -> Channel {
    serde_json::from_value(json!({"id":"GREMOTE01","name":"Remote group","is_mpim":true,"is_private":true,"is_member":true,"updated":1})).unwrap()
}

#[test]
fn default_flag_supplies_no_home_or_sidebar_origin_and_needs_no_channel_mode() {
    let mut s = setup(true);
    refresh(&mut s, |raw| {
        let e = &mut raw["blocks"][0]["element"];
        e.as_object_mut().unwrap().remove("initial_conversation");
        e["default_to_current_conversation"] = json!(true);
    });
    assert!(ids(&s).is_empty());
    // Existing initialized parent survives prop removal; this is not a provider default.
    assert_eq!(committed(&s).as_deref(), Some(INITIAL));
    let t = Ticket::capture(&s, &key()).unwrap();
    assert!(t.mode.is_none());
    assert!(t.conversation.is_some());
    assert!(t.current(&s));
    assert!(s.app_home.fixture_forms.is_empty());
}

#[test]
fn fresh_no_initial_default_flag_has_no_home_or_sidebar_provider() {
    let s = setup_source(true, false);
    assert!(ids(&s).is_empty());
    assert_eq!(committed(&s), None);
    let state = super::super::super::form(&s).unwrap().state().unwrap();
    assert_eq!(
        state["values"]["destination"]["pick"],
        json!({"type":"conversations_select"})
    );
    let t = Ticket::capture(&s, &key()).unwrap();
    assert!(t.mode.is_none());
    assert!(t.conversation.is_some());
}

#[test]
fn remote_user_and_mpim_keep_exact_ids_labels_and_no_dm_open_side_effect() {
    for member in [true, false] {
        let mut s = setup(true);
        let (t, n, r) = picker(&mut s);
        let active = s.core.active_channel.clone();
        let generation = s.channel_generation;
        let user = remote_user();
        let group = remote_mpim();
        let id = if member {
            user.id.clone()
        } else {
            group.id.clone()
        };
        assert!(search::complete_entities(
            &mut s,
            n,
            r,
            Ok((vec![user.clone()], vec![group.clone()]))
        ));
        assert!(choose_local(&mut s, &t, n, r, &id));
        assert_eq!(committed(&s), Some(id.clone()));
        let expected = Some(vec![(
            id.clone(),
            if member {
                "Remote member"
            } else {
                "Remote group"
            }
            .into(),
        )]);
        assert_eq!(display(&s, &key()), expected);
        assert_eq!(s.core.active_channel, active);
        assert_eq!(s.channel_generation, generation);
        assert!(s.app_home.fixture_forms.is_empty());
        s.overlay = Some(Overlay::AppModal);
        assert!(Ticket::capture(&s, &key()).is_none());
        assert_eq!(display(&s, &key()), expected);
        s.overlay = None;
        refresh(&mut s, |raw| {
            raw["blocks"][0]["element"]
                .as_object_mut()
                .unwrap()
                .remove("initial_conversation")
                .map(|_| ())
                .unwrap_or(())
        });
        assert!(ids(&s).is_empty());
        assert_eq!(committed(&s), Some(id.clone()));
        let owner = Owner::capture(&s).unwrap();
        let h = s.app_home.surface.as_mut().unwrap();
        h.id = owner.home_visit;
        h.owner = owner;
        hydrate(&mut s);
        assert_eq!(display(&s, &key()), expected);
        let (fresh, n, r) = picker(&mut s);
        assert!(!choose_local(&mut s, &fresh, n, r, &id)); // retained label is not acquired choice authority
        let team = s.core.active_team.clone().unwrap();
        if member {
            let mut current = user;
            current.profile.as_mut().unwrap().display_name = Some("Current member".into());
            s.core
                .workspaces
                .get_mut(&team)
                .unwrap()
                .users
                .insert(id.clone(), current);
        } else {
            let mut current = group;
            current.name = Some("Current group".into());
            s.core
                .workspaces
                .get_mut(&team)
                .unwrap()
                .channels
                .insert(id.clone(), current);
        }
        assert_eq!(
            display(&s, &key()).unwrap()[0].1,
            if member {
                "Current member"
            } else {
                "Current group"
            }
        );
    }
}

#[test]
fn filter_defaults_and_latest_canonical_metadata_control_choices_not_committed_state() {
    let mut s = setup(true);
    let team = s.core.active_team.clone().unwrap();
    let mut archived = channel("CARCHIVE1", &team);
    archived.is_archived = true;
    let group = remote_mpim();
    let t = Ticket::capture(&s, &key()).unwrap();
    let choices = super::candidates(&s, &t, &[], &[archived.clone(), group.clone()], "");
    assert!(!choices.iter().any(|c| c.id == archived.id));
    assert!(choices.iter().any(|c| c.id == group.id));
    refresh(&mut s, |raw| {
        raw["blocks"][0]["element"]["filter"] = json!({"exclude_bot_users":true})
    });
    let t = Ticket::capture(&s, &key()).unwrap();
    let choices = super::candidates(&s, &t, &[], &[archived.clone(), group.clone()], "");
    assert!(choices.iter().any(|c| c.id == archived.id));
    assert!(!choices.iter().any(|c| c.id == group.id));
    refresh(&mut s, |raw| {
        raw["blocks"][0]["element"]["filter"] =
            json!({"include":["public"],"only_from_team_id":"TEXTERN01"})
    });
    let (t, n, r) = picker(&mut s);
    assert!(t.mode.is_none()); // enterprise onlyFromTeam is not Modal's blanket refusal
    let mut remote = channel("CREMOTE01", &team);
    remote
        .extra
        .insert("connectedTeamIds".into(), json!(["TEXTERN01"]));
    assert!(search::complete_entities(
        &mut s,
        n,
        r,
        Ok((vec![], vec![remote.clone()]))
    ));
    remote.updated = Some(2);
    remote.is_archived = true;
    s.core
        .workspaces
        .get_mut(&team)
        .unwrap()
        .channels
        .insert(remote.id.clone(), remote.clone());
    assert!(!choose_local(&mut s, &t, n, r, &remote.id));
    assert_eq!(committed(&s).as_deref(), Some(INITIAL));
}

#[test]
fn stale_queries_and_context_socket_account_retirement_do_not_publish_or_choose() {
    for retire in ["query", "context", "socket", "account"] {
        let mut s = setup(true);
        let (t, n, r) = picker(&mut s);
        let team = s.core.active_team.clone().unwrap();
        match retire {
            "query" => {
                let p = s.app_home.channels.picker.as_mut().unwrap();
                p.query_revision = next();
                p.loading = true;
            }
            "context" => s
                .core
                .session
                .as_mut()
                .unwrap()
                .unified_user_client_enabled
                .clear(),
            "socket" => s.core.workspaces.get_mut(&team).unwrap().rt_generation += 1,
            "account" => s.core.account_epoch += 1,
            _ => unreachable!(),
        }
        assert!(
            !search::complete_entities(
                &mut s,
                n,
                r,
                Ok((vec![remote_user()], vec![remote_mpim()]))
            ),
            "{retire}"
        );
        assert!(!choose_local(&mut s, &t, n, r, CHOSEN), "{retire}");
        assert_eq!(
            s.app_home.forms[&t.form].fields[&key()].value,
            InputValue::Conversation(Some(INITIAL.into()))
        );
    }
}

#[test]
fn actual_aggregate_requests_keep_account_route_filter_masks_and_mpim_comma_normalization() {
    let mut s = setup(true);
    refresh(
        &mut s,
        |raw| raw["blocks"][0]["element"]["filter"] = json!({"include":["private","im","mpim"],"only_from_team_id":"TEXTERN01","exclude_external_shared_channels":true}),
    );
    let t = Ticket::capture(&s, &key()).unwrap();
    let requests =
        directory::aggregate::prepare(&t.owner.ctx, "Ada, Blair,  Casey", t.conversation.as_ref());
    for (endpoint, req) in [
        ("users", requests.users),
        ("channels", requests.channels),
        ("mpims", requests.mpims),
    ] {
        let req = req.unwrap().unwrap();
        assert!(
            req.url
                .contains(&format!("/cache/EHOME0001/{endpoint}/search"))
        );
        let RequestBody::Json(body) = req.body else {
            panic!("JSON")
        };
        assert_eq!(body["count"], 24);
        assert_eq!(body["fuzz"], 1);
        assert!(body.get("allowed_internal_teams").is_none());
        assert_eq!(
            body["query"],
            if endpoint == "mpims" {
                "Ada,Blair, Casey"
            } else {
                "Ada, Blair,  Casey"
            }
        );
        match endpoint {
            "users" => {
                assert_eq!(body["external_team"], "TEXTERN01");
                assert_eq!(body["filter"], "NOT deactivated AND NOT org");
            }
            "channels" => {
                assert_eq!(body["external_team"], "TEXTERN01");
                assert_eq!(body["filter"], "NOT shared");
                assert_eq!(body["include_record_channels"], true);
            }
            "mpims" => assert!(body.get("external_team").is_none()),
            _ => unreachable!(),
        }
    }
    refresh(&mut s, |raw| {
        raw["blocks"][0]["element"]["filter"] = json!({"include":["im"]})
    });
    let t = Ticket::capture(&s, &key()).unwrap();
    let requests = directory::aggregate::prepare(&t.owner.ctx, "", t.conversation.as_ref());
    assert!(requests.users.unwrap().is_some());
    assert!(requests.channels.unwrap().is_none());
    assert!(requests.mpims.unwrap().is_none());
}

#[test]
fn aggregate_cooldown_uses_longest_endpoint_before_other_errors_and_malformed_mpim_refuses() {
    use directory::aggregate::{Responses, decode};
    let limited = |s| {
        Err(Error::RateLimited {
            retry_after_secs: Some(s),
        })
    };
    let result = decode(Responses {
        users: limited(3),
        channels: Err(Error::Offline("synthetic".into())),
        mpims: limited(9),
    });
    assert!(matches!(
        result,
        Err(Error::RateLimited {
            retry_after_secs: Some(9)
        })
    ));
    assert!(
        decode(Responses {
            users: Ok(None),
            channels: Ok(None),
            mpims: Ok(Some(json!({"mpims":null})))
        })
        .is_err()
    );
    let (users,channels)=decode(Responses{users:Ok(Some(json!({"results":[]}))),channels:Ok(Some(json!({"results":[]}))),mpims:Ok(Some(json!({"mpims":[{"id":"GREMOTE01","members":["UONE00001","UTWO00002"],"details":{"is_mpim":true}}]})))}).unwrap();
    assert!(users.is_empty());
    assert_eq!(channels.len(), 1);
    assert!(channels[0].is_mpim);
    assert_eq!(channels[0].id, "GREMOTE01");
}
