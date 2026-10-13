use super::*;
use crate::{media::MediaRegistry, state::ChannelTab};
use serde_json::json;
use super_platinum_core::slack::{events::RtEvent, models::Message};

const TS: &str = "1719800000.000001";

fn message() -> Message {
    serde_json::from_value(json!({
        "ts":TS,"channel":"C2","bot_id":"B1",
        "bot_profile":{"id":"B1","app_id":"A1","team_id":"T1"},
        "blocks":[{"type":"actions","block_id":"review","elements":[{
            "type":"button","action_id":"approve","text":{"type":"plain_text","text":"Approve"},
            "value":"item-1", "confirm":{
                "title":{"type":"plain_text","text":"Confirm"},
                "text":{"type":"mrkdwn","text":"Approve *this* item?"},
                "confirm":{"type":"plain_text","text":"Approve"},
                "deny":{"type":"plain_text","text":"Cancel"},"style":"danger"
            }
        }]}]
    }))
    .unwrap()
}

fn shell() -> ShellState {
    let mut shell = ShellState::fixture(MediaRegistry::default());
    shell.channel_tab = ChannelTab::Messages;
    shell.overlay = None;
    shell.toast = None;
    shell.app_modal = None;
    shell
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .messages
        .entry("C2".into())
        .or_default()
        .upsert(message());
    shell
}

fn canonical(shell: &mut ShellState) -> &mut Message {
    shell
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .messages
        .get_mut("C2")
        .unwrap()
        .messages
        .iter_mut()
        .find(|m| m.ts.as_deref() == Some(TS))
        .unwrap()
}

fn captured(shell: &ShellState) -> Captured {
    let row = crate::message_vm::message_vm(&shell.core.workspaces["T1"], &message(), &shell.media);
    let source = Source::capture(shell, &row, "C2", source::Bag::Timeline).unwrap();
    source.capture_button(shell, "review", "approve").unwrap()
}

fn operation(
    shell: &mut ShellState,
    phase: Phase,
) -> (Captured, Key, u64, crate::app_modal::ActionToken) {
    let captured = captured(shell);
    let key = Key::new(captured.source(), captured.metadata()).unwrap();
    let generation = next_id();
    let token = crate::app_modal::register_action(shell, captured.app_id()).unwrap();
    shell.message_buttons.operations.insert(
        key.clone(),
        Operation {
            captured: captured.clone(),
            phase,
            generation,
        },
    );
    (captured, key, generation, token)
}

fn token_count(shell: &ShellState) -> usize {
    shell.app_modal.as_ref().unwrap().modal.token_count()
}

fn context(shell: &mut ShellState) -> Context {
    use super_platinum_core::{
        config::{Session, WorkspaceSession},
        slack::Transport,
    };
    shell.core.session = Some(Session {
        d_cookie: "fixture".into(),
        workspaces: [(
            "T1".into(),
            WorkspaceSession {
                team_id: "T1".into(),
                enterprise_id: None,
                user_id: shell.core.workspaces["T1"].self_user_id.clone(),
                name: "Fixture".into(),
                url: "https://example.invalid".into(),
                token: String::new(),
            },
        )]
        .into(),
    });
    shell.core.transport = Some(Arc::new(Transport::new("fixture").unwrap()));
    Context::capture(shell).unwrap()
}

fn open(shell: &mut ShellState, token: &str, app: &str) {
    let event:RtEvent=RtEvent::Unknown(serde_json::from_value(json!({
        "type":"view_opened","view_id":"V1","view_type":"modal","app_id":app,"client_token":token,
        "view":{"id":"V1","type":"modal","app_id":app,"title":{"type":"plain_text","text":"Review"},"blocks":[]}
    })).unwrap());
    let generation = shell.core.workspaces["T1"].rt_generation;
    crate::app_modal::realtime(shell, "T1", generation, &event);
}

#[test]
fn rendered_metadata_must_match_the_current_canonical_button() {
    let shell = shell();
    let captured = captured(&shell);
    assert!(capture(&shell, captured.source(), captured.metadata()).is_ok());
    for change in 0..4 {
        let mut metadata = captured.metadata().clone();
        match change {
            0 => metadata.value = Some("another-item".into()),
            1 => metadata.block_id = None,
            2 => metadata.action_id = Some("another-action".into()),
            _ => {
                metadata.source = BlockElementSource::UnsupportedControl {
                    kind: "static_select".into(),
                }
            }
        }
        assert!(capture(&shell, captured.source(), &metadata).is_err());
        assert!(!presentation_for_button(&shell, captured.source(), &metadata).enabled);
    }
}

