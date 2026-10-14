use super::*;
use crate::app_modal::{multi_conversation_fixture as fixture, requests, users};
use crate::model::SurfaceTarget;
use serde_json::json;

fn shell() -> ShellState {
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    fixture::install(&mut shell);
    shell
}
fn key() -> InputKey {
    InputKey {
        block_id: "reviewers".into(),
        action_id: "value".into(),
    }
}
fn viewed_value(shell: &ShellState) -> InputValue {
    shell
        .app_modal
        .as_ref()
        .unwrap()
        .current_form()
        .unwrap()
        .fields[&key()]
        .value
        .clone()
}

#[test]
fn production_registration_correlates_actual_view_separately_from_source_channel() {
    let mut shell = shell();
    shell.overlay = None;
    shell.app_modal = None;
    let token =
        crate::app_modal::register_viewed_action(&mut shell, "A_FIXTURE", Some("C00000001".into()))
            .unwrap();
    shell.core.active_channel = Some("C00000002".into());
    shell.refresh_from_core();
    let event = serde_json::from_value(
        json!({"type":"view_opened","view_id":"V_MODAL_USERS_FIXTURE",
        "app_id":"A_FIXTURE","view_type":"modal","client_token":token.token}),
    )
    .unwrap();
    let scope = crate::app_modal::scope(&shell).unwrap();
    let session = shell.app_modal.as_mut().unwrap();
    assert_eq!(
        session
            .modal
            .apply(&scope, &event, crate::app_modal::now_ms()),
        ModalChange::Opened
    );
    let view = &session.modal.stack()[0];
    assert_eq!(view.origin_conversation.as_deref(), Some("C00000001"));
    assert_eq!(view.viewed_conversation.as_ref().unwrap().id(), "C00000006");
}

#[test]
fn capture_matches_rendered_surface_not_hidden_or_stale_active_channel() {
    let mut shell = shell();
    assert!(capture(&shell).is_none()); // Modal overlay covers the conversation.
    shell.overlay = None;
    assert_eq!(capture(&shell).unwrap().id(), "C00000006");
    for surface in [
        MainView::Unreads,
        MainView::Threads,
        MainView::Files,
        MainView::Scheduled,
    ] {
        shell.main_view = surface;
        assert!(capture(&shell).is_none());
    }
    for surface in [MainView::Dms, MainView::Activity, MainView::Later] {
        shell.main_view = surface;
        shell.surfaces.remove(&surface);
        assert!(capture(&shell).is_none());
        shell.surfaces.insert(
            surface,
            SurfaceTarget {
                channel: "C00000001".into(),
                thread_root: None,
            },
        );
        assert!(capture(&shell).is_none());
        shell.surfaces.get_mut(&surface).unwrap().channel = "C00000006".into();
        assert_eq!(capture(&shell).unwrap().id(), "C00000006");
        shell.thread_root = Some("1719800400.000000".into());
        assert_eq!(capture(&shell).unwrap().id(), "C00000006");
    }
    shell.main_view = MainView::Home;
    shell.channel_tab = ChannelTab::Files;
    assert!(capture(&shell).is_none());
    shell.channel_tab = ChannelTab::Messages;
    shell.core.active_channel = Some("C00000001".into());
    assert!(capture(&shell).is_none()); // Projection has not switched yet.
    shell.refresh_from_core();
    assert_eq!(capture(&shell).unwrap().id(), "C00000001");
    shell
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .channels
        .get_mut("C00000001")
        .unwrap()
        .is_im = true;
    assert!(capture(&shell).is_none());
}

#[test]
fn frozen_viewed_default_overrides_source_initial_and_navigation_but_not_saved_clear() {
    let mut shell = shell();
    assert_eq!(
        viewed_value(&shell),
        InputValue::Conversations(vec!["C00000006".into()])
    );
    shell.core.active_channel = Some("C00000002".into());
    shell.refresh_from_core();
    assert!(requests::prepare_submission(&shell).is_ok());
    let mut raw = fixture::view();
    raw["blocks"][1]["element"]["initial_conversations"] = json!(["G00000003"]);
    shell
        .app_modal
        .as_mut()
        .unwrap()
        .current_form_mut()
        .unwrap()
        .reconcile(&raw)
        .unwrap();
    assert_eq!(
        viewed_value(&shell),
        InputValue::Conversations(vec!["C00000006".into()])
    );
    raw["blocks"][1]["optional"] = json!(true);
    raw["state"] = json!({"values":{"reviewers":{"value":{"type":"multi_conversations_select","selected_conversations":[]}}}});
    shell
        .app_modal
        .as_mut()
        .unwrap()
        .current_form_mut()
        .unwrap()
        .reconcile(&raw)
        .unwrap();
    assert_eq!(viewed_value(&shell), InputValue::Conversations(vec![]));
    assert_eq!(
        requests::prepare_submission(&shell).unwrap()["values"]["reviewers"]["value"],
        json!({"type":"multi_conversations_select","selected_conversations":[]})
    );
    assert_eq!(shell.app_modal.as_ref().unwrap().fixture_attempts, 0);
}

#[test]
fn excluded_missing_or_im_origin_blocks_picker_and_full_submission() {
    for case in ["filtered", "missing", "im", "archived"] {
        let mut shell = shell();
        let ticket = users::Ticket::capture(&shell, &key()).unwrap();
        if case == "filtered" {
            let mut raw = fixture::view();
            raw["blocks"][1]["element"]["filter"] =
                json!({"exclude_external_shared_channels":true});
            shell
                .app_modal
                .as_mut()
                .unwrap()
                .current_form_mut()
                .unwrap()
                .reconcile(&raw)
                .unwrap();
        } else {
            let w = shell.core.workspaces.get_mut("T1").unwrap();
            if case == "missing" {
                w.channels.remove("C00000006");
            } else {
                let c = w.channels.get_mut("C00000006").unwrap();
                if case == "im" {
                    c.is_im = true;
                } else {
                    c.is_archived = true;
                }
            }
        }
        assert!(!ticket.current(&shell), "{case}");
        assert!(users::Ticket::capture(&shell, &key()).is_none(), "{case}");
        assert!(
            matches!(
                requests::prepare_submission(&shell),
                Err(FormError::Unsupported(_))
            ),
            "{case}"
        );
        assert_eq!(
            viewed_value(&shell),
            InputValue::Conversations(vec!["C00000006".into()])
        );
        assert_eq!(shell.app_modal.as_ref().unwrap().fixture_attempts, 0);
    }
}
