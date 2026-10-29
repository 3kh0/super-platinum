//! Model/controller acceptance only; no native input or transport execution.
use super::*;
use crate::media::MediaRegistry;
use serde_json::json;

const ROOT: &str = "V_INTRODUCED_INTEGER";
fn key() -> InputKey {
    InputKey {
        block_id: "adjustment".into(),
        action_id: "value".into(),
    }
}
fn root_view(number: bool) -> Value {
    let mut blocks = vec![
        json!({"type":"input","block_id":"name","label":{"type":"plain_text","text":"Name"},
        "element":{"type":"plain_text_input","action_id":"value","initial_value":"Retained"}}),
    ];
    if number {
        blocks.push(json!({"type":"input","block_id":"adjustment","label":{"type":"plain_text","text":"Adjustment"},
            "element":{"type":"number_input","action_id":"value","initial_value":"-1","is_decimal_allowed":false}}));
    }
    json!({"id":ROOT,"type":"modal","app_id":"A1","title":{"type":"plain_text","text":"Adjust inventory"},
        "submit":{"type":"plain_text","text":"Save"},"blocks":blocks})
}
fn deliver(shell: &mut ShellState, kind: &str, view: Value, token: &str) {
    let scope = scope(shell).unwrap();
    let event = RtEvent::Unknown(
        serde_json::from_value(json!({"type":kind,"view_id":view["id"],
        "app_id":"A1","view_type":"modal","client_token":token,"view":view}))
        .unwrap(),
    );
    realtime(shell, &scope.team, scope.realtime_generation, &event);
}
fn armed() -> (ShellState, String) {
    let mut shell = ShellState::fixture(MediaRegistry::default());
    crate::app_home::fixture::install(&mut shell);
    let action = register_action(&mut shell, "A1").unwrap();
    assert!(action_scope_current(&shell, &action));
    deliver(&mut shell, "view_opened", root_view(false), &action.token);
    let session = shell.app_modal.as_ref().unwrap();
    assert!(!session.fixture);
    assert!(session.integer_context.is_some());
    assert!(session.modal.integer_introduction_context_live());
    deliver(&mut shell, "view_updated", root_view(true), &action.token);
    assert_eq!(
        requests::prepare_submission(&shell).unwrap()["values"]["adjustment"]["value"]["value"],
        "-1"
    );
    (shell, action.token)
}

#[test]
fn actual_token_rt_root_introduces_literal_then_consumes_once_and_survives_conflict_reset_without_revival()
 {
    let (mut shell, token) = armed();
    let pristine = requests::prepare_submission(&shell).unwrap();
    assert_eq!(
        pristine,
        json!({"values":{
            "name":{"value":{"type":"plain_text_input","value":"Retained"}},
            "adjustment":{"value":{"type":"number_input","value":"-1"}}
        }})
    );
    shell
        .app_modal
        .as_mut()
        .unwrap()
        .current_form_mut()
        .unwrap()
        .set_value(&key(), InputValue::Number(Some("2".into())))
        .unwrap();
    assert_eq!(
        requests::prepare_submission(&shell).unwrap()["values"]["adjustment"]["value"]["value"],
        "2"
    );
    shell
        .app_modal
        .as_mut()
        .unwrap()
        .current_form_mut()
        .unwrap()
        .set_value(&key(), InputValue::Number(None))
        .unwrap();
    assert!(matches!(
        requests::prepare_submission(&shell),
        Err(FormError::Validation(_))
    ));
    shell
        .app_modal
        .as_mut()
        .unwrap()
        .current_form_mut()
        .unwrap()
        .set_value(&key(), InputValue::Number(Some("-2".into())))
        .unwrap();
    let stale = shell
        .app_modal
        .as_ref()
        .unwrap()
        .current_form()
        .unwrap()
        .clone();
    deliver(&mut shell, "view_updated", root_view(true), &token);
    assert!(stale.submission().is_err());
    assert_eq!(
        requests::prepare_submission(&shell).unwrap()["values"]["adjustment"]["value"]["value"],
        "-2"
    );
    let session = shell.app_modal.as_ref().unwrap();
    let identity = (
        session.modal.revision(),
        session.generation,
        session.reset_revision,
    );
    let (payload, client_token, revision) =
        requests::accept_integer_submission_for_test(&mut shell).unwrap();
    assert_eq!(payload["values"]["adjustment"]["value"]["value"], "-2");
    let session = shell.app_modal.as_ref().unwrap();
    assert_eq!(
        (
            session.modal.revision(),
            session.generation,
            session.reset_revision
        ),
        identity
    );
    assert!(session.current_form().unwrap().submission().is_err());
    assert!(
        presentation(&shell).unwrap().view_error.is_none(),
        "a pending first request is not a form-change failure"
    );
    assert!(requests::accept_integer_submission_for_test(&mut shell).is_none());

    // A model acknowledgement exercises the real completion/conflict reset path.
    // The accepted request above is never executed by a transport.
    let child = json!({"id":"V_INTRO_CHILD","type":"modal","app_id":"A1","previous_view_id":ROOT,"root_view_id":ROOT,
        "title":{"type":"plain_text","text":"Review"},"submit":{"type":"plain_text","text":"Continue"},"blocks":[]});
    shell.app_modal.as_mut().unwrap().conflict = true;
    requests::complete(
        &mut shell,
        ROOT,
        revision,
        &client_token,
        Ok(api::BlockViewResponse::Accepted(api::BlockViewSubmission {
            action: api::BlockViewResponseAction::Push(child.clone()),
            toast_message: None,
        })),
    );
    let session = shell.app_modal.as_ref().unwrap();
    assert!(!session.conflict);
    assert!(
        !session.locked(),
        "a legacy child is independent of retired parent additions"
    );
    assert!(requests::prepare_submission(&shell).is_ok());
    let mut changed_child = child;
    changed_child["title"]["text"] = json!("Review updated");
    deliver(&mut shell, "view_updated", changed_child, &token);
    assert_eq!(presentation(&shell).unwrap().title, "Review updated");
    assert!(requests::prepare_submission(&shell).is_ok());
    back_local(&mut shell);
    let session = shell.app_modal.as_ref().unwrap();
    assert!(!session.conflict);
    assert_eq!(
        session.current_form().unwrap().fields[&key()].value,
        InputValue::Number(Some("-2".into()))
    );
    assert!(requests::prepare_submission(&shell).is_err());
    deliver(&mut shell, "view_updated", root_view(false), &token);
    let form = shell.app_modal.as_ref().unwrap().current_form().unwrap();
    assert!(!form.fields.contains_key(&key()));
    assert!(form.unsupported.is_empty());
    assert!(form.has_introduced_integers());
    assert!(requests::prepare_submission(&shell).is_err());
}

