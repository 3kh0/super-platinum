use super::*;
use crate::message_buttons::{source::Bag, time_fixture};
use serde_json::{Value, json};
use super_platinum_core::{
    config::WorkspaceSession,
    slack::{PreparedRequest, SlackClient, client::RequestBody, models::Message},
};

fn shell() -> ShellState {
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    time_fixture::install(&mut shell);
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
        .find(|message| message.ts.as_deref() == Some(time_fixture::MAIN_TS))
        .unwrap()
}

fn source(shell: &ShellState) -> Source {
    let workspace = &shell.core.workspaces["T1"];
    let message = workspace.messages["C2"]
        .messages
        .iter()
        .find(|message| message.ts.as_deref() == Some(time_fixture::MAIN_TS))
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
fn time_prepare_preserves_timezone_only_sibling_and_buttons_carry_committed_state() {
    let mut shell = shell();
    let source = source(&shell);
    let first = submit(&mut shell, &source);
    assert_eq!(
        first["choice"]["pick"],
        json!({"type":"timepicker","selected_time":"09:30","timezone":"America/New_York"})
    );
    assert_eq!(
        first["review"]["other"],
        json!({"type":"timepicker","timezone":"Europe/Berlin"})
    );
    let captured = source
        .capture_time(&shell, "choice", "pick", Some("10:45"))
        .unwrap();
    assert!(captured.confirmation().is_some());
    drop(captured);
    assert_eq!(submit(&mut shell, &source), first);
    let captured = source
        .capture_time(&shell, "choice", "pick", Some("10:45"))
        .unwrap();
    let (action, values) = send(&mut shell, &captured);
    assert_eq!(action["selected_time"], "10:45");
    assert_eq!(action["timezone"], "America/New_York");
    assert!(action.get("placeholder").is_none());
    assert_eq!(values["review"], first["review"]);
    let other = source
        .capture_time(&shell, "review", "other", Some("23:59"))
        .unwrap();
    let (_, values) = send(&mut shell, &other);
    assert_eq!(
        values["review"]["other"],
        json!({"type":"timepicker","selected_time":"23:59","timezone":"Europe/Berlin"})
    );
    assert_eq!(submit(&mut shell, &source), values);
}
#[test]
fn time_confirmation_retires_on_timezone_account_and_mount_changes() {
    let mut shell = shell();
    let source = source(&shell);
    let entry = candidate(&shell, &source).unwrap();
    shell
        .message_buttons
        .inputs
        .insert(ContainerKey::new(&source), entry);
    let metadata = source.time_metadata(&shell, "choice", "pick").unwrap();
    let mounted = display::Mounted::new();
    assert_eq!(
        display_selected(&shell, &source, &metadata, &mounted).as_deref(),
        Some("09:30")
    );
    let mut captured = source
        .capture_time(&shell, "choice", "pick", Some("10:45"))
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
            "time-token"
        )
        .is_err()
    );
    let captured = source
        .capture_time(&shell, "choice", "pick", Some("10:45"))
        .unwrap();
    canonical(&mut shell).blocks[1]["accessory"]["timezone"] = json!("Asia/Tokyo");
    assert!(captured.revalidate(&shell).is_err());
    let fresh = self::source(&shell);
    let captured = fresh
        .capture_time(&shell, "choice", "pick", Some("10:45"))
        .unwrap();
    shell.core.account_epoch += 1;
    assert!(captured.revalidate(&shell).is_err());
}
