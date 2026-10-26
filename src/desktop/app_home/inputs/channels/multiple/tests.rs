//! Original z+fW local state versus the independent committed Home form.
use super::super as channels;
use super::*;
use crate::app_home::inputs;
use super_platinum_core::slack::models::Channel;

const A: &str = "CINITIAL1";
const B: &str = "CSELECT02";
const C: &str = "CSELECT03";
fn key() -> InputKey {
    InputKey {
        block_id: "destination".into(),
        action_id: "pick".into(),
    }
}
fn channel(id: &str) -> Channel {
    serde_json::from_value(json!({"id":id,"name":id,"is_channel":true,"is_member":true,"updated":1,"internal_team_ids":["TOTHER001"]}))
        .unwrap()
}
fn setup(optional: bool, maximum: usize) -> ShellState {
    setup_initial(optional, maximum, &[A])
}
fn setup_initial(optional: bool, maximum: usize, initial: &[&str]) -> ShellState {
    let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
    crate::app_home::fixture::install_inputs(&mut s);
    let team = s.core.active_team.clone().unwrap();
    for id in [A, B, C] {
        s.core
            .workspaces
            .get_mut(&team)
            .unwrap()
            .channels
            .insert(id.into(), channel(id));
    }
    let h = s.app_home.surface.as_mut().unwrap();
    let mut raw = h.view.as_ref().unwrap().raw().clone();
    raw["blocks"] = json!([
        {"type":"input","block_id":"destination","optional":optional,"label":{"type":"plain_text","text":"Channels"},"element":{"type":"multi_channels_select","action_id":"pick","initial_channels":initial,"max_selected_items":maximum,"response_url_enabled":true}},
        {"type":"input","block_id":"notes","optional":true,"element":{"type":"plain_text_input","action_id":"edit","initial_value":"Retained draft"}},
        {"type":"actions","block_id":"entry","elements":[{"type":"button","action_id":"review","text":{"type":"plain_text","text":"Review"}}]}
    ]);
    h.view = Some(View::decode(h.home.as_ref().unwrap(), raw).unwrap());
    h.revision = next();
    s.app_home.forms.clear();
    inputs::hydrate(&mut s);
    s
}
fn committed(s: &ShellState) -> Vec<String> {
    let InputValue::Channels(ids) = &inputs::form(s).unwrap().fields[&key()].value else {
        panic!("multi-channel field")
    };
    ids.clone()
}
fn shown(s: &ShellState) -> Vec<String> {
    channels::display(s, &key())
        .unwrap()
        .into_iter()
        .map(|(id, _)| id)
        .collect()
}
fn widget(s: &ShellState) -> &Widget {
    &s.app_home.channels.multiples[&(inputs::owner(s).unwrap(), key())]
}
fn refresh(s: &mut ShellState, edit: impl FnOnce(&mut Value)) {
    let h = s.app_home.surface.as_mut().unwrap();
    let mut raw = h.view.as_ref().unwrap().raw().clone();
    edit(&mut raw);
    h.view = Some(View::decode(h.home.as_ref().unwrap(), raw).unwrap());
    h.revision = next();
    inputs::hydrate(s);
}
fn picker(s: &mut ShellState) -> (Ticket, u64, u64) {
    let t = Ticket::capture(s, &key()).unwrap();
    assert!(channels::open_local(s, t.clone()));
    let p = s.app_home.channels.picker.as_ref().unwrap();
    (t, p.serial, p.query_revision)
}
fn choose(s: &mut ShellState, id: &str) {
    let (t, serial, revision) = picker(s);
    assert!(channels::choose_local(s, &t, serial, revision, id));
}

