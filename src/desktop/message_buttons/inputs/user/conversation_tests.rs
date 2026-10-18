use super::*;
use crate::message_buttons::{conversation_fixture as user_fixture, source::Bag};
use serde_json::{Value, json};
use std::sync::Arc;
use super_platinum_core::{
    config::{Session, WorkspaceSession},
    slack::{SlackClient, Transport, client::RequestBody},
};
fn setup() -> (ShellState, Source, BlockElementMetadata, display::Mounted) {
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    user_fixture::install(&mut shell);
    let w = &shell.core.workspaces["T1"];
    let message = w.messages["C2"]
        .messages
        .iter()
        .find(|m| m.ts.as_deref() == Some(crate::message_buttons::user_fixture::MAIN_TS))
        .unwrap();
    let row = crate::message_vm::message_vm(w, message, &shell.media);
    let source = Source::capture(&shell, &row, "C2", Bag::Timeline).unwrap();
    let entry = super::super::candidate(&shell, &source).unwrap();
    shell
        .message_buttons
        .inputs
        .insert(ContainerKey::new(&source), entry);
    let metadata = source
        .conversation_metadata(&shell, "choice", "pick")
        .unwrap();
    let mounted = display::Mounted::new();
    assert_eq!(
        super::super::display_selected(&shell, &source, &metadata, &mounted).as_deref(),
        Some("C00000001")
    );
    (shell, source, metadata, mounted)
}
fn session(source: &Source) -> WorkspaceSession {
    WorkspaceSession {
        team_id: "T1".into(),
        enterprise_id: None,
        user_id: source.owner().self_user_id.clone(),
        name: "Fixture".into(),
        url: "https://example.invalid".into(),
        token: "synthetic-only".into(),
    }
}
fn values(shell: &mut ShellState, captured: &Captured) -> Value {
    let request = super::super::prepare(
        shell,
        captured,
        &SlackClient::default(),
        &session(captured.source()),
        "user-fixture",
    )
    .unwrap();
    assert!(!request.retry_safe());
    let RequestBody::Form(fields) = request.body else {
        panic!("form");
    };
    serde_json::from_str(&fields.iter().find(|(k, _)| k == "state").unwrap().1).unwrap()
}
fn install_picker(
    shell: &mut ShellState,
    source: Source,
    metadata: BlockElementMetadata,
    mounted: display::Mounted,
) -> u64 {
    let display = mounted
        .capture(&ContainerKey::new(&source), &field_key(&metadata).unwrap())
        .unwrap();
    let revision = next_id();
    shell.message_buttons.user_picker = Some(Picker {
        multiple: None,
        input_revision: super::super::revision(shell, &source).unwrap(),
        context: crate::scheduled::Context::capture(shell),
        filter: filter(shell, &source, &metadata),
        candidates: conversations::cached(
            shell,
            &source,
            &filter(shell, &source, &metadata).unwrap(),
            "",
        ),
        source,
        metadata,
        mounted,
        display,
        revision,
        query_revision: 5,
        query: "person".into(),
        loading: true,
        agent_unavailable: false,
        error: None,
    });
    shell.overlay = Some(Overlay::MessageUserSelect);
    revision
}
#[test]
fn conversations_confirm_cancel_preserves_wire_and_prepare_commits_full_siblings() {
    let (mut shell, source, metadata, mounted) = setup();
    let button = source.capture_button(&shell, "review", "submit").unwrap();
    let initial = values(&mut shell, &button);
    assert_eq!(
        initial["values"]["choice"]["pick"],
        json!({"type":"conversations_select","selected_conversation":"C00000001"})
    );
    assert!(initial["values"]["review"].get("other").is_none());
    let mut captured = source
        .capture_conversation(&shell, "choice", "pick", Some("U00000002"))
        .unwrap();
    captured.set_display(
        mounted
            .capture(&ContainerKey::new(&source), &field_key(&metadata).unwrap())
            .unwrap(),
    );
    assert!(captured.confirmation().is_some());
    drop(captured); // Cancel has no reducer mutation.
    assert_eq!(values(&mut shell, &button), initial);
    assert_eq!(
        super::super::display_selected(&shell, &source, &metadata, &mounted).as_deref(),
        Some("C00000001")
    );
    let mut captured = source
        .capture_conversation(&shell, "choice", "pick", Some("U00000002"))
        .unwrap();
    captured.set_display(
        mounted
            .capture(&ContainerKey::new(&source), &field_key(&metadata).unwrap())
            .unwrap(),
    );
    let next = values(&mut shell, &captured);
    assert_eq!(
        next["values"]["choice"]["pick"]["selected_conversation"],
        "U00000002"
    );
    assert_eq!(next["values"]["review"], initial["values"]["review"]);
    assert_eq!(
        super::super::display_selected(&shell, &source, &metadata, &mounted).as_deref(),
        Some("U00000002")
    );
    assert_eq!(values(&mut shell, &button), next);
}
#[test]
fn picker_query_and_mount_generations_and_transport_guard_are_independent() {
    let (mut shell, source, metadata, mounted) = setup();
    shell.core.session = Some(Session {
        unified_user_client_enabled: Default::default(),
        d_cookie: "synthetic".into(),
        workspaces: [("T1".into(), session(&source))].into(),
    });
    shell.core.transport = Some(Arc::new(Transport::new("synthetic").unwrap()));
    let revision = install_picker(
        &mut shell,
        source.clone(),
        metadata.clone(),
        mounted.clone(),
    );
    let context = crate::scheduled::Context::capture(&shell).unwrap();
    assert!(directory::request_current(&shell, revision, 5));
    assert!(!directory::request_current(&shell, revision, 4));
    // A 429 from an older in-flight query arrives during this query's debounce.
    shell.message_buttons.user_retry.insert(
        (context.epoch, "T1".into()),
        std::time::Instant::now() + std::time::Duration::from_secs(60),
    );
    assert!(!directory::admit_query(&mut shell, &context, revision, 5));
    assert!(!shell.message_buttons.user_picker.as_ref().unwrap().loading);
    cancel_local(&mut shell);
    let reopened = install_picker(&mut shell, source, metadata, mounted.clone());
    assert!(!directory::admit_query(&mut shell, &context, reopened, 5));
    shell.message_buttons.user_retry.clear();
    assert!(directory::admit_query(&mut shell, &context, reopened, 5));
    shell.core.active_team = Some("T2".into());
    assert!(directory::owner_current(&shell, &context)); // Preserve late Retry-After.
    assert!(!directory::context_current(&shell, &context));
    shell.core.active_team = Some("T1".into());
    shell.core.transport = Some(Arc::new(Transport::new("replacement").unwrap()));
    assert!(!directory::request_current(&shell, reopened, 5));
    assert!(poll(&mut shell));
    assert!(shell.message_buttons.user_picker.is_none());
    mounted.unmount();
}