#[test]
fn confirmation_carries_app_copy_and_cancellation_does_not_register_or_send() {
    let mut shell = shell();
    let captured = captured(&shell);
    shell.message_buttons.confirmation = Some(Pending {
        input_revision: None,
        captured,
        revision: 42,
    });
    shell.overlay = Some(Overlay::ButtonConfirmation);
    let shown = confirmation(&shell).unwrap();
    assert_eq!(shown.title.text, "Confirm");
    assert_eq!(shown.text.text, "Approve *this* item?");
    assert_eq!(shown.confirm_label.text, "Approve");
    assert_eq!(shown.cancel_label.text, "Cancel");
    assert_eq!(shown.style.as_deref(), Some("danger"));
    assert_eq!(shown.revision, 42);
    assert!(!shown.busy);
    cancel_local(&mut shell);
    assert!(confirmation(&shell).is_none());
    assert!(shell.overlay.is_none());
    assert!(shell.message_buttons.operations.is_empty());
    assert!(shell.app_modal.is_none());
}

#[test]
fn edited_confirmation_is_disabled_then_retired_without_dismissing_other_overlays() {
    let mut shell = shell();
    let captured = captured(&shell);
    shell.message_buttons.confirmation = Some(Pending {
        input_revision: None,
        captured: captured.clone(),
        revision: 42,
    });
    shell.overlay = Some(Overlay::ButtonConfirmation);
    canonical(&mut shell).blocks[0]["elements"][0]["value"] = json!("changed");
    assert!(confirmation(&shell).unwrap().busy);
    assert!(poll(&mut shell));
    assert!(shell.message_buttons.confirmation.is_none());
    assert!(shell.overlay.is_none());
    assert!(shell.toast.as_ref().unwrap().text.contains("changed"));
    shell.message_buttons.confirmation = Some(Pending {
        input_revision: None,
        captured,
        revision: 43,
    });
    shell.overlay = Some(Overlay::Settings);
    poll(&mut shell);
    assert!(shell.message_buttons.confirmation.is_none());
    assert!(shell.overlay == Some(Overlay::Settings));
}

#[test]
fn pending_and_uncertain_operations_disable_the_same_action_across_new_visits() {
    let mut shell = shell();
    let (old, key, generation, token) = operation(&mut shell, Phase::Pending);
    let shown = presentation_for_button(&shell, old.source(), old.metadata());
    assert!(!shown.enabled && shown.busy && !shown.uncertain);
    finish(
        &mut shell,
        &key,
        generation,
        &token,
        Err(Error::Api("unexpected_response".into())),
    );
    let shown = presentation_for_button(&shell, old.source(), old.metadata());
    assert!(!shown.enabled && !shown.busy && shown.uncertain);
    assert_eq!(token_count(&shell), 1);
    shell.channel_generation += 1;
    shell.core.workspaces.get_mut("T1").unwrap().rt_generation += 1;
    let fresh = captured(&shell);
    let shown = presentation_for_button(&shell, fresh.source(), fresh.metadata());
    assert!(!shown.enabled && shown.uncertain);
}

#[test]
fn stale_generation_completion_cannot_remove_a_newer_operation_or_its_token() {
    let mut shell = shell();
    let (_, key, generation, token) = operation(&mut shell, Phase::Pending);
    finish(
        &mut shell,
        &key,
        generation.wrapping_add(1),
        &token,
        Ok(api::BlockActionResponse::Accepted),
    );
    assert_eq!(
        shell.message_buttons.operations[&key].generation,
        generation
    );
    assert_eq!(token_count(&shell), 1);
    assert!(shell.toast.is_none());
}