#[test]
fn initial_overmax_and_lowered_max_keep_constructor_flag_and_exact_button_array() {
    use super_platinum_core::slack::{SlackClient, client::RequestBody};
    let mut s = setup_initial(false, 2, &[A, B, C]);
    for maximum in [2, 1] {
        if maximum == 1 {
            refresh(&mut s, |raw| {
                raw["blocks"][0]["element"]["max_selected_items"] = json!(1)
            });
        }
        assert_eq!(shown(&s), vec![A, B, C]);
        assert_eq!(committed(&s), vec![A, B, C]);
        assert!(!widget(&s).invalid);
        assert!(inputs::form(&s).unwrap().fields[&key()].error.is_some());
        let presentation = presentation(&s, &key()).unwrap();
        assert_eq!(presentation.maximum, Some(maximum));
        assert!(presentation.invalid);
        let h = s.app_home.surface.as_ref().unwrap();
        let view = h.view.as_ref().unwrap();
        let button = view
            .button_with_form(
                "entry",
                "review",
                &h.owner.ctx.session.user_id,
                inputs::form(&s).unwrap(),
            )
            .unwrap();
        let request = button
            .prepare_with_context(
                &SlackClient::default(),
                &h.owner.ctx.session,
                view,
                "fixture-constructor",
                None,
            )
            .unwrap();
        let RequestBody::Form(fields) = request.body else {
            panic!("form request")
        };
        let fields: BTreeMap<_, _> = fields.into_iter().collect();
        let state: Value = serde_json::from_str(&fields["state"]).unwrap();
        assert_eq!(
            state["values"]["destination"]["pick"],
            json!({
                "type":"multi_channels_select", "selected_channels":[A,B,C]
            })
        );
        let actions: Value = serde_json::from_str(&fields["actions"]).unwrap();
        assert_eq!(actions.as_array().unwrap().len(), 1);
        assert_eq!(actions[0]["type"], "button");
    }
    assert!(s.app_home.fixture_forms.is_empty());
}

#[test]
fn overmax_inline_choice_commits_ordered_array_and_required_removal_commits_empty() {
    let mut s = setup(false, 2);
    choose(&mut s, B);
    choose(&mut s, C);
    assert_eq!(shown(&s), vec![A, B, C]);
    assert_eq!(committed(&s), vec![A, B, C]);
    assert!(widget(&s).invalid);
    assert!(inputs::form(&s).unwrap().fields[&key()].error.is_some());
    assert_eq!(
        inputs::form(&s).unwrap().state().unwrap()["values"]["destination"]["pick"],
        json!({"type":"multi_channels_select","selected_channels":[A,B,C]})
    );
    let h = s.app_home.surface.as_ref().unwrap();
    assert!(
        h.view
            .as_ref()
            .unwrap()
            .button_with_form(
                "entry",
                "review",
                &h.owner.ctx.session.user_id,
                inputs::form(&s).unwrap()
            )
            .is_ok()
    );
    for id in [B, A, C] {
        let ticket = Ticket::capture(&s, &key()).unwrap();
        assert!(channels::remove_local(&mut s, &ticket, id));
    }
    assert!(shown(&s).is_empty());
    assert!(committed(&s).is_empty());
    assert!(!widget(&s).invalid);
    assert!(inputs::form(&s).unwrap().fields[&key()].error.is_some());
    assert_eq!(
        inputs::form(&s).unwrap().state().unwrap()["values"]["destination"]["pick"]["selected_channels"],
        json!([])
    );
    assert!(s.app_home.fixture_forms.is_empty());
}

