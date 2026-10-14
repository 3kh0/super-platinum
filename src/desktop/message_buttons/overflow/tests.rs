use super::*;
use crate::message_buttons::{self, overflow_fixture, source::Bag};
use serde_json::{Value, json};
use super_platinum_core::{
    config::WorkspaceSession,
    slack::{SlackClient, client::RequestBody},
};

fn setup() -> (ShellState, Source) {
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    overflow_fixture::install(&mut shell);
    let workspace = &shell.core.workspaces["T1"];
    let row = crate::message_vm::message_vm(
        workspace,
        &workspace.messages["C2"].messages[0],
        &shell.media,
    );
    let source = Source::capture(&shell, &row, "C2", Bag::Timeline).unwrap();
    let button = source.capture_button(&shell, "review", "submit").unwrap();
    prepare(&mut shell, &button);
    (shell, source)
}
fn prepare(shell: &mut ShellState, captured: &message_buttons::Captured) -> (Value, Value) {
    let workspace = WorkspaceSession {
        team_id: "T1".into(),
        enterprise_id: None,
        user_id: captured.source().owner().self_user_id.clone(),
        name: "Fixture".into(),
        url: "https://example.test".into(),
        token: "synthetic".into(),
    };
    let request = message_buttons::prepare_request(
        shell,
        captured,
        &SlackClient::default(),
        &workspace,
        "synthetic-correlation",
    )
    .unwrap();
    assert!(!request.retry_safe());
    let RequestBody::Form(fields) = request.body else {
        panic!("form")
    };
    let field = |name: &str| -> Value {
        serde_json::from_str(&fields.iter().find(|(key, _)| key == name).unwrap().1).unwrap()
    };
    (
        field("actions")[0].clone(),
        field("state")["values"].clone(),
    )
}
fn ticket(shell: &ShellState, source: &Source) -> Ticket {
    Ticket {
        source: source.clone(),
        metadata: source.overflow_metadata(shell, "choice", "pick").unwrap(),
        input_revision: inputs::revision(shell, source),
        stateless: stateless(shell, source),
        consumed: Arc::new(AtomicBool::new(false)),
        options: source.overflow_options(shell, "choice", "pick").unwrap(),
    }
}

#[test]
fn selected_option_is_exact_url_separate_and_sibling_state_preserved() {
    let (mut shell, source) = setup();
    let captured = source
        .capture_overflow(&shell, "choice", "pick", "guide")
        .unwrap();
    assert_eq!(captured.url(), Some("https://example.test/guide"));
    assert!(captured.confirmation().is_some());
    let (action, state) = prepare(&mut shell, &captured);
    assert_eq!(
        action["selected_option"],
        json!({"text":{"type":"plain_text","text":"Open review guide"},"value":"guide"})
    );
    assert_eq!(
        state["review"]["environment"]["selected_option"]["value"],
        "prod"
    );
    assert!(state.get("choice").is_none());
    assert!(state["review"].get("other").is_none());
    let button = source.capture_button(&shell, "review", "submit").unwrap();
    assert_eq!(prepare(&mut shell, &button).1, state);
}

#[test]
fn one_opening_accepts_one_choice_cancel_accepts_none() {
    let (shell, source) = setup();
    let opened = ticket(&shell, &source);
    assert!(opened.capture(&shell, "invented").is_none());
    assert!(opened.capture(&shell, "approve").is_some());
    assert!(opened.clone().capture(&shell, "guide").is_none());
    let canceled = ticket(&shell, &source);
    canceled.cancel();
    assert!(canceled.capture(&shell, "approve").is_none());
}

#[test]
fn owner_edit_and_sibling_revision_invalidate_open_menu() {
    let (mut shell, source) = setup();
    let opened = ticket(&shell, &source);
    inputs::invalidate(&mut shell, &source);
    assert!(opened.capture(&shell, "approve").is_none());
    let opened = ticket(&shell, &source);
    shell.core.account_epoch += 1;
    assert!(opened.capture(&shell, "approve").is_none());
    let (mut shell, source) = setup();
    let opened = ticket(&shell, &source);
    shell
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .messages
        .get_mut("C2")
        .unwrap()
        .messages[0]
        .blocks[1]["accessory"]["options"][0]["text"]["text"] = json!("Edited");
    assert!(opened.capture(&shell, "approve").is_none());
}

#[test]
fn uncertain_choice_cannot_be_reopened_or_replayed() {
    let (mut shell, source) = setup();
    let opened = ticket(&shell, &source);
    let captured = opened.capture(&shell, "approve").unwrap();
    let key = message_buttons::Key::new(&source, captured.metadata()).unwrap();
    shell.message_buttons.operations.insert(
        key,
        message_buttons::Operation {
            captured,
            phase: message_buttons::Phase::Uncertain,
            generation: 1,
        },
    );
    assert!(!presentation(&shell, &source, &opened.metadata, opened.stateless).enabled);
    assert!(ticket(&shell, &source).capture(&shell, "guide").is_none());
}
