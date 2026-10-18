use super::*;
use crate::message_buttons::{inputs, multi_user_fixture, source::Bag, user_fixture};
use serde_json::{Value, json};
use std::sync::Arc;
use super_platinum_core::{
    config::{Session, WorkspaceSession},
    slack::{SlackClient, Transport, client::RequestBody},
};

fn setup() -> (ShellState, Source, BlockElementMetadata, display::Mounted) {
    setup_with_initial(&["U00000001"])
}
fn setup_with_initial(
    initial: &[&str],
) -> (ShellState, Source, BlockElementMetadata, display::Mounted) {
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    multi_user_fixture::install(&mut shell);
    let message = shell
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .messages
        .get_mut("C2")
        .unwrap()
        .messages
        .iter_mut()
        .find(|m| m.ts.as_deref() == Some(user_fixture::MAIN_TS))
        .unwrap();
    message.blocks[1]["accessory"]["initial_users"] = json!(initial);
    let w = &shell.core.workspaces["T1"];
    let message = w.messages["C2"]
        .messages
        .iter()
        .find(|m| m.ts.as_deref() == Some(user_fixture::MAIN_TS))
        .unwrap();
    let row = crate::message_vm::message_vm(w, message, &shell.media);
    let source = Source::capture(&shell, &row, "C2", Bag::Timeline).unwrap();
    let entry = inputs::candidate(&shell, &source).unwrap();
    shell
        .message_buttons
        .inputs
        .insert(ContainerKey::new(&source), entry);
    let metadata = source.users_metadata(&shell, "choice", "pick").unwrap();
    let mounted = display::Mounted::new();
    assert_eq!(
        display_selected_users(&shell, &source, &metadata, &mounted),
        initial
    );
    (shell, source, metadata, mounted)
}
pub(super) fn session() -> WorkspaceSession {
    WorkspaceSession {
        team_id: "T1".into(),
        enterprise_id: None,
        user_id: "U00000001".into(),
        name: "Fixture".into(),
        url: "https://example.invalid".into(),
        token: "synthetic-only".into(),
    }
}
fn prepare(shell: &mut ShellState, captured: &Captured) -> Value {
    let request = inputs::prepare(
        shell,
        captured,
        &SlackClient::default(),
        &session(),
        "multi-user-fixture",
    )
    .unwrap();
    assert!(!request.retry_safe());
    let RequestBody::Form(fields) = request.body else {
        panic!("form")
    };
    serde_json::from_str(&fields.iter().find(|(k, _)| k == "state").unwrap().1).unwrap()
}
fn install(
    shell: &mut ShellState,
    source: Source,
    metadata: BlockElementMetadata,
    mounted: display::Mounted,
) -> u64 {
    let multiple = selection(shell, &source, &metadata, &mounted);
    let revision = next_id();
    let picker = Picker {
        multiple,
        input_revision: inputs::revision(shell, &source).unwrap(),
        context: crate::scheduled::Context::capture(shell),
        filter: None,
        candidates: directory::cached(shell, &source, ""),
        display: mounted
            .capture(&ContainerKey::new(&source), &field_key(&metadata).unwrap())
            .unwrap(),
        source,
        metadata,
        mounted,
        revision,
        query_revision: 5,
        query: String::new(),
        loading: false,
        agent_unavailable: false,
        error: None,
    };
    shell.message_buttons.user_picker = Some(picker);
    shell.overlay = Some(Overlay::MessageUserSelect);
    revision
}
#[test]
fn staged_users_cancel_keeps_mounted_and_shared_values_and_checks_required_maximum() {
    let (mut shell, source, metadata, mounted) = setup();
    install(
        &mut shell,
        source.clone(),
        metadata.clone(),
        mounted.clone(),
    );
    let draft = shell
        .message_buttons
        .user_picker
        .as_mut()
        .unwrap()
        .multiple
        .as_mut()
        .unwrap();
    toggle(draft, "U00000001".into());
    assert!(error(draft).unwrap().contains("at least one"));
    for id in ["U00000001", "U00000002", "U00000003"] {
        toggle(draft, id.into());
    }
    assert!(error(draft).unwrap().contains("at most 2"));
    cancel_local(&mut shell);
    assert_eq!(
        display_selected_users(&shell, &source, &metadata, &mounted),
        vec!["U00000001"]
    );
    let button = source.capture_button(&shell, "review", "submit").unwrap();
    let values = prepare(&mut shell, &button);
    assert_eq!(
        values["values"]["choice"]["pick"],
        json!({"type":"multi_users_select","selected_users":["U00000001"]})
    );
    assert!(values["values"]["review"].get("other").is_none());
}
#[test]
fn prepared_multi_user_action_commits_exact_full_state_once_and_remount_keeps_it() {
    let (mut shell, source, metadata, mounted) = setup();
    let selected = vec!["U00000001".into(), "U00000002".into()];
    let mut captured = source
        .capture_users(&shell, "choice", "pick", &selected)
        .unwrap();
    captured.set_display(
        mounted
            .capture(&ContainerKey::new(&source), &field_key(&metadata).unwrap())
            .unwrap(),
    );
    let values = prepare(&mut shell, &captured);
    assert_eq!(
        values["values"]["choice"]["pick"],
        json!({"type":"multi_users_select","selected_users":selected})
    );
    assert_eq!(
        values["values"]["review"]["environment"]["selected_option"]["value"],
        "prod"
    );
    assert!(
        inputs::prepare(
            &mut shell,
            &captured,
            &SlackClient::default(),
            &session(),
            "duplicate"
        )
        .is_err()
    );
    let button = source.capture_button(&shell, "review", "submit").unwrap();
    assert_eq!(prepare(&mut shell, &button), values);
    mounted.unmount();
    assert_eq!(
        display_selected_users(&shell, &source, &metadata, &display::Mounted::new()),
        selected
    );
}
#[test]
fn owner_source_or_sibling_revision_changes_retire_multi_user_drafts() {
    let changes: [fn(&mut ShellState); 5] = [
        |s| s.core.account_epoch += 1,
        |s| s.channel_generation += 1,
        |s| s.core.workspaces.get_mut("T1").unwrap().rt_generation += 1,
        |s| s.core.workspaces.get_mut("T1").unwrap().self_user_id = "replacement".into(),
        |s| {
            s.message_buttons
                .inputs
                .values_mut()
                .next()
                .unwrap()
                .revision += 1
        },
    ];
    for change in changes {
        let (mut shell, source, metadata, mounted) = setup();
        install(&mut shell, source, metadata, mounted);
        assert!(picker(&shell).is_some());
        change(&mut shell);
        assert!(picker(&shell).is_none());
        assert!(poll(&mut shell));
        assert!(shell.message_buttons.user_picker.is_none());
    }
}
#[test]
fn multi_user_search_backoff_is_retained_and_old_queries_do_not_apply() {
    let (mut shell, source, metadata, mounted) = setup();
    shell.core.transport = Some(Arc::new(Transport::new("synthetic-cookie").unwrap()));
    shell.core.session = Some(Session {
        unified_user_client_enabled: Default::default(),
        d_cookie: "synthetic-cookie".into(),
        workspaces: [("T1".into(), session())].into(),
    });
    let revision = install(
        &mut shell,
        source.clone(),
        metadata.clone(),
        mounted.clone(),
    );
    let context = crate::scheduled::Context::capture(&shell).unwrap();
    let epoch = shell.core.account_epoch;
    directory::remember_cooldown(
        &mut shell,
        epoch,
        "T1",
        std::time::Instant::now() + std::time::Duration::from_secs(60),
    );
    assert!(!directory::admit_query(&mut shell, &context, revision, 5));
    cancel_local(&mut shell);
    let next = install(&mut shell, source, metadata, mounted);
    assert!(!directory::admit_query(&mut shell, &context, next, 5));
    shell.message_buttons.user_retry.clear();
    assert!(!directory::request_current(&shell, revision, 5));
    assert!(!directory::request_current(&shell, next, 4));
    assert!(directory::admit_query(&mut shell, &context, next, 5));
}

