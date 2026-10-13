use super::*;
use crate::media::MediaRegistry;
use serde_json::json;
use super_platinum_core::slack::events::RawEvent;

const ROOT: &str = "V_FORM_FIXTURE";

fn event(kind: &str, view: Value, previous: Option<&str>) -> RtEvent {
    let raw: RawEvent = serde_json::from_value(json!({
        "type":kind,"view_id":view["id"],"app_id":"A_FIXTURE",
        "view_type":"modal","client_token":"test-open","previous_view_id":previous,
        "view":view
    }))
    .unwrap();
    RtEvent::Unknown(raw)
}

fn shell() -> ShellState {
    let mut shell = ShellState::fixture(MediaRegistry::default());
    let scope = scope(&shell).unwrap();
    let mut session = Session::new(scope.clone());
    assert!(session.modal.enqueue(
        &scope,
        "test-open".into(),
        "A_FIXTURE".into(),
        now_ms(),
        60_000
    ));
    let RtEvent::Unknown(raw) = event("view_opened", fixture::view(), None) else {
        unreachable!()
    };
    assert_eq!(
        session.modal.apply(&scope, &raw, now_ms()),
        ModalChange::Opened
    );
    session.sync_forms();
    shell.app_modal = Some(session);
    shell.overlay = Some(Overlay::AppModal);
    shell
}

fn key(block: &str) -> InputKey {
    InputKey {
        block_id: block.into(),
        action_id: "value".into(),
    }
}

fn apply(shell: &mut ShellState, event: RtEvent) {
    let scope = scope(shell).unwrap();
    realtime(shell, &scope.team, scope.realtime_generation, &event);
}

fn child() -> Value {
    json!({"id":"V_CHILD","app_id":"A_FIXTURE","type":"modal",
        "previous_view_id":ROOT,"root_view_id":ROOT,
        "title":{"type":"plain_text","text":"Review"},
        "submit":{"type":"plain_text","text":"Save"},"blocks":[]})
}

#[test]
fn remote_update_preserves_dirty_writing_and_refreshes_pristine_fields() {
    let mut shell = shell();
    let session = shell.app_modal.as_mut().unwrap();
    session
        .current_form_mut()
        .unwrap()
        .set_value(&key("name"), InputValue::Text("My local release".into()))
        .unwrap();
    let mut view = fixture::view();
    view["blocks"][1]["element"]["initial_value"] = json!("Remote release");
    view["blocks"][2]["element"]["initial_value"] = json!("Remote notes");
    apply(&mut shell, event("view_updated", view, None));
    let session = shell.app_modal.as_ref().unwrap();
    let form = session.current_form().unwrap();
    assert_eq!(
        form.fields[&key("name")].value,
        InputValue::Text("My local release".into())
    );
    assert_eq!(
        form.fields[&key("note")].value,
        InputValue::Text("Remote notes".into())
    );
    assert!(!session.conflict);
}

#[test]
fn conflicting_remote_field_removal_keeps_local_form_and_blocks_submission() {
    let mut shell = shell();
    shell
        .app_modal
        .as_mut()
        .unwrap()
        .current_form_mut()
        .unwrap()
        .set_value(&key("name"), InputValue::Text("Keep this writing".into()))
        .unwrap();
    let mut view = fixture::view();
    view["blocks"].as_array_mut().unwrap().remove(1);
    apply(&mut shell, event("view_updated", view, None));
    let session = shell.app_modal.as_ref().unwrap();
    assert!(session.conflict);
    assert!(session.locked());
    assert_eq!(
        session.current_form().unwrap().fields[&key("name")].value,
        InputValue::Text("Keep this writing".into())
    );
    let shown = presentation(&shell).unwrap();
    assert!(shown.submit_label.is_none());
    assert!(shown.view_error.is_some());
}

