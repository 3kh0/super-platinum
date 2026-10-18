use super::*;
use serde_json::json;

fn setup() -> (ShellState, Origin) {
    let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
    crate::app_home::fixture::install(&mut s);
    let h = s.app_home.surface.as_mut().unwrap();
    h.fixture = false;
    let mut raw = h.view.as_ref().unwrap().raw().clone();
    raw["blocks"] = json!([{"type":"input","block_id":"local","label":{"type":"plain_text","text":"Local note"},"element":{"type":"plain_text_input","action_id":"value","initial_value":"unsent Home note"}},{"type":"actions","block_id":"calendar","elements":[{
        "type":"button","action_id":"calendar_hermes_connect","text":{"type":"plain_text","text":"Connect"},
        "value":json!({"app_id":"A_HOME","provider_key":"gcal"}).to_string()}]}]);
    h.view = Some(View::decode(h.home.as_ref().unwrap(), raw).unwrap());
    inputs::hydrate(&mut s);
    let h = s.app_home.surface.as_ref().unwrap();
    let origin = Origin {
        owner: h.owner.clone(),
        key: Key {
            visit: h.id,
            revision: h.revision,
        },
        entry: Entry::capture(
            h.view.as_ref().unwrap(),
            "calendar",
            "calendar_hermes_connect",
            &h.owner.ctx.session.user_id,
            "2026-10-17",
        )
        .unwrap(),
        input_revision: inputs::revision(&s),
    };
    (s, origin)
}
#[test]
fn quiet_input_edit_retires_old_calendar_origin_and_confirmation_freezes_callbacks() {
    use super_platinum_core::block_actions::form::{InputKey, InputValue};
    let (mut s, o) = setup();
    let key = InputKey {
        block_id: "local".into(),
        action_id: "value".into(),
    };
    let before = inputs::revision(&s);
    assert!(inputs::set_local(
        &mut s,
        o.key,
        &key,
        InputValue::Text("new unsent Home note".into())
    ));
    assert_ne!(inputs::revision(&s), before);
    assert!(!o.current(&s));
    let mut fresh = o.clone();
    fresh.input_revision = inputs::revision(&s);
    assert!(fresh.current(&s));
    s.app_home.surface.as_mut().unwrap().calendar_confirmation = Some(fresh.clone());
    assert!(!inputs::set_local(
        &mut s,
        o.key,
        &key,
        InputValue::Text("stale callback".into())
    ));
    assert!(fresh.current(&s));
    assert_eq!(
        inputs::form(&s).unwrap().fields[&key].value,
        InputValue::Text("new unsent Home note".into())
    );
    s.app_home.surface.as_mut().unwrap().calendar_confirmation = None;
    assert!(fresh.current(&s));
}
fn queue(s: &mut ShellState, origin: &Origin) -> u64 {
    let id = next();
    s.app_home.calendar_attempts.push(Attempt {
        id,
        origin: origin.clone(),
        phase: Phase::Queued,
        cancelled: false,
        native_lease: None,
    });
    id
}
#[test]
fn queued_connect_is_claimed_once_and_cancel_before_claim_dispatches_nothing() {
    let (mut s, o) = setup();
    assert!(o.current(&s));
    let id = queue(&mut s, &o);
    assert!(!available(&s, &o));
    cancel_pending(&mut s, id, o.key);
    assert!(!claim(&mut s, id));
    assert!(matches!(
        s.app_home.calendar_attempts[0].phase,
        Phase::Unsent
    ));
    let next = queue(&mut s, &o);
    cancel_pending(&mut s, id, o.key);
    assert!(claim(&mut s, next));
    assert!(!claim(&mut s, next));
}
#[test]
fn retired_or_cancelled_sent_connect_keeps_result_without_opening_or_replay() {
    for cancelled in [false, true] {
        let (mut s, o) = setup();
        s.core.composer.text = "new unsent input".into();
        let id = queue(&mut s, &o);
        assert!(claim(&mut s, id));
        if cancelled {
            cancel_pending(&mut s, id, o.key)
        } else {
            s.channel_generation += 1;
        }
        assert!(
            settle(
                &mut s,
                id,
                &Ok(json!({"ok":true,"authorization_url":"https://auth.example/owned"}))
            )
            .is_none()
        );
        assert!(
            matches!(&s.app_home.calendar_attempts[0].phase,Phase::Ready(url) if url=="https://auth.example/owned")
        );
        assert!(settle(&mut s, id, &Ok(json!({"ok":false}))).is_none());
        assert_eq!(s.core.composer.text, "new unsent input");
    }
}
#[test]
fn unknown_connect_is_retained_across_reopen_and_rate_limit_survives_retired_ui() {
    let (mut s, o) = setup();
    let id = queue(&mut s, &o);
    assert!(claim(&mut s, id));
    assert!(matches!(
        settle(&mut s, id, &Ok(json!({"ok":true}))),
        Some(Phase::Uncertain)
    ));
    assert!(!available(&s, &o));
    let mut reopened = o.clone();
    reopened.key.visit += 1;
    assert!(!available(&s, &reopened));
    let (mut s, o) = setup();
    let id = queue(&mut s, &o);
    assert!(claim(&mut s, id));
    s.channel_generation += 1;
    assert!(
        settle(
            &mut s,
            id,
            &Err(Error::RateLimited {
                retry_after_secs: Some(60)
            })
        )
        .is_none()
    );
    assert!(cooling(&s, &o.owner.ctx));
    assert!(matches!(
        s.app_home.calendar_attempts[0].phase,
        Phase::Unsent
    ));
}
#[test]
fn changed_home_account_raw_view_and_confirmation_key_never_gain_dispatch() {
    for change in 0..5 {
        let (mut s, o) = setup();
        let id = queue(&mut s, &o);
        match change {
            0 => s.core.account_epoch += 1,
            1 => s.app_home.surface.as_mut().unwrap().revision += 1,
            2 => s.channel_generation += 1,
            3 => s.overlay = Some(Overlay::Palette),
            _ => {
                let h = s.app_home.surface.as_mut().unwrap();
                let mut raw = h.view.as_ref().unwrap().raw().clone();
                raw["hash"] = json!("new");
                h.view = Some(View::decode(h.home.as_ref().unwrap(), raw).unwrap());
            }
        }
        assert!(!claim(&mut s, id));
        assert!(matches!(
            s.app_home.calendar_attempts[0].phase,
            Phase::Unsent
        ));
    }
}