#[test]
fn max_and_initial_prop_effects_do_not_emit_or_revalidate_internal_widget_state() {
    let mut s = setup(false, 2);
    choose(&mut s, B);
    choose(&mut s, C);
    let old = Ticket::capture(&s, &key()).unwrap();
    refresh(&mut s, |raw| {
        raw["blocks"][0]["element"]["max_selected_items"] = json!(4)
    });
    assert!(!old.current(&s));
    assert!(widget(&s).invalid);
    let p = presentation(&s, &key()).unwrap();
    assert_eq!(p.count, 3);
    assert!(!p.invalid);
    assert_eq!(p.text.as_deref(), Some("You can select up to 4 channels."));
    refresh(&mut s, |raw| {
        raw["blocks"][0]["element"]["initial_channels"] = json!([B])
    });
    assert_eq!(shown(&s), vec![B]);
    assert_eq!(committed(&s), vec![A, B, C]);
    assert!(widget(&s).invalid);
    refresh(&mut s, |raw| {
        raw["blocks"][0]["element"]["initial_channels"] = Value::Null
    });
    assert_eq!(shown(&s), vec![B]);
    refresh(&mut s, |raw| {
        raw["blocks"][0]["element"]
            .as_object_mut()
            .unwrap()
            .remove("initial_channels");
    });
    assert_eq!(shown(&s), vec![B]);
    refresh(&mut s, |raw| {
        raw["blocks"][0]["element"]["initial_channels"] = json!([])
    });
    assert!(shown(&s).is_empty());
    assert_eq!(committed(&s), vec![A, B, C]);
    assert!(widget(&s).invalid);
    refresh(&mut s, |raw| {
        raw["blocks"][0]["element"]["max_selected_items"] = json!(1)
    });
    let p = presentation(&s, &key()).unwrap();
    assert_eq!(p.count, 0);
    assert!(form(&s).unwrap().fields[&key()].error.is_some());
    assert!(!p.invalid);
    assert!(p.parent_error.is_none());
    assert_eq!(p.text.as_deref(), Some("You can select up to 1 channels."));
    assert!(s.app_home.fixture_forms.is_empty());
}

#[test]
fn saved_only_refresh_retains_initialized_parent_and_new_mount_uses_it() {
    let mut s = setup(true, 2);
    refresh(
        &mut s,
        |raw| raw["state"] = json!({"values":{"destination":{"pick":{"type":"multi_channels_select","selected_channels":[B,C]}}}}),
    );
    assert_eq!(shown(&s), vec![A]);
    // Actual hvzX stored-field guard retains the pristine initialized parent.
    assert_eq!(committed(&s), vec![A]);
    refresh(&mut s, |raw| {
        raw["state"]["values"]["destination"]["pick"]["selected_channels"] = json!([])
    });
    assert_eq!(shown(&s), vec![A]);
    assert_eq!(committed(&s), vec![A]);
    let owner = Owner::capture(&s).unwrap();
    let h = s.app_home.surface.as_mut().unwrap();
    h.id = owner.home_visit;
    h.owner = owner;
    channels::hydrate(&mut s);
    assert_eq!(shown(&s), vec![A]);
    assert!(!widget(&s).invalid);
    assert!(s.app_home.fixture_forms.is_empty());
}

#[test]
fn local_edit_retires_held_query_without_dropping_current_rows_or_query() {
    let mut s = setup(true, 2);
    let (t, serial, revision) = picker(&mut s);
    let (cancel, rx) = tokio::sync::watch::channel(false);
    let p = s.app_home.channels.picker.as_mut().unwrap();
    p.cancel = Some(cancel);
    p.loading = true;
    p.query = "held query".into();
    let before: Vec<_> = p.candidates.iter().map(|c| c.id.clone()).collect();
    assert!(channels::choose_local(&mut s, &t, serial, revision, B));
    assert!(*rx.borrow());
    assert!(!t.current(&s));
    let p = s.app_home.channels.picker.as_ref().unwrap();
    assert_eq!(p.serial, serial);
    assert_ne!(p.query_revision, revision);
    assert_eq!(p.query, "held query");
    assert!(!p.loading);
    assert!(p.ticket.current(&s));
    assert_eq!(
        p.candidates
            .iter()
            .map(|c| c.id.clone())
            .collect::<Vec<_>>(),
        before
    );
    assert!(!channels::search::complete(
        &mut s,
        serial,
        revision,
        Ok(vec![channel(C)])
    ));
    assert!(!channels::choose_local(&mut s, &t, serial, revision, C));
    assert_eq!(shown(&s), vec![A, B]);
    assert_eq!(committed(&s), vec![A, B]);
}