#[test]
fn scope_replacement_hides_and_discards_modal_without_touching_other_overlay() {
    for change in 0..3 {
        let mut shell = shell();
        match change {
            0 => shell.core.account_epoch += 1,
            1 => shell.core.active_team = Some("other-team".into()),
            _ => shell.core.workspaces.get_mut("T1").unwrap().rt_generation += 1,
        }
        assert!(presentation(&shell).is_none());
        assert!(poll(&mut shell));
        assert!(shell.app_modal.is_none());
        assert!(shell.overlay.is_none());
        assert!(!poll(&mut shell));
    }
    let mut shell = shell();
    shell.overlay = Some(Overlay::Settings);
    shell.core.account_epoch += 1;
    assert!(poll(&mut shell));
    assert_eq!(shell.overlay, Some(Overlay::Settings));
}

#[test]
fn dismissed_child_restores_dirty_parent_and_invalidates_old_correlation() {
    let mut shell = shell();
    shell
        .app_modal
        .as_mut()
        .unwrap()
        .current_form_mut()
        .unwrap()
        .set_value(&key("name"), InputValue::Text("Parent writing".into()))
        .unwrap();
    apply(&mut shell, event("view_opened", child(), Some(ROOT)));
    let generation = shell.app_modal.as_ref().unwrap().generation;
    assert_eq!(shell.app_modal.as_ref().unwrap().modal.stack().len(), 2);
    back_local(&mut shell);
    let session = shell.app_modal.as_ref().unwrap();
    assert_eq!(session.modal.stack().len(), 1);
    assert_ne!(session.generation, generation);
    assert_eq!(
        session.current_form().unwrap().fields[&key("name")].value,
        InputValue::Text("Parent writing".into())
    );
    assert_eq!(session.modal.token_count(), 0);
    apply(&mut shell, event("view_opened", child(), Some(ROOT)));
    assert_eq!(shell.app_modal.as_ref().unwrap().modal.stack().len(), 1);
    dismiss_local(&mut shell);
    assert!(presentation(&shell).is_none());
    assert!(shell.overlay.is_none());
    assert!(
        shell.app_modal_closes.is_empty(),
        "disconnected tests must not enqueue live writes"
    );
}

#[test]
fn full_close_discards_entire_stack_and_all_correlation() {
    let mut shell = shell();
    apply(&mut shell, event("view_opened", child(), Some(ROOT)));
    dismiss_local(&mut shell);
    let session = shell.app_modal.as_ref().unwrap();
    assert!(session.modal.stack().is_empty());
    assert_eq!(session.modal.token_count(), 0);
    assert!(shell.overlay.is_none());
    apply(&mut shell, event("view_opened", child(), Some(ROOT)));
    assert!(shell.app_modal.as_ref().unwrap().modal.stack().is_empty());
}

#[test]
fn replacing_overlay_discards_modal_without_replacing_new_overlay() {
    let mut shell = shell();
    apply(&mut shell, event("view_opened", child(), Some(ROOT)));
    shell.overlay = Some(Overlay::Settings);
    assert!(poll(&mut shell));
    let session = shell.app_modal.as_ref().unwrap();
    assert!(session.modal.stack().is_empty());
    assert_eq!(session.modal.token_count(), 0);
    assert_eq!(shell.overlay, Some(Overlay::Settings));
    assert!(!poll(&mut shell));
}

#[test]
fn back_from_clear_on_close_child_discards_entire_stack() {
    let mut shell = shell();
    let mut child = child();
    child["clear_on_close"] = json!(true);
    apply(&mut shell, event("view_opened", child, Some(ROOT)));
    back_local(&mut shell);
    assert!(shell.app_modal.as_ref().unwrap().modal.stack().is_empty());
    assert!(shell.overlay.is_none());
}

