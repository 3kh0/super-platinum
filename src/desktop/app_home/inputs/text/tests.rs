//! Pure serial-shell regressions. Occurrences here are test data; these tests do
//! not prove native DOM occurrence, focus, text leases, timers, or HTTP delivery.
use super::*;
use serde_json::{Value, json};
use super_platinum_core::slack::client::RequestBody;

fn setup() -> ShellState {
    let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
    fixture::install(&mut s);
    for block in ["enter", "characters"] {
        mount(&mut s, block);
    }
    assert!(ready(&s));
    s
}
fn binding(s: &ShellState, block: &str) -> Binding {
    let h = s.app_home.surface.as_ref().unwrap();
    Binding {
        owner: h.owner.clone(),
        form: owner(s).unwrap(),
        visit: actions::Key {
            visit: h.id,
            revision: h.revision,
        },
        key: fixture::key(block),
        raw: h.view.as_ref().unwrap().raw().clone(),
    }
}
fn mount(s: &mut ShellState, block: &str) {
    let b = binding(s, block);
    let InputValue::Text(value) = &form(s).unwrap().fields[&b.key].value else {
        panic!("text")
    };
    let value = value.clone();
    s.app_home.text.mounts.insert(
        (b.form.clone(), b.key.clone()),
        Mount {
            id: next(),
            seq: 0,
            value: value.clone(),
            binding: b.clone(),
        },
    );
    assert!(emit(s, &b, "mounted", &value).is_none());
}
fn occurrence(s: &ShellState, b: &Binding, kind: &str, value: &str) -> Occurrence {
    let m = &s.app_home.text.mounts[&(b.form.clone(), b.key.clone())];
    Occurrence {
        mount: m.id,
        seq: m.seq + 1,
        kind: kind.into(),
        value: value.into(),
        key: (kind == "enter").then(|| "Enter".into()),
        code: None,
        ctrl: false,
        alt: false,
        shift: false,
        meta: false,
        repeat: false,
        composing: false,
        trusted: false,
        prevented: kind == "enter",
        number: None,
        email: None,
    }
}
fn emit(s: &mut ShellState, b: &Binding, kind: &str, value: &str) -> Option<u64> {
    let e = occurrence(s, b, kind, value);
    receive(s, b, &e)
}
fn queued(s: &ShellState, b: &Binding) -> Queued {
    s.app_home.text.queued[&(b.form.clone(), b.key.clone())].clone()
}
fn state_value(s: &ShellState, b: &Binding) -> Value {
    form(s).unwrap().state().unwrap()["values"][&b.key.block_id][&b.key.action_id]["value"].clone()
}

#[test]
fn queued_literal_survives_normal_and_invalid_edits_but_prepares_latest_state() {
    let mut s = setup();
    let b = binding(&s, "enter");
    let id = emit(&mut s, &b, "enter", " alpha ").unwrap();
    assert!(emit(&mut s, &b, "input", " beta ").is_none());
    assert_eq!(queued(&s, &b).id, id);
    assert_eq!(state_value(&s, &b), "beta");
    assert!(emit(&mut s, &b, "enter", "").is_none());
    let q = queued(&s, &b);
    assert_eq!(q.id, id);
    assert!(q.current(&s));
    assert_eq!(state_value(&s, &b), Value::Null);
    assert!(form(&s).unwrap().fields[&b.key].error.is_some());
    let current = s.app_home.surface.as_ref().unwrap().view.as_ref().unwrap();
    let request = q
        .action
        .prepare_with_context(
            &b.owner.ctx.client,
            &b.owner.ctx.session,
            current,
            form(&s).unwrap(),
            "synthetic-event",
            q.route.team_id(),
        )
        .unwrap();
    let RequestBody::Form(fields) = request.body else {
        panic!("form")
    };
    let fields: BTreeMap<_, _> = fields.into_iter().collect();
    assert_eq!(
        serde_json::from_str::<Value>(&fields["actions"]).unwrap()[0]["value"],
        " alpha "
    );
    assert_eq!(
        serde_json::from_str::<Value>(&fields["state"]).unwrap()["values"]["enter"]["value"]["value"],
        Value::Null
    );
}

#[test]
fn trim_equal_change_keeps_literal_parent_wire_and_queued_occurrence() {
    let mut s = setup();
    let b = binding(&s, "enter");
    let id = emit(&mut s, &b, "enter", " alpha ").unwrap();
    assert!(emit(&mut s, &b, "input", "  alpha  ").is_none());
    assert_eq!(queued(&s, &b).id, id);
    assert_eq!(state_value(&s, &b), " alpha ");
    assert_eq!(
        form(&s).unwrap().fields[&b.key].value,
        InputValue::Text("  alpha  ".into())
    );
    assert!(ready(&s));
}

