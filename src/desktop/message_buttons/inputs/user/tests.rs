use super::*;
use crate::message_buttons::{source::Bag, user_fixture};
use serde_json::{Value, json};
use std::sync::Arc;
use super_platinum_core::{
    config::{Session, WorkspaceSession},
    slack::{SlackClient, Transport, client::RequestBody, models::User},
};
fn setup() -> (ShellState, Source, BlockElementMetadata, display::Mounted) {
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    user_fixture::install(&mut shell);
    let w = &shell.core.workspaces["T1"];
    let message = w.messages["C2"]
        .messages
        .iter()
        .find(|m| m.ts.as_deref() == Some(user_fixture::MAIN_TS))
        .unwrap();
    let row = crate::message_vm::message_vm(w, message, &shell.media);
    let source = Source::capture(&shell, &row, "C2", Bag::Timeline).unwrap();
    let entry = super::super::candidate(&shell, &source).unwrap();
    shell
        .message_buttons
        .inputs
        .insert(ContainerKey::new(&source), entry);
    let metadata = source.user_metadata(&shell, "choice", "pick").unwrap();
    let mounted = display::Mounted::new();
    assert_eq!(
        super::super::display_selected(&shell, &source, &metadata, &mounted).as_deref(),
        Some("U00000001")
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
        input_revision: super::super::revision(shell, &source).unwrap(),
        context: crate::scheduled::Context::capture(shell),
        filter: None,
        candidates: directory::cached(shell, &source, ""),
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
fn users_confirm_cancel_preserves_wire_and_prepare_commits_full_siblings() {
    let (mut shell, source, metadata, mounted) = setup();
    let button = source.capture_button(&shell, "review", "submit").unwrap();
    let initial = values(&mut shell, &button);
    assert_eq!(
        initial["values"]["choice"]["pick"],
        json!({"type":"users_select","selected_user":"U00000001"})
    );
    assert!(initial["values"]["review"].get("other").is_none());
    let mut captured = source
        .capture_user(&shell, "choice", "pick", Some("U00000002"))
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
        Some("U00000001")
    );
    let mut captured = source
        .capture_user(&shell, "choice", "pick", Some("U00000002"))
        .unwrap();
    captured.set_display(
        mounted
            .capture(&ContainerKey::new(&source), &field_key(&metadata).unwrap())
            .unwrap(),
    );
    let next = values(&mut shell, &captured);
    assert_eq!(
        next["values"]["choice"]["pick"]["selected_user"],
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
fn remote_user_labels_are_owner_scoped_and_new_canonical_profiles_win() {
    let (mut shell, source, _, _) = setup();
    let id = "W99999999";
    let owner = source.owner();
    let key = (
        owner.account_epoch,
        owner.team_id.clone(),
        owner.self_user_id.clone(),
        id.into(),
    );
    shell
        .message_buttons
        .user_labels
        .insert(key.clone(), "Remote person".into());
    assert_eq!(label(&shell, &source, id), "Remote person");
    let profile: User = serde_json::from_value(
        json!({"id":id,"name":"current-name","profile":{"display_name":"Current name"}}),
    )
    .unwrap();
    shell
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .users
        .insert(id.into(), profile);
    assert_eq!(label(&shell, &source, id), "Current name");
    shell
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .users
        .remove(id);
    shell.message_buttons.user_labels.clear();
    let mut wrong = key;
    wrong.0 += 1;
    shell
        .message_buttons
        .user_labels
        .insert(wrong, "Other account".into());
    assert_eq!(label(&shell, &source, id), id);
}
#[test]
fn candidate_policy_includes_bots_and_remote_fuzzy_hits_but_excludes_special_users() {
    let (shell, source, _, _) = setup();
    let workspace = &shell.core.workspaces["T1"];
    let choices = directory::cached(&shell, &source, "");
    for id in ["U00000001", "U00000005", "USLACKBOT", "USLACKSECURITY"] {
        assert!(choices.iter().any(|c| c.id == id), "{id}");
    }
    let remote: User =
        serde_json::from_value(json!({"id":"W99999999","name":"Fuzzy Person","is_bot":true}))
            .unwrap();
    assert!(
        directory::candidates(workspace, [(&remote, true)].into_iter(), "fzzy")
            .iter()
            .any(|c| c.id == remote.id)
    );
    assert!(directory::candidates(workspace, [(&remote, false)].into_iter(), "fzzy").is_empty());
    for (key, value) in [
        ("is_workflow_bot", json!(true)),
        ("is_profile_only_user", json!(true)),
        ("is_agentforce_bot", json!(true)),
        ("deleted", json!(true)),
    ] {
        let mut raw = json!({"id":"W99999999","name":"Excluded"});
        raw[key] = value;
        assert!(!directory::eligible(
            workspace,
            &serde_json::from_value(raw).unwrap()
        ));
    }
}

#[test]
fn shorter_late_backoffs_never_shorten_a_retained_retry_after() {
    let (mut shell, _, _, _) = setup();
    let epoch = shell.core.account_epoch;
    let long = std::time::Instant::now() + std::time::Duration::from_secs(100);
    directory::remember_cooldown(&mut shell, epoch, "T1", long);
    directory::remember_cooldown(
        &mut shell,
        epoch,
        "T1",
        std::time::Instant::now() + std::time::Duration::from_secs(1),
    );
    assert_eq!(
        shell.message_buttons.user_retry[&(epoch, "T1".into())],
        long
    );
}