#[test]
fn context_or_account_retirement_refuses_old_callbacks_without_form_mutation() {
    for retire in ["context", "socket", "account"] {
        let mut s = setup(true, 2);
        let (t, serial, revision) = picker(&mut s);
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
        assert!(!channels::choose_local(&mut s, &t, serial, revision, B));
        assert!(!channels::search::complete(
            &mut s,
            serial,
            revision,
            Ok(vec![channel(B)])
        ));
        assert_eq!(
            s.app_home.forms[&t.form].fields[&key()].value,
            InputValue::Channels(vec![A.into()])
        );
    }
}

#[test]
fn remote_selected_label_survives_picker_overlay_and_prefers_current_native_name() {
    let mut s = setup(true, 2);
    let (t, serial, revision) = picker(&mut s);
    let remote_id = "CREMOTE01";
    let mut remote = channel(remote_id);
    remote.name = Some("remote-project".into());
    assert!(channels::search::complete(
        &mut s,
        serial,
        revision,
        Ok(vec![remote.clone()])
    ));
    let expected = s
        .app_home
        .channels
        .picker
        .as_ref()
        .unwrap()
        .candidates
        .iter()
        .find(|c| c.id == remote_id)
        .unwrap()
        .label
        .clone();
    assert!(channels::choose_local(
        &mut s, &t, serial, revision, remote_id
    ));
    s.app_home.channels.picker = None;
    s.overlay = Some(Overlay::AppModal);
    assert!(Ticket::capture(&s, &key()).is_none());
    assert_eq!(
        channels::display(&s, &key()).unwrap().last(),
        Some(&(remote_id.into(), expected))
    );
    let team = s.core.active_team.clone().unwrap();
    remote.name = Some("current-project".into());
    let w = s.core.workspaces.get_mut(&team).unwrap();
    w.channels.insert(remote_id.into(), remote);
    let expected = "#current-project".to_string();
    assert_eq!(
        channels::display(&s, &key()).unwrap().last(),
        Some(&(remote_id.into(), expected))
    );
    assert_eq!(committed(&s), vec![A, remote_id]);
}

#[test]
fn multi_search_does_not_require_single_mode_or_apply_its_enterprise_team_mask() {
    use super_platinum_core::slack::client::RequestBody;
    let mut s = setup(true, 3);
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
    inputs::hydrate(&mut s);
    assert!(
        channels::mode::Observation::capture(&s, &s.app_home.surface.as_ref().unwrap().owner)
            .is_none()
    );
    let (ticket, serial, revision) = picker(&mut s);
    assert!(ticket.mode.is_none());
    assert!(ticket.policy.allowed_internal_teams().is_none());
    let request =
        directory::prepare_channels(&ticket.owner.ctx, "project", &ticket.policy).unwrap();
    assert!(request.url.contains("/cache/ELOGICAL1/channels/search"));
    let RequestBody::Json(body) = request.body else {
        panic!("JSON request")
    };
    assert!(body.get("allowed_internal_teams").is_none());
    assert!(
        body.get("should_allowed_internal_teams_match_any")
            .is_none()
    );
    // Source-backed public rows on another internal team remain selectable.
    assert!(channels::choose_local(&mut s, &ticket, serial, revision, B));
    assert_eq!(committed(&s), vec![A, B]);
    refresh(&mut s, |raw| {
        raw["blocks"][0]["element"] = json!({
            "type":"channels_select", "action_id":"pick", "initial_channel":A
        });
    });
    assert!(Ticket::capture(&s, &key()).is_none());
}