#[test]
fn each_field_replaces_only_its_queue_and_pending_activity_does_not_reject_new_event() {
    let mut s = setup();
    let enter = binding(&s, "enter");
    let chars = binding(&s, "characters");
    let first = emit(&mut s, &enter, "enter", "first").unwrap();
    let other = emit(&mut s, &chars, "input", "  notes  ").unwrap();
    assert_eq!(queued(&s, &chars).action.action()["value"], "notes");
    let route = context::Witness::capture(&s).unwrap();
    s.app_home.text.activity.insert(
        (enter.form.clone(), enter.key.block_id.clone()),
        Activity {
            binding: enter.clone(),
            route,
            loading: true,
            error: None,
        },
    );
    let second = emit(&mut s, &enter, "enter", "second").unwrap();
    assert_ne!(first, second);
    assert_eq!(queued(&s, &chars).id, other);
    assert_eq!(s.app_home.text.queued.len(), 2);
    assert!(s.app_home.text.activity[&(enter.form.clone(), enter.key.block_id.clone())].loading);
    assert!(queued(&s, &enter).current(&s));
}

#[test]
fn duplicate_gap_and_foreign_mount_occurrences_cannot_change_parent_or_queue() {
    let mut s = setup();
    let b = binding(&s, "enter");
    let id = emit(&mut s, &b, "enter", "kept").unwrap();
    let before = state_value(&s, &b);
    let event = occurrence(&s, &b, "enter", "foreign");
    // Each rejected occurrence must leave the next exact sequence available.
    let mut duplicate = event.clone();
    duplicate.seq -= 1;
    assert!(receive(&mut s, &b, &duplicate).is_none());
    let mut gap = event.clone();
    gap.seq += 1;
    assert!(receive(&mut s, &b, &gap).is_none());
    let mut foreign = event.clone();
    foreign.mount = next();
    assert!(receive(&mut s, &b, &foreign).is_none());
    assert_eq!(state_value(&s, &b), before);
    assert_eq!(queued(&s, &b).id, id);
    assert!(receive(&mut s, &b, &event).is_some());
}

#[test]
fn replacing_native_mount_retires_old_queue_without_losing_form() {
    let mut s = setup();
    let b = binding(&s, "enter");
    emit(&mut s, &b, "enter", "retained").unwrap();
    let old = queued(&s, &b);
    mount(&mut s, "enter");
    assert!(!old.current(&s));
    assert!(poll(&mut s));
    assert!(s.app_home.text.queued.is_empty());
    assert_eq!(state_value(&s, &b), "retained");
    assert!(ready(&s));
    let current = binding(&s, "enter");
    assert!(emit(&mut s, &current, "enter", "fresh").is_some());
}

