use super::*;
use crate::message_buttons::{radio_fixture, source::Bag};
use serde_json::{Value, json};
use super_platinum_core::{
    config::WorkspaceSession,
    slack::{PreparedRequest, SlackClient, client::RequestBody, models::Message},
};

fn shell() -> ShellState {
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    radio_fixture::install(&mut shell);
    // Exercise both typed controls in one ordinary message container.
    canonical(&mut shell).blocks[2]["elements"][0]["type"] = json!("static_select");
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
        .find(|message| message.ts.as_deref() == Some(radio_fixture::MAIN_TS))
        .unwrap()
}

fn source(shell: &ShellState) -> Source {
    let workspace = &shell.core.workspaces["T1"];
    let message = workspace.messages["C2"]
        .messages
        .iter()
        .find(|message| message.ts.as_deref() == Some(radio_fixture::MAIN_TS))
        .unwrap();
    let row = crate::message_vm::message_vm(workspace, message, &shell.media);
    Source::capture(shell, &row, "C2", Bag::Timeline).unwrap()
}

fn workspace(source: &Source) -> WorkspaceSession {
    WorkspaceSession {
        team_id: source.owner().team_id.clone(),
        enterprise_id: None,
        user_id: source.owner().self_user_id.clone(),
        name: "Fixture".into(),
        url: "https://example.test".into(),
        token: "synthetic".into(),
    }
}

fn wire(request: PreparedRequest) -> (Value, Value) {
    assert!(!request.retry_safe());
    let RequestBody::Form(fields) = request.body else {
        panic!("expected form request")
    };
    let field = |name: &str| -> Value {
        serde_json::from_str(&fields.iter().find(|(key, _)| key == name).unwrap().1).unwrap()
    };
    (
        field("actions")[0].clone(),
        field("state")["values"].clone(),
    )
}

fn send(shell: &mut ShellState, captured: &Captured) -> (Value, Value) {
    wire(
        prepare(
            shell,
            captured,
            &SlackClient::default(),
            &workspace(captured.source()),
            "synthetic-radio",
        )
        .unwrap(),
    )
}

fn submit(shell: &mut ShellState, source: &Source) -> Value {
    let captured = source.capture_button(shell, "review", "submit").unwrap();
    send(shell, &captured).1
}

fn key(block: &str, action: &str) -> InputKey {
    InputKey {
        block_id: block.into(),
        action_id: action.into(),
    }
}

#[test]
fn first_button_initializes_radio_and_select_state_with_exact_raw_options() {
    let mut shell = shell();
    let source = source(&shell);
    let message = canonical(&mut shell).clone();
    assert!(shell.message_buttons.inputs.is_empty());
    let values = submit(&mut shell, &source);
    assert_eq!(
        values,
        json!({
            "choice":{"pick":{"type":"radio_buttons","selected_option":message.blocks[1]["accessory"]["initial_option"]}},
            "review":{"environment":{"type":"static_select","selected_option":message.blocks[2]["elements"][0]["initial_option"]}}
        })
    );
    assert_eq!(shell.message_buttons.inputs.len(), 1);
}

#[test]
fn mixed_radio_and_select_actions_commit_complete_state_for_later_button() {
    let mut shell = shell();
    let source = source(&shell);
    let radio = source
        .capture_radio(&shell, "choice", "pick", Some("b"))
        .unwrap();
    let (action, values) = send(&mut shell, &radio);
    assert_eq!(action["type"], "radio_buttons");
    assert_eq!(
        action["selected_option"],
        values["choice"]["pick"]["selected_option"]
    );
    assert_eq!(
        action["selected_option"]["description"]["text"],
        "Preview release"
    );
    assert!(action.get("confirm").is_some());
    assert!(action.get("placeholder").is_none());
    assert_eq!(
        values["review"]["environment"]["selected_option"]["value"],
        "prod"
    );
    let select = source
        .capture_select(&shell, "review", "environment", Some("test"))
        .unwrap();
    let (action, values) = send(&mut shell, &select);
    assert_eq!(action["type"], "static_select");
    assert_eq!(values["choice"]["pick"]["selected_option"]["value"], "b");
    assert_eq!(
        values["review"]["environment"]["selected_option"]["value"],
        "test"
    );
    assert_eq!(submit(&mut shell, &source), values);
}