#[test]
fn terminal_unsent_records_do_not_exhaust_admission_but_unknown_records_are_not_evicted() {
    let (mut s, o) = setup();
    for _ in 0..64 {
        let id = queue(&mut s, &o);
        cancel_pending(&mut s, id, o.key);
    }
    assert!(available(&s, &o));
    for a in &mut s.app_home.calendar_attempts {
        a.phase = Phase::Uncertain;
    }
    assert!(!available(&s, &o));
    assert_eq!(s.app_home.calendar_attempts.len(), 64);
}

#[test]
fn dispatch_failure_and_malformed_failure_do_not_become_replayable_rejections() {
    for raw in [
        json!({"ok":false,"error":"dispatch_failed"}),
        json!({"ok":false}),
        json!({"ok":true,"authorization_url":"javascript:bad"}),
    ] {
        let (mut s, o) = setup();
        let id = queue(&mut s, &o);
        assert!(claim(&mut s, id));
        assert!(matches!(
            settle(&mut s, id, &Ok(raw)),
            Some(Phase::Uncertain)
        ));
        assert!(!available(&s, &o));
    }
    let (mut s, o) = setup();
    let id = queue(&mut s, &o);
    assert!(claim(&mut s, id));
    assert!(matches!(
        settle(
            &mut s,
            id,
            &Ok(json!({"ok":false,"error":"permission_denied"}))
        ),
        Some(Phase::Rejected)
    ));
    assert!(available(&s, &o));
}