#[test]
fn accepted_acknowledgement_retains_token_for_the_later_matching_modal() {
    let mut shell = shell();
    let (_, key, generation, token) = operation(&mut shell, Phase::Pending);
    finish(
        &mut shell,
        &key,
        generation,
        &token,
        Ok(api::BlockActionResponse::Accepted),
    );
    assert!(shell.message_buttons.operations.is_empty());
    assert_eq!(token_count(&shell), 1);
    open(&mut shell, "unrelated-token", "A1");
    assert!(shell.overlay.is_none());
    open(&mut shell, &token.token, "A_OTHER");
    assert!(shell.overlay.is_none());
    open(&mut shell, &token.token, "A1");
    assert!(shell.overlay == Some(Overlay::AppModal));
    assert_eq!(
        shell
            .app_modal
            .as_ref()
            .unwrap()
            .current_form()
            .unwrap()
            .view_id,
        "V1"
    );
}

#[test]
fn app_rejection_and_proven_unsent_failure_release_the_action_and_reject_tokens() {
    for unsent in [false, true] {
        let mut shell = shell();
        let (captured, key, generation, token) = operation(&mut shell, Phase::Pending);
        let response = if unsent {
            Err(Error::Api("block_interaction_not_sent_offline".into()))
        } else {
            Ok(api::BlockActionResponse::Rejected(
                api::BlockActionRejection {
                    error: "invalid_action".into(),
                    message: Some("Please review the item".into()),
                },
            ))
        };
        finish(&mut shell, &key, generation, &token, response);
        assert!(shell.message_buttons.operations.is_empty());
        assert_eq!(token_count(&shell), 0);
        assert!(presentation_for_button(&shell, captured.source(), captured.metadata()).enabled);
        assert_eq!(
            shell.toast.as_ref().unwrap().text,
            if unsent {
                "Reconnect to use this app action. It has not been sent."
            } else {
                "Please review the item"
            }
        );
        open(&mut shell, &token.token, "A1");
        assert!(shell.overlay.is_none());
    }
}

#[test]
fn completion_after_navigation_updates_bookkeeping_without_toasting_another_view() {
    let mut shell = shell();
    let (_, key, generation, token) = operation(&mut shell, Phase::Pending);
    shell.core.active_channel = Some("C1".into());
    shell.channel_generation += 1;
    finish(
        &mut shell,
        &key,
        generation,
        &token,
        Ok(api::BlockActionResponse::Rejected(
            api::BlockActionRejection {
                error: "rejected".into(),
                message: Some("Other channel app response".into()),
            },
        )),
    );
    assert!(shell.message_buttons.operations.is_empty());
    assert_eq!(token_count(&shell), 0);
    assert!(shell.toast.is_none());
}

#[test]
fn realtime_open_before_acknowledgement_is_preserved() {
    let mut shell = shell();
    let (_, key, generation, token) = operation(&mut shell, Phase::Pending);
    open(&mut shell, &token.token, "A1");
    assert!(shell.overlay == Some(Overlay::AppModal));
    finish(
        &mut shell,
        &key,
        generation,
        &token,
        Ok(api::BlockActionResponse::Accepted),
    );
    assert!(shell.message_buttons.operations.is_empty());
    assert!(shell.overlay == Some(Overlay::AppModal));
    assert_eq!(shell.app_modal.as_ref().unwrap().modal.stack().len(), 1);
}

#[test]
fn account_replacement_discards_operation_bookkeeping() {
    let mut shell = shell();
    operation(&mut shell, Phase::Uncertain);
    shell.core.account_epoch += 1;
    poll(&mut shell);
    assert!(shell.message_buttons.operations.is_empty());
}

#[test]
fn generic_offline_and_transport_failures_are_not_proof_that_nothing_was_sent() {
    for error in [
        Error::Offline("socket closed after write".into()),
        Error::Transport("response unreadable".into()),
        Error::RateLimited {
            retry_after_secs: Some(5),
        },
    ] {
        let mut shell = shell();
        let (captured, key, generation, token) = operation(&mut shell, Phase::Pending);
        finish(&mut shell, &key, generation, &token, Err(error));
        let shown = presentation_for_button(&shell, captured.source(), captured.metadata());
        assert!(!shown.enabled && shown.uncertain);
        assert_eq!(token_count(&shell), 1);
    }
}

#[test]
fn reconnect_invalidates_old_modal_correlation_even_if_the_action_remains_uncertain() {
    let mut shell = shell();
    let (_, _, _, token) = operation(&mut shell, Phase::Uncertain);
    shell.core.workspaces.get_mut("T1").unwrap().rt_generation += 1;
    crate::app_modal::poll(&mut shell);
    assert!(shell.app_modal.is_none());
    open(&mut shell, &token.token, "A1");
    assert!(shell.app_modal.is_none());
    assert!(shell.overlay.is_none());
    assert_eq!(shell.message_buttons.operations.len(), 1);
}