#[test]
fn cached_return_keeps_remote_display_but_cannot_reuse_it_as_search_authority() {
    for replacement in ["same", "props", "changed", "account"] {
        let mut s = setup(true, 3);
        let (ticket, serial, revision) = picker(&mut s);
        let id = "CREMOTE01";
        let mut remote = channel(id);
        remote.name = Some("remote-project".into());
        assert!(channels::search::complete(
            &mut s,
            serial,
            revision,
            Ok(vec![remote])
        ));
        assert!(channels::choose_local(
            &mut s, &ticket, serial, revision, id
        ));
        match replacement {
            "props" => {
                // These display-only prop effects cannot discard the label of
                // the independent, still-committed remote selection.
                for initial in [json!([C]), json!([])] {
                    refresh(&mut s, |raw| {
                        raw["blocks"][0]["element"]["initial_channels"] = initial.clone();
                    });
                    assert_eq!(
                        shown(&s),
                        initial
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(|v| v.as_str().unwrap().to_owned())
                            .collect::<Vec<_>>()
                    );
                    assert_eq!(committed(&s), vec![A, id]);
                }
            }
            "changed" => {
                let current = Ticket::capture(&s, &key()).unwrap();
                assert!(inputs::set_local(
                    &mut s,
                    current.surface,
                    &key(),
                    InputValue::Channels(vec![B.into()])
                ));
            }
            "account" => {
                refresh(&mut s, |raw| {
                    raw["blocks"][0]["element"]["initial_channels"] = json!([id]);
                });
                s.core.account_epoch += 1;
            }
            _ => {}
        }
        let owner = Owner::capture(&s).unwrap();
        let h = s.app_home.surface.as_mut().unwrap();
        h.id = owner.home_visit;
        h.owner = owner;
        inputs::hydrate(&mut s);
        if matches!(replacement, "same" | "props") {
            assert_eq!(shown(&s), vec![A, id]);
            assert_eq!(
                channels::display(&s, &key()).unwrap().last(),
                Some(&(id.into(), "#remote-project".into()))
            );
            let (ticket, serial, revision) = picker(&mut s);
            assert!(
                !s.app_home
                    .channels
                    .picker
                    .as_ref()
                    .unwrap()
                    .candidates
                    .iter()
                    .any(|c| c.id == id)
            );
            assert!(!channels::choose_local(
                &mut s, &ticket, serial, revision, id
            ));
            assert_eq!(committed(&s), vec![A, id]);
            let team = s.core.active_team.clone().unwrap();
            let mut current = channel(id);
            current.name = Some("current-project".into());
            current.updated = Some(2);
            s.core
                .workspaces
                .get_mut(&team)
                .unwrap()
                .channels
                .insert(id.into(), current);
            assert_eq!(
                channels::display(&s, &key()).unwrap().last(),
                Some(&(id.into(), "#current-project".into()))
            );
        } else {
            assert!(widget(&s).retained_choices.is_empty());
            if replacement == "account" {
                assert_eq!(
                    channels::display(&s, &key()),
                    Some(vec![(id.into(), id.into())])
                );
            } else {
                assert_eq!(shown(&s), vec![B]);
            }
        }
    }
}

#[test]
fn current_archive_or_private_metadata_rejects_old_choice_without_rewriting_committed_ids() {
    for flag in ["is_archived", "is_private"] {
        let mut s = setup(true, 3);
        let (ticket, serial, revision) = picker(&mut s);
        let id = "CREMOTE01";
        assert!(channels::search::complete(
            &mut s,
            serial,
            revision,
            Ok(vec![channel(id)])
        ));
        let mut current = serde_json::to_value(channel(id)).unwrap();
        current[flag] = json!(true);
        current["updated"] = json!(2);
        let team = s.core.active_team.clone().unwrap();
        s.core
            .workspaces
            .get_mut(&team)
            .unwrap()
            .channels
            .insert(id.into(), serde_json::from_value(current).unwrap());
        assert!(!channels::choose_local(
            &mut s, &ticket, serial, revision, id
        ));
        assert_eq!(committed(&s), vec![A]);
        // Existing selected IDs are payload state, not a fresh directory choice.
        s.core
            .workspaces
            .get_mut(&team)
            .unwrap()
            .channels
            .get_mut(A)
            .unwrap()
            .is_archived = true;
        assert_eq!(
            inputs::form(&s).unwrap().state().unwrap()["values"]["destination"]["pick"],
            json!({"type":"multi_channels_select","selected_channels":[A]})
        );
    }
}
