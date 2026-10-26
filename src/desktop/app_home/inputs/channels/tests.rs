//! Actual Home hydration/picker regressions; no network or generated UI authority.
use super::*;
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
    let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
    crate::app_home::fixture::install_inputs(&mut s);
    let team = s.core.active_team.clone().unwrap();
    // Explicit configured positive mode, before rebinding the Home owner.
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
      {"type":"input","block_id":"destination","optional":optional,"label":{"type":"plain_text","text":"Destination"},"element":{"type":"channels_select","action_id":"pick","initial_channel":INITIAL,"response_url_enabled":true}},
      {"type":"input","block_id":"notes","optional":true,"label":{"type":"plain_text","text":"Notes"},"element":{"type":"plain_text_input","action_id":"edit","initial_value":"Retained draft"}},
      {"type":"actions","block_id":"entry","elements":[{"type":"button","action_id":"review","text":{"type":"plain_text","text":"Review"}}]}
    ]);
    h.view = Some(View::decode(h.home.as_ref().unwrap(), raw).unwrap());
    h.revision = next();
    s.app_home.forms.clear();
    super::super::hydrate(&mut s);
    s
}
fn committed(s: &ShellState) -> Option<String> {
    let InputValue::Channel(v) = &super::super::form(s).unwrap().fields[&key()].value else {
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
    super::super::hydrate(s);
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
fn initial_and_saved_props_preserve_initialized_parent_independent_of_display() {
    let mut s = setup(true);
    choose(&mut s, CHOSEN);
    let old = Ticket::capture(&s, &key()).unwrap();
    refresh(&mut s, |raw| {
        raw["blocks"][0]["element"]["initial_channel"] = json!(REFRESHED)
    });
    assert!(!old.current(&s));
    assert_eq!(ids(&s), vec![REFRESHED]);
    assert_eq!(committed(&s).as_deref(), Some(CHOSEN));
    refresh(&mut s, |raw| {
        raw["blocks"][0]["element"]
            .as_object_mut()
            .unwrap()
            .remove("initial_channel");
    });
    assert!(ids(&s).is_empty());
    assert_eq!(committed(&s).as_deref(), Some(CHOSEN));
    assert!(s.app_home.fixture_forms.is_empty());
    let mut s = setup(true);
    refresh(
        &mut s,
        |raw| raw["state"] = json!({"values":{"destination":{"pick":{"type":"channels_select","selected_channel":CHOSEN}}}}),
    );
    assert_eq!(ids(&s), vec![INITIAL]);
    assert_eq!(committed(&s).as_deref(), Some(INITIAL));
    refresh(&mut s, |raw| {
        raw["state"]["values"]["destination"]["pick"]["selected_channel"] = Value::Null
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
    let form = super::super::form(&s).unwrap();
    assert_eq!(
        form.state().unwrap()["values"]["destination"]["pick"],
        json!({"type":"channels_select","selected_channel":null,"response_url_enabled":true})
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

#[test]
fn canonical_public_archive_and_internal_teams_are_checked_at_choice_not_committed_state() {
    for mutation in ["archive", "private", "other-team"] {
        let mut s = setup(true);
        let (t, n, r) = picker(&mut s);
        let team = s.core.active_team.clone().unwrap();
        let c = s
            .core
            .workspaces
            .get_mut(&team)
            .unwrap()
            .channels
            .get_mut(CHOSEN)
            .unwrap();
        match mutation {
            "archive" => c.is_archived = true,
            "private" => c.is_private = true,
            "other-team" => {
                c.extra
                    .insert("internal_team_ids".into(), json!(["TOTHER001"]));
            }
            _ => unreachable!(),
        }
        assert!(!choose_local(&mut s, &t, n, r, CHOSEN), "{mutation}");
        assert_eq!(committed(&s).as_deref(), Some(INITIAL));
    }
    let mut s = setup(true);
    choose(&mut s, CHOSEN);
    let team = s.core.active_team.clone().unwrap();
    s.core
        .workspaces
        .get_mut(&team)
        .unwrap()
        .channels
        .get_mut(CHOSEN)
        .unwrap()
        .is_archived = true;
    // Accepted IDs remain structural Home state, not a new per-Button directory gate.
    assert_eq!(
        super::super::form(&s).unwrap().state().unwrap()["values"]["destination"]["pick"]["selected_channel"],
        CHOSEN
    );
}

#[test]
fn remote_canonical_choice_survives_close_and_current_native_metadata_wins() {
    let mut s = setup(true);
    let (t, n, r) = picker(&mut s);
    let team = s.core.active_team.clone().unwrap();
    let mut remote = channel("CREMOTE01", &team);
    remote.name = Some("remote-project".into());
    assert!(search::complete(&mut s, n, r, Ok(vec![remote.clone()])));
    assert!(choose_local(&mut s, &t, n, r, &remote.id));
    assert!(s.app_home.channels.picker.is_none());
    let expected = Some(vec![(remote.id.clone(), "#remote-project".into())]);
    assert_eq!(display(&s, &key()), expected);
    s.overlay = Some(Overlay::AppModal);
    assert!(Ticket::capture(&s, &key()).is_none());
    assert_eq!(display(&s, &key()), expected);
    s.overlay = None;
    s.core
        .session
        .as_mut()
        .unwrap()
        .unified_user_client_enabled
        .clear();
    assert!(Ticket::capture(&s, &key()).is_none());
    assert_eq!(display(&s, &key()), expected);
    remote.name = Some("current-name".into());
    remote.updated = Some(2);
    s.core
        .workspaces
        .get_mut(&team)
        .unwrap()
        .channels
        .insert(remote.id.clone(), remote.clone());
    assert_eq!(
        display(&s, &key()),
        Some(vec![(remote.id, "#current-name".into())])
    );
}

#[test]
fn display_empty_remains_empty_without_action_lease_and_foreign_owners_are_refused() {
    let mut s = setup(true);
    choose(&mut s, CHOSEN);
    refresh(&mut s, |raw| {
        raw["blocks"][0]["element"]
            .as_object_mut()
            .unwrap()
            .remove("initial_channel");
    });
    let old = Ticket::capture(&s, &key()).unwrap();
    s.overlay = Some(Overlay::AppModal);
    assert_eq!(display(&s, &key()), Some(vec![]));
    assert_eq!(committed(&s).as_deref(), Some(CHOSEN));
    assert!(!remove_local(&mut s, &old, CHOSEN));
    for retire in ["account", "socket", "visit", "raw"] {
        let mut s = setup(true);
        match retire {
            "account" => s.core.account_epoch += 1,
            "socket" => {
                let team = s.core.active_team.clone().unwrap();
                s.core.workspaces.get_mut(&team).unwrap().rt_generation += 1;
            }
            "visit" => {
                let o = Owner::capture(&s).unwrap();
                let h = s.app_home.surface.as_mut().unwrap();
                h.id = o.home_visit;
                h.owner = o;
            }
            "raw" => {
                let h = s.app_home.surface.as_mut().unwrap();
                let mut raw = h.view.as_ref().unwrap().raw().clone();
                raw["blocks"][0]["element"]["initial_channel"] = json!(REFRESHED);
                h.view = Some(View::decode(h.home.as_ref().unwrap(), raw).unwrap());
            }
            _ => unreachable!(),
        };
        assert!(display(&s, &key()).is_none(), "{retire}");
    }
}

#[test]
fn late_queries_and_context_socket_account_replacement_cannot_publish_or_choose() {
    let mut s = setup(true);
    let (t, n, r) = picker(&mut s);
    let team = s.core.active_team.clone().unwrap();
    let p = s.app_home.channels.picker.as_mut().unwrap();
    p.query_revision = next();
    p.loading = true;
    assert!(!search::complete(
        &mut s,
        n,
        r,
        Ok(vec![channel(REFRESHED, &team)])
    ));
    assert!(!choose_local(&mut s, &t, n, r, CHOSEN));
    assert!(s.app_home.channels.picker.as_ref().unwrap().loading);
    for retire in ["context", "socket", "account"] {
        let mut s = setup(true);
        let (t, n, r) = picker(&mut s);
        let team = s.core.active_team.clone().unwrap();
        match retire {
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
        assert!(!t.current(&s));
        assert!(!search::complete(
            &mut s,
            n,
            r,
            Ok(vec![channel(REFRESHED, &team)])
        ));
        assert!(!choose_local(&mut s, &t, n, r, CHOSEN));
        assert!(poll(&mut s));
        assert_eq!(
            s.app_home.forms[&t.form].fields[&key()].value,
            InputValue::Channel(Some(INITIAL.into()))
        );
    }
}

#[test]
fn shared_prepared_directory_route_keeps_account_separate_from_home_team_filter() {
    let s = setup(true);
    let t = Ticket::capture(&s, &key()).unwrap();
    let policy = ChannelTeamPolicy::new(true, "TCONTEXT1").unwrap();
    let req = directory::prepare_channels(&t.owner.ctx, "project", &policy).unwrap();
    assert!(req.url.contains("/cache/EHOME0001/channels/search"));
    let RequestBody::Json(body) = req.body else {
        panic!("JSON")
    };
    assert_eq!(body["allowed_internal_teams"], json!(["TCONTEXT1"]));
    assert_eq!(body["should_allowed_internal_teams_match_any"], true);
    assert!(body.get("external_team").is_none());
    assert_eq!(body["query"], "project");
    let ordinary = directory::prepare_channels(
        &t.owner.ctx,
        "project",
        &ChannelTeamPolicy::new(false, "TCONTEXT1").unwrap(),
    )
    .unwrap();
    let RequestBody::Json(body) = ordinary.body else {
        panic!("JSON")
    };
    assert!(body.get("allowed_internal_teams").is_none());
}

#[test]
fn cached_return_retains_only_same_form_same_id_remote_label_without_choice_authority() {
    for change in ["same", "changed-id", "foreign-account"] {
        let mut s = setup(true);
        let (t, n, r) = picker(&mut s);
        let team = s.core.active_team.clone().unwrap();
        let remote_id = "CREMOTE01";
        let mut remote = channel(remote_id, &team);
        remote.name = Some("remote-project".into());
        assert!(search::complete(&mut s, n, r, Ok(vec![remote])));
        assert!(choose_local(&mut s, &t, n, r, remote_id));
        match change {
            "changed-id" => {
                let current = Ticket::capture(&s, &key()).unwrap();
                assert!(super::super::set_local(
                    &mut s,
                    current.surface,
                    &key(),
                    InputValue::Channel(Some(CHOSEN.into()))
                ));
            }
            "foreign-account" => {
                // Same identifier is deliberately present in a new account's raw
                // initial field; old remote display evidence must not cross it.
                let h = s.app_home.surface.as_mut().unwrap();
                let mut raw = h.view.as_ref().unwrap().raw().clone();
                raw["blocks"][0]["element"]["initial_channel"] = json!(remote_id);
                h.view = Some(View::decode(h.home.as_ref().unwrap(), raw).unwrap());
                s.core.account_epoch += 1;
            }
            _ => {}
        }
        let owner = Owner::capture(&s).unwrap();
        let h = s.app_home.surface.as_mut().unwrap();
        h.id = owner.home_visit;
        h.owner = owner;
        super::super::hydrate(&mut s);
        let form = super::super::owner(&s).unwrap();
        let widget = &s.app_home.channels.widgets[&(form, key())];
        match change {
            "same" => {
                assert_eq!(
                    display(&s, &key()),
                    Some(vec![(remote_id.into(), "#remote-project".into())])
                );
                assert!(widget.retained_choice.is_some());
                let (t, n, r) = picker(&mut s);
                assert!(
                    !s.app_home
                        .channels
                        .picker
                        .as_ref()
                        .unwrap()
                        .candidates
                        .iter()
                        .any(|c| c.id == remote_id)
                );
                assert!(!choose_local(&mut s, &t, n, r, remote_id));
                assert_eq!(committed(&s).as_deref(), Some(remote_id));
            }
            "changed-id" => {
                assert!(widget.retained_choice.is_none());
                assert_eq!(ids(&s), vec![CHOSEN]);
            }
            "foreign-account" => {
                assert!(widget.retained_choice.is_none());
                assert_eq!(
                    display(&s, &key()),
                    Some(vec![(remote_id.into(), remote_id.into())])
                );
            }
            _ => unreachable!(),
        }
    }
}

#[test]
fn configured_workspace_mode_has_explicit_ordinary_and_enterprise_cases_without_e_guess() {
    let mut s = setup(true);
    let positive =
        mode::Observation::capture(&s, &s.app_home.surface.as_ref().unwrap().owner).unwrap();
    assert!(positive.enterprise());
    assert!(positive.current(&s));
    let team = s.core.active_team.clone().unwrap();
    assert!(team.starts_with('T'));
    s.core
        .session
        .as_mut()
        .unwrap()
        .workspaces
        .get_mut(&team)
        .unwrap()
        .enterprise_id = None;
    assert!(!positive.current(&s));
    let owner = Owner::capture(&s).unwrap();
    let h = s.app_home.surface.as_mut().unwrap();
    h.id = owner.home_visit;
    h.owner = owner;
    super::super::hydrate(&mut s);
    let ordinary =
        mode::Observation::capture(&s, &s.app_home.surface.as_ref().unwrap().owner).unwrap();
    assert!(!ordinary.enterprise());
    assert!(ordinary.current(&s));
    let ticket = Ticket::capture(&s, &key()).unwrap();
    assert!(ticket.current(&s));
    assert!(ticket.policy.allowed_internal_teams().is_none());
    s.core
        .session
        .as_mut()
        .unwrap()
        .workspaces
        .get_mut(&team)
        .unwrap()
        .enterprise_id = Some(String::new());
    let owner = Owner::capture(&s).unwrap();
    let h = s.app_home.surface.as_mut().unwrap();
    h.id = owner.home_visit;
    h.owner = owner;
    assert!(mode::Observation::capture(&s, &s.app_home.surface.as_ref().unwrap().owner).is_none());
    // Re-key a synthetic credential/workspace before a new owner: a logical E
    // identity is not the missing explicit enterprise startup-context source.
    let mut s = setup(true);
    let team = s.core.active_team.clone().unwrap();
    let enterprise = "ELOGICAL1";
    let session = s.core.session.as_mut().unwrap();
    let mut credential = session.workspaces.remove(&team).unwrap();
    credential.team_id = enterprise.into();
    credential.enterprise_id = None;
    session.workspaces.insert(enterprise.into(), credential);
    session
        .unified_user_client_enabled
        .insert(enterprise.into(), false);
    let workspace = s.core.workspaces.remove(&team).unwrap();
    s.core.workspaces.insert(enterprise.into(), workspace);
    s.core.active_team = Some(enterprise.into());
    let owner = Owner::capture(&s).unwrap();
    let h = s.app_home.surface.as_mut().unwrap();
    h.id = owner.home_visit;
    h.owner = owner;
    assert!(s.app_home.surface.as_ref().unwrap().owner.current(&s));
    assert!(mode::Observation::capture(&s, &s.app_home.surface.as_ref().unwrap().owner).is_none());
}

#[test]
fn mode_and_picker_retire_on_any_captured_config_identity_change() {
    for field in ["enterprise", "url", "user", "token", "name", "team"] {
        let mut s = setup(true);
        let ticket = Ticket::capture(&s, &key()).unwrap();
        let owner = s.app_home.surface.as_ref().unwrap().owner.clone();
        let observed = mode::Observation::capture(&s, &owner).unwrap();
        let team = s.core.active_team.clone().unwrap();
        let configured = s
            .core
            .session
            .as_mut()
            .unwrap()
            .workspaces
            .get_mut(&team)
            .unwrap();
        match field {
            "enterprise" => configured.enterprise_id = Some("ECHANGED1".into()),
            "url" => configured.url = "https://other.example.test".into(),
            "user" => configured.user_id = "UOTHER001".into(),
            "token" => configured.token = "synthetic-replacement".into(),
            "name" => configured.name = "Renamed credential".into(),
            "team" => configured.team_id = "TOTHER001".into(),
            _ => unreachable!(),
        }
        assert!(!observed.current(&s), "{field}");
        assert!(mode::Observation::capture(&s, &owner).is_none(), "{field}");
        assert!(!ticket.current(&s), "{field}");
    }
}