#[test]
fn calendar_home_static_fixture_retains_quiet_input_and_cannot_capture_live_actions() {
    let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
    crate::app_home::fixture::install_calendar(&mut s);
    let h = s.app_home.surface.as_ref().unwrap();
    assert!(h.fixture);
    let key = Key {
        visit: h.id,
        revision: h.revision,
    };
    let view = h.view.as_ref().unwrap();
    let raw = view.raw();
    assert!(
        raw["blocks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|b| b["type"] == "input")
    );
    let action = raw["blocks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["type"] == "actions")
        .unwrap();
    let button = &action["elements"][0];
    let m = BlockElementMetadata {
        source: BlockElementSource::Button,
        block_id: action["block_id"].as_str().map(str::to_owned),
        action_id: button["action_id"].as_str().map(str::to_owned),
        value: button["value"].as_str().map(str::to_owned),
        confirm: super_platinum_core::block_actions::BlockConfirmationState::Missing,
        accessibility_label: None,
    };
    assert!(inputs::ready(&s));
    assert!(capture(&s, key, &m).is_none());
    assert!(s.app_home.calendar_attempts.is_empty());
}

#[test]
fn local_input_edit_does_not_revoke_cancel_but_still_revokes_dispatch_and_adoption() {
    use super_platinum_core::block_actions::form::{InputKey, InputValue};
    for sent in [false, true] {
        let (mut s, o) = setup();
        let id = queue(&mut s, &o);
        if sent {
            assert!(claim(&mut s, id));
        }
        let input = InputKey {
            block_id: "local".into(),
            action_id: "value".into(),
        };
        assert!(inputs::set_local(
            &mut s,
            o.key,
            &input,
            InputValue::Text("newer unsent draft".into())
        ));
        assert!(!o.current(&s));
        cancel_pending(&mut s, id, o.key);
        assert!(s.app_home.calendar_attempts[0].cancelled);
        assert!(s.app_home.calendar_attempts[0].native_lease.is_none());
        assert!(!claim(&mut s, id));
        if sent {
            assert!(
                settle(
                    &mut s,
                    id,
                    &Ok(json!({"ok":true,"authorization_url":"https://auth.example/late"}))
                )
                .is_none()
            );
            assert!(
                matches!(&s.app_home.calendar_attempts[0].phase,Phase::Ready(url) if url=="https://auth.example/late")
            );
        } else {
            assert!(matches!(
                s.app_home.calendar_attempts[0].phase,
                Phase::Unsent
            ));
        }
        assert_eq!(
            inputs::form(&s).unwrap().fields[&input].value,
            InputValue::Text("newer unsent draft".into())
        );
    }
}

#[test]
fn cancellation_still_requires_the_exact_attempt_owner_visit_and_callback_key() {
    for changed in 0..5 {
        let (mut s, o) = setup();
        let id = queue(&mut s, &o);
        assert!(claim(&mut s, id));
        let mut key = o.key;
        let mut requested = id;
        match changed {
            0 => s.core.account_epoch += 1,
            1 => s.channel_generation += 1,
            2 => s.app_home.surface.as_mut().unwrap().revision += 1,
            3 => key.visit += 1,
            _ => requested += 1,
        }
        cancel_pending(&mut s, requested, key);
        assert!(!s.app_home.calendar_attempts[0].cancelled);
        assert!(matches!(s.app_home.calendar_attempts[0].phase, Phase::Sent));
    }
}

#[test]
fn calendar_cancel_and_retirement_release_exact_mount_without_clearing_sent_custody() {
    use tokio::sync::oneshot::error::TryRecvError;
    for sent in [false, true] {
        let (mut s, origin) = setup();
        let id = queue(&mut s, &origin);
        if sent {
            assert!(claim(&mut s, id));
        }
        let (release, mut released) = inputs::native::test_pair();
        s.app_home.calendar_attempts[0].native_lease = Some(release);
        cancel_pending(&mut s, id + 1, origin.key);
        assert_eq!(released.try_recv(), Err(TryRecvError::Empty));
        cancel_pending(&mut s, id, origin.key);
        assert_eq!(released.try_recv(), Ok(()));
        assert!(s.app_home.calendar_attempts[0].cancelled);
        if sent {
            assert!(matches!(s.app_home.calendar_attempts[0].phase, Phase::Sent));
        } else {
            assert!(matches!(
                s.app_home.calendar_attempts[0].phase,
                Phase::Unsent
            ));
        }
    }
    let (mut s, origin) = setup();
    let id = queue(&mut s, &origin);
    assert!(claim(&mut s, id));
    let (release, mut released) = inputs::native::test_pair();
    s.app_home.calendar_attempts[0].native_lease = Some(release);
    retire_ui(&mut s);
    assert_eq!(released.try_recv(), Ok(()));
    assert!(s.app_home.calendar_attempts[0].cancelled);
    assert!(matches!(s.app_home.calendar_attempts[0].phase, Phase::Sent));
}
