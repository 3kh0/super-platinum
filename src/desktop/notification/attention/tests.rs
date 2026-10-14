use super::*;
use serde_json::json;
use std::{cell::Cell, rc::Rc};

fn owner() -> Owner {
    Owner {
        account: "fixture".into(),
        epoch: 3,
        team: "T1".into(),
        user: "U_SELF".into(),
        transport: 7,
        credential: 9,
    }
}

fn request_counter(
    counter: Rc<Cell<usize>>,
) -> impl FnOnce(DockAttention) -> Option<Box<dyn FnOnce()>> {
    move |_| Some(Box::new(move || counter.set(counter.get() + 1)))
}

#[test]
fn replacement_focus_retirement_and_drop_cancel_exactly_once() {
    let cancels = Rc::new(Cell::new(0));
    let mut controller = Controller::default();
    let first = controller
        .begin(owner(), DockAttention::UntilActive)
        .unwrap();
    controller.complete(
        first,
        Admission::Available,
        request_counter(cancels.clone()),
    );
    let next = controller.begin(owner(), DockAttention::Once).unwrap();
    controller.complete(next, Admission::Available, request_counter(cancels.clone()));
    assert_eq!(cancels.get(), 1);
    controller.maintain(true, |_| true);
    assert_eq!(cancels.get(), 2);
    controller.cancel();
    assert_eq!(cancels.get(), 2);
    let final_ticket = controller.begin(owner(), DockAttention::Once).unwrap();
    controller.complete(
        final_ticket,
        Admission::Available,
        request_counter(cancels.clone()),
    );
    drop(controller);
    assert_eq!(cancels.get(), 3);
}

#[test]
fn burst_reads_coalesce_and_old_focus_reads_cannot_replay_after_retirement() {
    let mut controller = Controller::default();
    let first = controller.begin(owner(), DockAttention::Once).unwrap();
    assert!(controller.begin(owner(), DockAttention::Once).is_none());
    controller.cancel();
    let second = controller.begin(owner(), DockAttention::Once).unwrap();
    controller.complete(first, Admission::Available, |_| {
        panic!("retired read started native attention")
    });
    assert_eq!(controller.pending.as_ref(), Some(&second));
    controller.complete(second, Admission::GloballyQuiet, |_| {
        panic!("quiet or unknown Focus started native attention")
    });
    assert!(controller.active.is_none() && controller.pending.is_none());
}

#[test]
fn continuous_attention_is_retained_until_owner_or_preference_changes() {
    let cancels = Rc::new(Cell::new(0));
    let mut controller = Controller::default();
    let first = controller
        .begin(owner(), DockAttention::UntilActive)
        .unwrap();
    controller.complete(
        first,
        Admission::Available,
        request_counter(cancels.clone()),
    );
    let next = controller
        .begin(owner(), DockAttention::UntilActive)
        .unwrap();
    controller.complete(next, Admission::Available, |_| {
        panic!("continuous attention was restarted")
    });
    controller.maintain(false, |_| true);
    assert_eq!(cancels.get(), 0);
    controller.maintain(false, |_| false);
    assert_eq!(cancels.get(), 1);
    assert!(controller.active.is_none());
}

fn core() -> CoreAppState {
    use super_platinum_core::{
        config::{Session, WorkspaceSession},
        slack::Transport,
        state::Workspace,
    };
    let mut core = CoreAppState::new(Default::default());
    let session = WorkspaceSession {
        team_id: "T1".into(),
        enterprise_id: None,
        user_id: "U_SELF".into(),
        name: "Fixture".into(),
        url: "https://example.invalid".into(),
        token: "fixture-token".into(),
    };
    let mut workspace = Workspace::from_session(&session);
    workspace.rt_generation = 1;
    workspace.notifications.apply_change(
        "all_notifications_prefs",
        json!({"global":{"global_desktop":"everything"},"channels":{}}),
    );
    workspace
        .notifications
        .apply_change("mac_ssb_bounce", json!("short"));
    core.workspaces.insert("T1".into(), workspace);
    core.active_account = Some("fixture".into());
    core.transport = Some(Arc::new(Transport::new("fixture-cookie").unwrap()));
    core.session = Some(Session {
        d_cookie: "fixture-cookie".into(),
        workspaces: [("T1".into(), session)].into(),
    });
    core
}