#[test]
fn source_context_socket_account_and_view_retirement_refuse_queued_delivery() {
    for retirement in ["source", "context", "socket", "account", "view"] {
        let mut s = setup();
        let b = binding(&s, "enter");
        emit(&mut s, &b, "enter", "retained").unwrap();
        let q = queued(&s, &b);
        let team = s.core.active_team.clone().unwrap();
        match retirement {
            "source" => {
                let h = s.app_home.surface.as_mut().unwrap();
                let mut raw = h.view.as_ref().unwrap().raw().clone();
                raw["hash"] = json!("replacement");
                h.view = Some(View::decode(h.home.as_ref().unwrap(), raw).unwrap());
                h.revision = next();
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
            "view" => s.app_home.surface.as_mut().unwrap().id = next(),
            _ => unreachable!(),
        }
        assert!(!q.current(&s), "{retirement}");
        assert!(poll(&mut s));
        assert!(s.app_home.text.queued.is_empty());
        assert_eq!(
            s.app_home.forms[&b.form].fields[&b.key].value,
            InputValue::Text("retained".into())
        );
    }
}

#[test]
fn required_error_and_unknown_event_never_replace_valid_queue() {
    let mut s = setup();
    let b = binding(&s, "enter");
    let id = emit(&mut s, &b, "enter", "valid").unwrap();
    assert!(emit(&mut s, &b, "input", "x").is_none());
    assert!(form(&s).unwrap().fields[&b.key].error.is_some());
    assert_eq!(queued(&s, &b).id, id);
    assert!(emit(&mut s, &b, "not-a-text-event", "unaccepted").is_none());
    assert_eq!(state_value(&s, &b), "x");
    assert!(!ready(&s));
    assert_eq!(queued(&s, &b).id, id);
}

#[test]
fn cooldown_expiry_requests_one_render_transition_without_sleeping() {
    let mut s = setup();
    let b = binding(&s, "enter");
    let cooldown = (b.owner.ctx.epoch, b.owner.ctx.session.team_id.clone());
    assert!(!poll(&mut s));
    s.app_home.cooldowns.insert(
        cooldown.clone(),
        Some(Instant::now() + Duration::from_secs(60)),
    );
    assert!(poll(&mut s));
    assert!(s.app_home.text.was_cooling);
    assert!(!b.interactive(&s));
    assert!(!poll(&mut s));
    s.app_home
        .cooldowns
        .insert(cooldown, Some(Instant::now() - Duration::from_secs(1)));
    assert!(poll(&mut s));
    assert!(!s.app_home.text.was_cooling);
    assert!(b.interactive(&s));
    assert!(!poll(&mut s));
}

#[test]
fn cooldown_publication_remembers_disabled_state_when_first_poll_is_after_expiry() {
    let mut s = setup();
    let b = binding(&s, "enter");
    let cooldown = (b.owner.ctx.epoch, b.owner.ctx.session.team_id.clone());
    assert!(!s.app_home.text.was_cooling);
    failure(
        &mut s,
        &b.owner.ctx,
        &Error::RateLimited {
            retry_after_secs: Some(60),
        },
    );
    assert!(s.app_home.text.was_cooling);
    assert!(!b.interactive(&s));
    // No poll during the active deadline: publication itself must retain this state.
    s.app_home
        .cooldowns
        .insert(cooldown, Some(Instant::now() - Duration::from_secs(1)));
    assert!(poll(&mut s));
    assert!(!s.app_home.text.was_cooling);
    assert!(b.interactive(&s));
    assert!(!poll(&mut s));
}

#[test]
fn direct_due_text_retry_preserves_action_while_received_siblings_advance() {
    for sibling in ["enter", "quiet"] {
        let mut s = setup();
        super::super::native::seed(&mut s);
        let b = binding(&s, "enter");
        emit(&mut s, &b, "enter", " queued literal ").unwrap();
        let q = queued(&s, &b);
        // Delivery removes this slot before acquisition; progress must not
        // require a queued-map entry or manufacture a replacement action.
        s.app_home
            .text
            .queued
            .remove(&(b.form.clone(), b.key.clone()));
        let attempted = button::RetrySnapshot::capture(&s, b.visit).unwrap();
        assert_eq!(attempted.progress(&s), Some(false));
        if sibling == "enter" {
            assert!(emit(&mut s, &b, "input", "latest parent").is_none());
        } else {
            assert!(super::super::set_local(
                &mut s,
                b.visit,
                &fixture::key("quiet"),
                InputValue::Text("latest sibling".into())
            ));
        }
        assert!(q.current(&s));
        assert_eq!(attempted.progress(&s), Some(true));
        assert!(s.app_home.text.queued.is_empty());
        assert_eq!(q.action.action()["value"], " queued literal ");
        let current = s.app_home.surface.as_ref().unwrap().view.as_ref().unwrap();
        let request = q
            .action
            .prepare_with_context(
                &b.owner.ctx.client,
                &b.owner.ctx.session,
                current,
                form(&s).unwrap(),
                "synthetic-direct-retry",
                q.route.team_id(),
            )
            .unwrap();
        let RequestBody::Form(fields) = request.body else {
            panic!("form")
        };
        let fields: BTreeMap<_, _> = fields.into_iter().collect();
        let state: Value = serde_json::from_str(&fields["state"]).unwrap();
        assert_eq!(
            state["values"][sibling]["value"]["value"],
            if sibling == "enter" {
                "latest parent"
            } else {
                "latest sibling"
            }
        );
    }
}

#[test]
fn direct_due_text_retry_does_not_convert_mismatch_or_retirement_into_progress() {
    for mismatch in [
        "unchanged",
        "unreceived",
        "remount",
        "source",
        "account",
        "socket",
        "visit",
    ] {
        let mut s = setup();
        super::super::native::seed(&mut s);
        let b = binding(&s, "enter");
        emit(&mut s, &b, "enter", "queued").unwrap();
        let q = queued(&s, &b);
        let attempted = button::RetrySnapshot::capture(&s, b.visit).unwrap();
        match mismatch {
            "unreceived" => {
                // The sequence advances but no accepted parent callback matches
                // this buffer: readiness must reject it, not treat it as progress.
                assert!(emit(&mut s, &b, "unknown", "unaccepted").is_none());
            }
            "remount" => mount(&mut s, "enter"),
            "source" => s.app_home.surface.as_mut().unwrap().revision = next(),
            "account" => s.core.account_epoch += 1,
            "socket" => s.core.workspaces.get_mut("T1").unwrap().rt_generation += 1,
            "visit" => s.app_home.surface.as_mut().unwrap().id = next(),
            _ => {}
        }
        assert!(!attempted.progressed(&s), "{mismatch}");
        if !matches!(mismatch, "unchanged" | "unreceived") {
            assert!(!q.current(&s), "{mismatch}");
        }
        assert_eq!(queued(&s, &b).id, q.id);
        assert_eq!(q.action.action()["value"], "queued");
    }
}