#[test]
fn revealing_stale_parent_requires_current_guarded_hydration() {
    use super_platinum_core::slack::api::BlockViewResponseAction;
    for submit_child in [false, true] {
        let mut shell = shell();
        shell
            .app_modal
            .as_mut()
            .unwrap()
            .current_form_mut()
            .unwrap()
            .set_value(&key("name"), InputValue::Text("Retain parent text".into()))
            .unwrap();
        apply(&mut shell, event("view_opened", child(), Some(ROOT)));
        let stale_revision = shell.app_modal.as_ref().unwrap().modal.revision();
        let raw = serde_json::from_value(
            json!({"type":"view_updated","view_id":ROOT,"app_id":"A_FIXTURE"}),
        )
        .unwrap();
        apply(&mut shell, RtEvent::Unknown(raw));
        assert!(
            !shell.app_modal.as_ref().unwrap().loading,
            "active child remains loaded"
        );
        if submit_child {
            let revision = shell.app_modal.as_ref().unwrap().modal.revision();
            requests::complete(
                &mut shell,
                "V_CHILD",
                revision,
                "test-open",
                accepted(BlockViewResponseAction::Absent),
            );
        } else {
            back_local(&mut shell);
        }
        let session = shell.app_modal.as_mut().unwrap();
        assert_eq!(session.modal.stack().last().unwrap().id, ROOT);
        assert!(session.modal.stack().last().unwrap().needs_content);
        assert!(session.loading);
        assert!(session.locked());
        assert_eq!(
            session.current_form().unwrap().fields[&key("name")].value,
            InputValue::Text("Retain parent text".into())
        );
        let scope = session.modal.scope().clone();
        let revision = session.modal.revision();
        assert!(
            !session
                .modal
                .hydrate_current(&scope, ROOT, stale_revision, fixture::view())
        );
        let mut wrong_scope = scope.clone();
        wrong_scope.realtime_generation += 1;
        assert!(
            !session
                .modal
                .hydrate_current(&wrong_scope, ROOT, revision, fixture::view())
        );
        assert!(session.loading);
        assert!(
            session
                .modal
                .hydrate_current(&scope, ROOT, revision, fixture::view())
        );
        session.sync_forms();
        assert!(!session.loading);
        assert!(!session.locked());
        assert_eq!(
            session.current_form().unwrap().fields[&key("name")].value,
            InputValue::Text("Retain parent text".into())
        );
    }
}

fn accepted(
    action: super_platinum_core::slack::api::BlockViewResponseAction,
) -> Result<
    super_platinum_core::slack::api::BlockViewResponse<
        super_platinum_core::slack::api::BlockViewSubmission,
    >,
    super_platinum_core::slack::Error,
> {
    use super_platinum_core::slack::api::{BlockViewResponse, BlockViewSubmission};
    Ok(BlockViewResponse::Accepted(BlockViewSubmission {
        action,
        toast_message: None,
    }))
}

#[test]
fn rejected_submission_preserves_writing_and_exposes_field_feedback() {
    use super_platinum_core::slack::api::{BlockViewRejection, BlockViewResponse};
    let mut shell = shell();
    let session = shell.app_modal.as_mut().unwrap();
    session
        .current_form_mut()
        .unwrap()
        .set_value(&key("name"), InputValue::Text("Keep my text".into()))
        .unwrap();
    session.busy = true;
    let revision = session.modal.revision();
    requests::complete(
        &mut shell,
        ROOT,
        revision,
        "test-open",
        Ok(BlockViewResponse::Rejected(BlockViewRejection {
            error: "validation_failed".into(),
            message: Some("Less specific message".into()),
            errors: BTreeMap::from([("name".into(), "Choose a different name".into())]),
            view_error: Some("Review the name".into()),
        })),
    );
    let session = shell.app_modal.as_ref().unwrap();
    assert!(!session.locked());
    assert_eq!(session.modal.token_count(), 0);
    let field = &session.current_form().unwrap().fields[&key("name")];
    assert_eq!(field.value, InputValue::Text("Keep my text".into()));
    assert_eq!(field.error.as_deref(), Some("Choose a different name"));
    assert_eq!(session.view_error.as_deref(), Some("Review the name"));
}

#[test]
fn ambiguous_submission_stays_locked_and_preserves_form() {
    let mut shell = shell();
    let session = shell.app_modal.as_mut().unwrap();
    session.busy = true;
    let revision = session.modal.revision();
    requests::complete(
        &mut shell,
        ROOT,
        revision,
        "test-open",
        Err(super_platinum_core::slack::Error::Api(
            "invalid_response".into(),
        )),
    );
    let session = shell.app_modal.as_ref().unwrap();
    assert!(session.uncertain);
    assert!(session.locked());
    assert_eq!(session.current_form().unwrap().view_id, ROOT);
    assert_eq!(shell.overlay, Some(Overlay::AppModal));
}