#[test]
fn account_credentials_and_preference_rotation_retire_owned_requests() {
    let mut core = core();
    let initial = Owner::capture(&core, "T1").unwrap();
    assert!(initial.current(&core, DockAttention::Once));
    core.session.as_mut().unwrap().d_cookie = "new-fixture-cookie".into();
    assert!(!initial.current(&core, DockAttention::Once));
    let current = Owner::capture(&core, "T1").unwrap();
    core.session
        .as_mut()
        .unwrap()
        .workspaces
        .get_mut("T1")
        .unwrap()
        .token = "new-fixture-token".into();
    assert!(!current.current(&core, DockAttention::Once));
    let current = Owner::capture(&core, "T1").unwrap();
    core.account_epoch += 1;
    assert!(!current.current(&core, DockAttention::Once));
    let current = Owner::capture(&core, "T1").unwrap();
    core.workspaces
        .get_mut("T1")
        .unwrap()
        .notifications
        .apply_change("mac_ssb_bounce", json!(""));
    assert!(!current.current(&core, DockAttention::Once));
}

#[test]
fn delayed_focus_read_rechecks_generation_delivery_and_read_mark() {
    let mut core = core();
    let ticket = Ticket {
        generation: 1,
        owner: Owner::capture(&core, "T1").unwrap(),
        mode: DockAttention::Once,
    };
    let event = super_platinum_core::slack::realtime::parse_event(
        r#"{"type":"message","channel":"C1","user":"U1","text":"hello <@U_SELF>","ts":"200.0"}"#,
    )
    .unwrap();
    let message = message_event(&event).unwrap();
    assert!(still_eligible(&core, &ticket, 1, message));
    assert!(!still_eligible(&core, &ticket, 0, message));
    let workspace = core.workspaces.get_mut("T1").unwrap();
    workspace.messages.entry("C1".into()).or_default().last_read = Some("200.0".into());
    assert!(!still_eligible(&core, &ticket, 1, message));
    let workspace = core.workspaces.get_mut("T1").unwrap();
    workspace.messages.get_mut("C1").unwrap().last_read = None;
    workspace
        .notifications
        .global
        .insert("global_desktop_push_enabled".into(), json!(false));
    assert!(!still_eligible(&core, &ticket, 1, message));
}

#[test]
fn a_retired_new_candidate_does_not_cancel_existing_attention() {
    let cancels = Rc::new(Cell::new(0));
    let mut controller = Controller::default();
    let first = controller
        .begin(owner(), DockAttention::UntilActive)
        .unwrap();
    controller.complete(
        first,
        Admission::Available,
        request_counter(cancels.clone()),
    );
    let next = controller
        .begin(owner(), DockAttention::UntilActive)
        .unwrap();
    controller.complete(next, Admission::CandidateRetired, |_| {
        panic!("retired message bounced")
    });
    assert!(controller.active.is_some());
    assert_eq!(cancels.get(), 0);
    let next = controller
        .begin(owner(), DockAttention::UntilActive)
        .unwrap();
    controller.complete(next, Admission::GloballyQuiet, |_| {
        panic!("quiet Focus bounced")
    });
    assert_eq!(cancels.get(), 1);
}

#[test]
fn burst_keeps_latest_unread_candidate_without_another_focus_read() {
    let core = core();
    let owner = Owner::capture(&core, "T1").unwrap();
    let mut first = Message {
        channel: Some("C1".into()),
        ts: Some("200.0".into()),
        user: Some("U1".into()),
        text: Some("hello <@U_SELF>".into()),
        ..Default::default()
    };
    let mut controller = Controller::default();
    let ticket = controller
        .queue(owner.clone(), DockAttention::Once, 1, &first)
        .unwrap();
    first.ts = Some("201.0".into());
    assert!(
        controller
            .queue(owner, DockAttention::Once, 1, &first)
            .is_none()
    );
    assert_eq!(controller.pending.as_ref(), Some(&ticket));
    assert_eq!(
        controller.candidate.as_ref().unwrap().1.ts.as_deref(),
        Some("201.0")
    );
    let mut core = core;
    core.workspaces
        .get_mut("T1")
        .unwrap()
        .messages
        .entry("C1".into())
        .or_default()
        .last_read = Some("200.0".into());
    let (generation, message) = controller.candidate.as_ref().unwrap();
    assert!(still_eligible(&core, &ticket, *generation, message));
}