#[test]
fn removed_radio_option_retains_display_while_select_resets_and_both_retain_wire_state() {
    let mut shell = shell();
    let old = source(&shell);
    let radio = old
        .capture_radio(&shell, "choice", "pick", Some("b"))
        .unwrap();
    send(&mut shell, &radio);
    let select = old
        .capture_select(&shell, "review", "environment", Some("test"))
        .unwrap();
    let (_, committed) = send(&mut shell, &select);
    let message = canonical(&mut shell);
    message.blocks[1]["accessory"]["options"]
        .as_array_mut()
        .unwrap()
        .pop();
    message.blocks[2]["elements"][0]["options"]
        .as_array_mut()
        .unwrap()
        .pop();
    let current = source(&shell);
    assert_eq!(submit(&mut shell, &current), committed);
    let entry = &shell.message_buttons.inputs[&ContainerKey::new(&current)];
    assert_eq!(
        entry.model.fields[&key("choice", "pick")].value,
        InputValue::Selected(vec!["b".into()])
    );
    assert_eq!(
        entry.model.fields[&key("review", "environment")].value,
        InputValue::Selected(vec![])
    );
    assert!(
        current
            .capture_radio(&shell, "choice", "pick", Some("b"))
            .is_err()
    );
    assert!(
        current
            .capture_select(&shell, "review", "environment", Some("test"))
            .is_err()
    );
}

#[test]
fn changed_radio_initial_option_preserves_existing_display_and_wire_until_next_choice() {
    let mut shell = shell();
    let old = source(&shell);
    let original = submit(&mut shell, &old);
    let radio = &mut canonical(&mut shell).blocks[1]["accessory"];
    radio["initial_option"] = radio["options"][1].clone();
    let current = source(&shell);
    assert_eq!(submit(&mut shell, &current), original);
    let entry = &shell.message_buttons.inputs[&ContainerKey::new(&current)];
    assert_eq!(
        entry.model.fields[&key("choice", "pick")].value,
        InputValue::Selected(vec!["a".into()])
    );
    let captured = current
        .capture_radio(&shell, "choice", "pick", Some("b"))
        .unwrap();
    let (action, values) = send(&mut shell, &captured);
    assert_eq!(action["initial_option"]["value"], "b");
    assert_eq!(values["choice"]["pick"]["selected_option"]["value"], "b");
}

#[test]
fn pending_radio_confirmation_is_pure_and_retires_after_sibling_selection() {
    let mut shell = shell();
    let source = source(&shell);
    let before = submit(&mut shell, &source);
    let pending = Pending {
        captured: source
            .capture_radio(&shell, "choice", "pick", Some("b"))
            .unwrap(),
        revision: next_id(),
        input_revision: revision(&shell, &source),
    };
    assert!(pending_current(&shell, &pending));
    assert_eq!(
        shell.message_buttons.inputs[&ContainerKey::new(&source)]
            .model
            .values()
            .unwrap(),
        Some(before)
    );
    let sibling = source
        .capture_select(&shell, "review", "environment", Some("test"))
        .unwrap();
    send(&mut shell, &sibling);
    assert!(pending.captured.revalidate(&shell).is_ok());
    assert!(!pending_current(&shell, &pending));
}

#[test]
fn stale_source_or_changed_owner_cannot_prepare_radio_or_mutate_input_state() {
    for case in 0..4 {
        let mut shell = shell();
        let source = source(&shell);
        submit(&mut shell, &source);
        let captured = source
            .capture_radio(&shell, "choice", "pick", Some("b"))
            .unwrap();
        let before = shell.message_buttons.inputs[&ContainerKey::new(&source)]
            .model
            .clone();
        match case {
            0 => shell.core.account_epoch += 1,
            1 => shell.core.workspaces.get_mut("T1").unwrap().self_user_id = "U_OTHER".into(),
            2 => {
                canonical(&mut shell).blocks[1]["accessory"]["options"][1]["description"]["text"] =
                    json!("Updated description")
            }
            _ => shell.channel_generation += 1,
        }
        assert!(captured.revalidate(&shell).is_err(), "case {case}");
        assert!(
            prepare(
                &mut shell,
                &captured,
                &SlackClient::default(),
                &workspace(&source),
                "synthetic-radio"
            )
            .is_err(),
            "case {case}"
        );
        assert_eq!(
            shell.message_buttons.inputs[&ContainerKey::new(&source)].model,
            before
        );
    }
}