#[test]
fn reduced_maximum_keeps_picker_available_until_excess_initial_users_are_removed() {
    let (mut shell, source, metadata, mounted) =
        setup_with_initial(&["U00000001", "U00000002", "U00000003"]);
    assert!(presentation(&shell, &source, &metadata).enabled);
    install(
        &mut shell,
        source.clone(),
        metadata.clone(),
        mounted.clone(),
    );
    let view = picker(&shell).unwrap();
    assert_eq!(view.selected.len(), 3);
    assert_eq!(
        view.selection_error.as_deref(),
        Some("Choose at most 2 users")
    );
    assert!(
        source
            .capture_users(
                &shell,
                "choice",
                "pick",
                &["U00000001".into(), "U00000002".into(), "U00000003".into()]
            )
            .is_err()
    );
    let draft = shell
        .message_buttons
        .user_picker
        .as_mut()
        .unwrap()
        .multiple
        .as_mut()
        .unwrap();
    toggle(draft, "U00000003".into());
    let selected = draft.selected.clone();
    assert!(picker(&shell).unwrap().selection_error.is_none());
    let mut captured = source
        .capture_users(&shell, "choice", "pick", &selected)
        .unwrap();
    captured.set_display(
        mounted
            .capture(&ContainerKey::new(&source), &field_key(&metadata).unwrap())
            .unwrap(),
    );
    cancel_local(&mut shell);
    let values = prepare(&mut shell, &captured);
    assert_eq!(
        values["values"]["choice"]["pick"],
        json!({"type":"multi_users_select","selected_users":["U00000001","U00000002"]})
    );
    assert_eq!(
        values["values"]["review"]["environment"]["selected_option"]["value"],
        "prod"
    );
    let button = source.capture_button(&shell, "review", "submit").unwrap();
    assert_eq!(prepare(&mut shell, &button), values);
}