#[test]
fn evicting_the_original_bag_does_not_unlock_an_uncertain_action_in_a_thread() {
    let mut shell = shell();
    operation(&mut shell, Phase::Uncertain);
    let root = "1719800000.000000";
    let mut reply = message();
    reply.thread_ts = Some(root.into());
    let row = crate::message_vm::message_vm(&shell.core.workspaces["T1"], &reply, &shell.media);
    shell
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .messages
        .remove("C2");
    shell.thread_root = Some(root.into());
    shell
        .core
        .threads
        .entry(("T1".into(), "C2".into(), root.into()))
        .or_default()
        .upsert(reply);
    let source = Source::capture(
        &shell,
        &row,
        "C2",
        source::Bag::Thread { root: root.into() },
    )
    .unwrap();
    let captured = source.capture_button(&shell, "review", "approve").unwrap();
    let shown = presentation_for_button(&shell, &source, captured.metadata());
    assert!(!shown.enabled && shown.uncertain);
}

#[test]
fn dispatch_preflight_survives_navigation_but_rechecks_account_transport_and_source() {
    use super_platinum_core::slack::Transport;
    for case in 0..7 {
        let mut shell = shell();
        let context = context(&mut shell);
        let (_, key, generation, token) = operation(&mut shell, Phase::Pending);
        match case {
            0 => {
                shell.core.active_channel = Some("C1".into());
                shell.channel_generation += 1;
            }
            1 => shell.core.account_epoch += 1,
            2 => shell.core.transport = Some(Arc::new(Transport::new("replacement").unwrap())),
            3 => shell.core.active_team = Some("T_OTHER".into()),
            4 => canonical(&mut shell).blocks[0]["elements"][0]["value"] = json!("changed"),
            5 => {
                shell
                    .core
                    .workspaces
                    .get_mut("T1")
                    .unwrap()
                    .messages
                    .remove("C2");
            }
            _ => shell.core.workspaces.get_mut("T1").unwrap().rt_generation += 1,
        }
        assert_eq!(
            begin_dispatch(&mut shell, &context, &key, generation, &token),
            case == 0
        );
        assert_eq!(
            shell.message_buttons.operations.contains_key(&key),
            case == 0
        );
        assert_eq!(token_count(&shell), usize::from(case == 0));
        assert!(shell.toast.is_none());
    }
}

#[test]
fn stale_presend_task_cannot_clear_a_newer_or_uncertain_operation() {
    for uncertain in [false, true] {
        let mut shell = shell();
        let context = context(&mut shell);
        let (_, key, generation, token) = operation(&mut shell, Phase::Pending);
        let expected_generation = if uncertain {
            generation
        } else {
            generation + 1
        };
        let operation = shell.message_buttons.operations.get_mut(&key).unwrap();
        operation.generation = expected_generation;
        if uncertain {
            operation.phase = Phase::Uncertain;
        }
        assert!(!begin_dispatch(
            &mut shell, &context, &key, generation, &token
        ));
        assert_eq!(
            shell.message_buttons.operations[&key].generation,
            expected_generation
        );
        assert_eq!(token_count(&shell), usize::from(uncertain));
    }
}

#[test]
fn a_displaced_modal_stack_does_not_swallow_the_next_message_button_registration() {
    let mut shell = shell();
    let first = crate::app_modal::register_action(&mut shell, "A1").unwrap();
    open(&mut shell, &first.token, "A1");
    assert!(shell.overlay == Some(Overlay::AppModal));
    assert!(crate::app_modal::register_action(&mut shell, "A1").is_none());
    shell.overlay = None;
    let second = crate::app_modal::register_action(&mut shell, "A1").unwrap();
    assert_ne!(first.token, second.token);
    assert!(shell.app_modal.as_ref().unwrap().modal.stack().is_empty());
    assert_eq!(token_count(&shell), 1);
    open(&mut shell, &first.token, "A1");
    assert!(shell.overlay.is_none());
    open(&mut shell, &second.token, "A1");
    assert!(shell.overlay == Some(Overlay::AppModal));
}
