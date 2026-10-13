use super::*;
use crate::message_buttons::{date_fixture, source::Bag};
use serde_json::{Value, json};
use super_platinum_core::{
    config::WorkspaceSession,
    slack::{PreparedRequest, SlackClient, client::RequestBody, models::Message},
};

fn shell() -> ShellState {
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    date_fixture::install(&mut shell);
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
        .find(|message| message.ts.as_deref() == Some(date_fixture::MAIN_TS))
        .unwrap()
}

fn source(shell: &ShellState) -> Source {
    let workspace = &shell.core.workspaces["T1"];
    let message = workspace.messages["C2"]
        .messages
        .iter()
        .find(|message| message.ts.as_deref() == Some(date_fixture::MAIN_TS))
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
fn date_capture_cancel_prepare_and_later_button_preserve_complete_state() {
    let mut shell = shell();
    let source = source(&shell);
    let first = submit(&mut shell, &source);
    assert_eq!(
        first["choice"]["pick"],
        json!({"type":"datepicker","selected_date":"2026-10-13"})
    );
    assert!(first["review"].get("other").is_none());
    let captured = source
        .capture_date(&shell, "choice", "pick", Some("2028-02-29"))
        .unwrap();
    assert!(captured.confirmation().is_some());
    assert_eq!(
        shell.message_buttons.inputs[&ContainerKey::new(&source)]
            .model
            .values()
            .unwrap(),
        Some(first.clone())
    );
    drop(captured);
    assert_eq!(submit(&mut shell, &source), first);
    let captured = source
        .capture_date(&shell, "choice", "pick", Some("2028-02-29"))
        .unwrap();
    let (action, values) = send(&mut shell, &captured);
    assert_eq!(action["selected_date"], json!("2028-02-29"));
    assert!(action.get("placeholder").is_none());
    assert!(action.get("confirm").is_some());
    assert_eq!(
        values["review"]["environment"],
        first["review"]["environment"]
    );
    let other = source
        .capture_date(&shell, "review", "other", Some("2028-03-01"))
        .unwrap();
    let (_, values) = send(&mut shell, &other);
    assert_eq!(
        values["choice"]["pick"]["selected_date"],
        json!("2028-02-29")
    );
    assert_eq!(
        values["review"]["other"]["selected_date"],
        json!("2028-03-01")
    );
    assert_eq!(submit(&mut shell, &source), values);
}
#[test]
fn date_confirmation_rejects_unmount_local_revision_canonical_or_account_drift() {
    let mut shell = shell();
    let source = source(&shell);
    let entry = candidate(&shell, &source).unwrap();
    shell
        .message_buttons
        .inputs
        .insert(ContainerKey::new(&source), entry);
    let metadata = source.date_metadata(&shell, "choice", "pick").unwrap();
    let mounted = display::Mounted::new();
    assert_eq!(
        display_selected(&shell, &source, &metadata, &mounted).as_deref(),
        Some("2026-10-13")
    );
    let mut captured = source
        .capture_date(&shell, "choice", "pick", Some("2026-10-14"))
        .unwrap();
    captured.set_display(
        mounted
            .capture(&ContainerKey::new(&source), &key("choice", "pick"))
            .unwrap(),
    );
    let pending = Pending {
        captured: captured.clone(),
        revision: next_id(),
        input_revision: revision(&shell, &source),
    };
    assert!(pending_current(&shell, &pending));
    invalidate(&mut shell, &source);
    assert!(!pending_current(&shell, &pending));
    mounted.unmount();
    assert!(
        prepare(
            &mut shell,
            &captured,
            &SlackClient::default(),
            &workspace(&source),
            "date-token"
        )
        .is_err()
    );
    let captured = source
        .capture_date(&shell, "choice", "pick", Some("2026-10-14"))
        .unwrap();
    canonical(&mut shell).blocks[1]["accessory"]["initial_date"] = json!("2026-11-01");
    assert!(captured.revalidate(&shell).is_err());
    let fresh = self::source(&shell);
    let captured = fresh
        .capture_date(&shell, "choice", "pick", Some("2026-11-02"))
        .unwrap();
    shell.core.account_epoch += 1;
    assert!(captured.revalidate(&shell).is_err());
}