#[test]
fn unsent_offline_preflight_allows_retry_but_network_offline_stays_uncertain() {
    use super_platinum_core::slack::Error;
    for (error, definitely_unsent) in [
        (
            Error::Api("block_interaction_not_sent_offline".into()),
            true,
        ),
        (Error::Offline("connection interrupted".into()), false),
    ] {
        let mut shell = shell();
        let session = shell.app_modal.as_mut().unwrap();
        session.busy = true;
        let revision = session.modal.revision();
        requests::complete(&mut shell, ROOT, revision, "test-open", Err(error));
        let session = shell.app_modal.as_ref().unwrap();
        assert!(!session.busy);
        assert_eq!(session.uncertain, !definitely_unsent);
        assert_eq!(session.locked(), !definitely_unsent);
        assert_eq!(session.modal.token_count(), usize::from(!definitely_unsent));
        assert!(session.view_error.is_some());
        assert_eq!(session.current_form().unwrap().view_id, ROOT);
    }
}

#[test]
fn stale_completion_cannot_close_or_unlock_current_view() {
    use super_platinum_core::slack::api::BlockViewResponseAction;
    let mut shell = shell();
    let session = shell.app_modal.as_mut().unwrap();
    session.busy = true;
    let revision = session.modal.revision();
    let generation = session.generation;
    for (view, result_revision) in [(ROOT, revision + 1), ("V_OTHER", revision)] {
        requests::complete(
            &mut shell,
            view,
            result_revision,
            "test-open",
            accepted(BlockViewResponseAction::Clear),
        );
    }
    let session = shell.app_modal.as_ref().unwrap();
    assert!(session.busy);
    assert_eq!(session.generation, generation);
    assert_eq!(session.current_form().unwrap().view_id, ROOT);
}

#[test]
fn accepted_push_then_absent_restores_dirty_parent() {
    use super_platinum_core::slack::api::BlockViewResponseAction;
    let mut shell = shell();
    let session = shell.app_modal.as_mut().unwrap();
    session
        .current_form_mut()
        .unwrap()
        .set_value(&key("name"), InputValue::Text("Parent draft".into()))
        .unwrap();
    session.busy = true;
    let revision = session.modal.revision();
    requests::complete(
        &mut shell,
        ROOT,
        revision,
        "test-open",
        accepted(BlockViewResponseAction::Push(child())),
    );
    let session = shell.app_modal.as_mut().unwrap();
    assert_eq!(session.current_form().unwrap().view_id, "V_CHILD");
    session.busy = true;
    let revision = session.modal.revision();
    requests::complete(
        &mut shell,
        "V_CHILD",
        revision,
        "test-open",
        accepted(BlockViewResponseAction::Absent),
    );
    let session = shell.app_modal.as_ref().unwrap();
    assert!(!session.locked());
    assert_eq!(
        session.current_form().unwrap().fields[&key("name")].value,
        InputValue::Text("Parent draft".into())
    );
    assert_eq!(shell.overlay, Some(Overlay::AppModal));
}

#[test]
fn parent_update_during_child_submission_cannot_enable_duplicate_submission() {
    use super_platinum_core::slack::api::BlockViewResponseAction;
    for inline in [false, true] {
        let mut shell = shell();
        apply(&mut shell, event("view_opened", child(), Some(ROOT)));
        let session = shell.app_modal.as_mut().unwrap();
        session.busy = true;
        let generation = session.generation;
        let revision = session.modal.revision();
        let mut update = event("view_updated", fixture::view(), None);
        if !inline {
            let RtEvent::Unknown(raw) = &mut update else {
                unreachable!()
            };
            raw.rest.remove("view");
        }
        apply(&mut shell, update);
        let session = shell.app_modal.as_ref().unwrap();
        assert_eq!(session.modal.stack().last().unwrap().id, "V_CHILD");
        assert!(session.busy && session.locked());
        assert!(!session.uncertain && !session.loading);
        assert_eq!(session.generation, generation);
        assert_eq!(session.modal.revision(), revision);
        requests::complete(
            &mut shell,
            "V_CHILD",
            revision,
            "test-open",
            accepted(BlockViewResponseAction::Absent),
        );
        let session = shell.app_modal.as_ref().unwrap();
        assert_eq!(session.modal.stack().last().unwrap().id, ROOT);
        assert!(!session.busy && !session.uncertain);
        assert_eq!(session.loading, !inline);
    }
}