fn replace_maximum(shell: &mut ShellState, maximum: usize) -> Source {
    let message = shell
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .messages
        .get_mut("C2")
        .unwrap()
        .messages
        .iter_mut()
        .find(|m| m.ts.as_deref() == Some(user_fixture::MAIN_TS))
        .unwrap();
    message.blocks[1]["accessory"]["max_selected_items"] = json!(maximum);
    let workspace = &shell.core.workspaces["T1"];
    let message = workspace.messages["C2"]
        .messages
        .iter()
        .find(|m| m.ts.as_deref() == Some(user_fixture::MAIN_TS))
        .unwrap();
    let row = crate::message_vm::message_vm(workspace, message, &shell.media);
    let source = Source::capture(shell, &row, "C2", Bag::Timeline).unwrap();
    let entry = inputs::candidate(shell, &source).unwrap();
    shell
        .message_buttons
        .inputs
        .insert(ContainerKey::new(&source), entry);
    source
}

#[test]
fn canonical_maximum_shrink_preserves_initial_and_saved_users_until_an_explicit_commit() {
    for saved in [false, true] {
        let initial = if saved {
            vec!["U00000001"]
        } else {
            vec!["U00000001", "U00000002", "U00000003"]
        };
        let (mut shell, _, metadata, mounted) = setup_with_initial(&initial);
        let source = replace_maximum(&mut shell, 3);
        display_selected_users(&shell, &source, &metadata, &mounted);
        let three = vec![
            "U00000001".to_owned(),
            "U00000002".to_owned(),
            "U00000003".to_owned(),
        ];
        if saved {
            let mut captured = source
                .capture_users(&shell, "choice", "pick", &three)
                .unwrap();
            captured.set_display(
                mounted
                    .capture(&ContainerKey::new(&source), &field_key(&metadata).unwrap())
                    .unwrap(),
            );
            prepare(&mut shell, &captured);
        }
        let source = replace_maximum(&mut shell, 2);
        assert_eq!(
            display_selected_users(&shell, &source, &metadata, &mounted),
            three
        );
        let button = source.capture_button(&shell, "review", "submit").unwrap();
        assert_eq!(
            prepare(&mut shell, &button)["values"]["choice"]["pick"]["selected_users"],
            json!(three)
        );
        install(
            &mut shell,
            source.clone(),
            metadata.clone(),
            mounted.clone(),
        );
        assert_eq!(
            picker(&shell).unwrap().selection_error.as_deref(),
            Some("Choose at most 2 users")
        );
        let draft = shell
            .message_buttons
            .user_picker
            .as_mut()
            .unwrap()
            .multiple
            .as_mut()
            .unwrap();
        toggle(draft, "U00000003".into());
        let selected = draft.selected.clone();
        assert!(picker(&shell).unwrap().selection_error.is_none());
        let mut captured = source
            .capture_users(&shell, "choice", "pick", &selected)
            .unwrap();
        captured.set_display(
            mounted
                .capture(&ContainerKey::new(&source), &field_key(&metadata).unwrap())
                .unwrap(),
        );
        cancel_local(&mut shell);
        assert_eq!(
            prepare(&mut shell, &captured)["values"]["choice"]["pick"]["selected_users"],
            json!(selected)
        );
    }
}
