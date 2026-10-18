use super::*;
use crate::message_buttons::{channel_fixture as user_fixture, source::Bag};
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
    let metadata = source.channel_metadata(&shell, "choice", "pick").unwrap();
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
fn channels_confirm_cancel_preserves_wire_and_prepare_commits_full_siblings() {
    let (mut shell, source, metadata, mounted) = setup();
    let button = source.capture_button(&shell, "review", "submit").unwrap();
    let initial = values(&mut shell, &button);
    assert_eq!(
        initial["values"]["choice"]["pick"],
        json!({"type":"channels_select","selected_channel":"C00000001"})
    );
    assert!(initial["values"]["review"].get("other").is_none());
    let mut captured = source
        .capture_channel(&shell, "choice", "pick", Some("C00000007"), None)
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
        .capture_channel(&shell, "choice", "pick", Some("C00000007"), None)
        .unwrap();
    captured.set_display(
        mounted
            .capture(&ContainerKey::new(&source), &field_key(&metadata).unwrap())
            .unwrap(),
    );
    let next = values(&mut shell, &captured);
    assert_eq!(
        next["values"]["choice"]["pick"]["response_url_enabled"],
        true
    );
    assert_eq!(
        next["values"]["choice"]["pick"]["selected_channel"],
        "C00000007"
    );
    assert_eq!(next["values"]["review"], initial["values"]["review"]);
    assert_eq!(
        super::super::display_selected(&shell, &source, &metadata, &mounted).as_deref(),
        Some("C00000007")
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

#[test]
fn channel_public_policy_and_confirmation_recheck_reject_new_archived_metadata() {
    let (mut shell, source, metadata, mounted) = setup();
    let filter = filter(&shell, &source, &metadata).unwrap();
    let choices = conversations::cached(&shell, &source, &filter, "");
    for id in ["C00000001", "C00000007", "C00000004"] {
        assert!(choices.iter().any(|c| c.id == id));
    }
    for id in [
        "G00000002",
        "G00000003",
        "D00000006",
        "C00000005",
        "U00000001",
    ] {
        assert!(!choices.iter().any(|c| c.id == id));
    }
    let mut captured = source
        .capture_channel(&shell, "choice", "pick", Some("C00000007"), None)
        .unwrap();
    captured.set_display(
        mounted
            .capture(&ContainerKey::new(&source), &field_key(&metadata).unwrap())
            .unwrap(),
    );
    shell
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .channels
        .get_mut("C00000007")
        .unwrap()
        .is_archived = true;
    assert!(captured.revalidate(&shell).is_err());
    assert!(
        super::super::prepare(
            &mut shell,
            &captured,
            &SlackClient::default(),
            &session(&source),
            "synthetic"
        )
        .is_err()
    );
    assert!(
        source
            .capture_channel(&shell, "choice", "pick", None, None)
            .is_err()
    );
}
#[test]
fn channel_enterprise_context_invalidates_cached_picker_and_sibling_preparation() {
    let (mut shell, source, metadata, mounted) = setup();
    let button = source.capture_button(&shell, "review", "submit").unwrap();
    let before = values(&mut shell, &button);
    let ticket = install_picker(&mut shell, source.clone(), metadata.clone(), mounted);
    assert!(directory::request_current(&shell, ticket, 5));
    let mut enterprise = session(&source);
    enterprise.enterprise_id = Some("E00000001".into());
    shell.core.session = Some(Session {
        unified_user_client_enabled: Default::default(),
        d_cookie: "synthetic".into(),
        workspaces: [("T1".into(), enterprise)].into(),
    });
    assert!(!presentation(&shell, &source, &metadata).enabled);
    assert_eq!(
        presentation(&shell, &source, &metadata).reason.as_deref(),
        Some(super::super::channel::UNAVAILABLE)
    );
    assert!(!directory::request_current(&shell, ticket, 5));
    assert!(super::super::candidate(&shell, &source).is_err());
    assert!(
        super::super::prepare(
            &mut shell,
            &button,
            &SlackClient::default(),
            &session(&source),
            "synthetic"
        )
        .is_err()
    );
    assert_eq!(
        shell.message_buttons.inputs[&ContainerKey::new(&source)]
            .model
            .values()
            .unwrap()
            .unwrap(),
        before["values"]
    );
    assert!(poll(&mut shell));
    assert!(shell.message_buttons.user_picker.is_none());
}
#[test]
fn remote_public_nonmember_retains_canonical_proof_without_join_and_newer_cache_wins() {
    let (mut shell, source, metadata, _) = setup();
    let f = filter(&shell, &source, &metadata).unwrap();
    let remote: super_platinum_core::slack::models::Channel = serde_json::from_value(
        json!({"id":"C00000099","name":"remote","is_channel":true,"is_member":false,"updated":20}),
    )
    .unwrap();
    let choices = conversations::candidates(
        &shell.core.workspaces["T1"],
        &f,
        std::iter::empty(),
        std::iter::once((&remote, true)),
        "remote",
    );
    assert_eq!(choices.len(), 1);
    assert!(
        !shell.core.workspaces["T1"]
            .channels
            .contains_key(&remote.id)
    );
    let captured = source
        .capture_channel(
            &shell,
            "choice",
            "pick",
            Some(&remote.id),
            Some(&choices[0]),
        )
        .unwrap();
    assert!(captured.revalidate(&shell).is_ok());
    let mut newer = remote.clone();
    newer.updated = Some(21);
    newer.is_private = true;
    shell
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .channels
        .insert(newer.id.clone(), newer);
    assert!(captured.revalidate(&shell).is_err());
}
#[test]
fn channel_uncertain_action_is_not_reoffered_and_consumed_display_cannot_replay() {
    let (mut shell, source, metadata, mounted) = setup();
    let mut captured = source
        .capture_channel(&shell, "choice", "pick", Some("C00000007"), None)
        .unwrap();
    captured.set_display(
        mounted
            .capture(&ContainerKey::new(&source), &field_key(&metadata).unwrap())
            .unwrap(),
    );
    values(&mut shell, &captured);
    assert!(
        super::super::prepare(
            &mut shell,
            &captured,
            &SlackClient::default(),
            &session(&source),
            "again"
        )
        .is_err()
    );
    let key = crate::message_buttons::Key::new(&source, &metadata).unwrap();
    shell.message_buttons.operations.insert(
        key,
        crate::message_buttons::Operation {
            captured,
            phase: crate::message_buttons::Phase::Uncertain,
            generation: 7,
        },
    );
    assert!(!presentation(&shell, &source, &metadata).enabled);
    assert!(
        presentation(&shell, &source, &metadata)
            .reason
            .unwrap()
            .contains("will not be sent again")
    );
}