#[test]
fn hidden_parent_dirty_conflicts_surface_only_when_back_reveals_it() {
    for remove_dirty_field in [false, true] {
        let mut shell = shell();
        shell
            .app_modal
            .as_mut()
            .unwrap()
            .current_form_mut()
            .unwrap()
            .set_value(&key("name"), InputValue::Text("My parent draft".into()))
            .unwrap();
        apply(&mut shell, event("view_opened", child(), Some(ROOT)));
        let mut parent = fixture::view();
        if remove_dirty_field {
            parent["blocks"].as_array_mut().unwrap().remove(1);
        } else {
            parent["blocks"][2]["element"]["initial_value"] = json!("New parent note");
        }
        apply(&mut shell, event("view_updated", parent.clone(), None));
        let session = shell.app_modal.as_ref().unwrap();
        assert_eq!(session.modal.stack()[0].payload.as_ref(), Some(&parent));
        assert_ne!(session.forms[ROOT].raw_view, parent);
        assert!(!session.locked());
        // Even explicit synchronization while the child is active cannot
        // reconcile hidden fields or make their conflict block the child.
        shell.app_modal.as_mut().unwrap().sync_forms();
        assert!(!shell.app_modal.as_ref().unwrap().locked());
        back_local(&mut shell);
        let session = shell.app_modal.as_ref().unwrap();
        assert_eq!(
            session.current_form().unwrap().fields[&key("name")].value,
            InputValue::Text("My parent draft".into())
        );
        assert_eq!(session.conflict, remove_dirty_field);
        if !remove_dirty_field {
            assert_eq!(
                session.current_form().unwrap().fields[&key("note")].value,
                InputValue::Text("New parent note".into())
            );
        }
    }
}

#[test]
fn updating_the_current_child_still_invalidates_its_old_submission_response() {
    use super_platinum_core::slack::api::BlockViewResponseAction;
    let mut shell = shell();
    apply(&mut shell, event("view_opened", child(), Some(ROOT)));
    let session = shell.app_modal.as_mut().unwrap();
    session.busy = true;
    let revision = session.modal.revision();
    let generation = session.generation;
    let mut changed = child();
    changed["title"]["text"] = json!("Changed current child");
    apply(&mut shell, event("view_updated", changed, Some(ROOT)));
    let session = shell.app_modal.as_ref().unwrap();
    assert!(!session.busy && session.uncertain);
    assert_ne!(session.generation, generation);
    assert_ne!(session.modal.revision(), revision);
    requests::complete(
        &mut shell,
        "V_CHILD",
        revision,
        "test-open",
        accepted(BlockViewResponseAction::Clear),
    );
    assert_eq!(
        shell
            .app_modal
            .as_ref()
            .unwrap()
            .modal
            .stack()
            .last()
            .unwrap()
            .id,
        "V_CHILD"
    );
    assert!(shell.overlay == Some(Overlay::AppModal));
}

#[test]
fn unrelated_realtime_scope_cannot_replace_active_form_or_cancel_busy_state() {
    let mut shell = shell();
    shell.app_modal.as_mut().unwrap().busy = true;
    let before = shell.app_modal.as_ref().unwrap().generation;
    let scope = scope(&shell).unwrap();
    for (team, generation) in [
        ("T_OTHER", scope.realtime_generation),
        ("T1", scope.realtime_generation + 1),
    ] {
        assert!(!realtime(
            &mut shell,
            team,
            generation,
            &event("view_updated", fixture::view(), None)
        ));
    }
    let session = shell.app_modal.as_ref().unwrap();
    assert_eq!(session.generation, before);
    assert!(session.busy);
    assert_eq!(session.current_form().unwrap().view_id, ROOT);
}