#[test]
fn actual_root_context_and_terminal_outcomes_retire_authority_without_false_retry_guidance() {
    let (mut shell, token) = armed();
    shell
        .app_modal
        .as_mut()
        .unwrap()
        .current_form_mut()
        .unwrap()
        .set_value(&key(), InputValue::Number(Some("23".into())))
        .unwrap();
    let stale = shell
        .app_modal
        .as_ref()
        .unwrap()
        .current_form()
        .unwrap()
        .clone();
    let mut changed = root_view(true);
    changed["blocks"][1]["label"]["text"] = json!("Changed while live");
    deliver(&mut shell, "view_updated", changed, &token);
    assert!(stale.submission().is_err());
    assert!(requests::prepare_submission(&shell).is_err());
    assert_eq!(
        shell
            .app_modal
            .as_ref()
            .unwrap()
            .current_form()
            .unwrap()
            .fields[&key()]
            .value,
        InputValue::Number(Some("23".into()))
    );
    assert!(
        presentation(&shell)
            .unwrap()
            .view_error
            .unwrap()
            .contains("Close and reopen")
    );

    for outcome in 0..3 {
        let (mut shell, _) = armed();
        let (_, token, revision) =
            requests::accept_integer_submission_for_test(&mut shell).unwrap();
        let result = match outcome {
            0 => Err(super_platinum_core::slack::Error::Api(
                "block_interaction_not_sent_offline".into(),
            )),
            1 => Ok(api::BlockViewResponse::Rejected(api::BlockViewRejection {
                error: "validation_failed".into(),
                message: Some("Review your entries.".into()),
                errors: BTreeMap::new(),
                view_error: None,
            })),
            _ => Err(super_platinum_core::slack::Error::Api(
                "invalid_response".into(),
            )),
        };
        requests::complete(&mut shell, ROOT, revision, &token, result);
        assert!(requests::prepare_submission(&shell).is_err());
        assert!(requests::accept_integer_submission_for_test(&mut shell).is_none());
        let error = presentation(&shell).unwrap().view_error.unwrap();
        match outcome {
            0 => assert!(error.contains("It has not been sent.")),
            1 => assert!(error.contains("Review your entries.")),
            _ => {
                assert!(error.contains("could not confirm the app's response"));
                assert!(error.contains("Your submission will not be repeated."));
            }
        }
        assert_eq!(
            error.contains("Close and reopen"),
            outcome != 2,
            "known terminal failures explain recovery; uncertain delivery keeps its no-replay guidance"
        );
    }

    for case in 0..4 {
        let (mut shell, _) = armed();
        let stale = shell
            .app_modal
            .as_ref()
            .unwrap()
            .current_form()
            .unwrap()
            .clone();
        let team = shell.core.active_team.clone().unwrap();
        match case {
            0 => {
                shell
                    .core
                    .session
                    .as_mut()
                    .unwrap()
                    .workspaces
                    .get_mut(&team)
                    .unwrap()
                    .token
                    .push('x');
            }
            1 => {
                shell.core.transport = Some(std::sync::Arc::new(
                    super_platinum_core::slack::Transport::new("synthetic-successor").unwrap(),
                ));
            }
            2 => {
                shell.core.account_epoch += 1;
            }
            _ => {
                shell.core.workspaces.get_mut(&team).unwrap().rt_generation += 1;
            }
        }
        if case == 0 {
            assert!(requests::prepare_submission(&shell).is_err());
        } else {
            assert!(poll(&mut shell));
        }
        assert!(stale.submission().is_err(), "case={case}");
        assert!(requests::prepare_submission(&shell).is_err(), "case={case}");
        if case < 2 {
            assert_eq!(
                shell
                    .app_modal
                    .as_ref()
                    .unwrap()
                    .current_form()
                    .unwrap()
                    .fields[&key()]
                    .value,
                InputValue::Number(Some("-1".into()))
            );
        }
    }
}
