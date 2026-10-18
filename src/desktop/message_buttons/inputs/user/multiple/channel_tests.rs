use super::*;
use crate::message_buttons::{inputs, multi_channel_fixture, source::Bag, user_fixture};
use serde_json::json;

fn setup() -> (ShellState, Source, BlockElementMetadata, display::Mounted) {
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    multi_channel_fixture::install(&mut shell);
    let w = &shell.core.workspaces["T1"];
    let m = w.messages["C2"]
        .messages
        .iter()
        .find(|m| m.ts.as_deref() == Some(user_fixture::MAIN_TS))
        .unwrap();
    let row = crate::message_vm::message_vm(w, m, &shell.media);
    let source = Source::capture(&shell, &row, "C2", Bag::Timeline).unwrap();
    let entry = inputs::candidate(&shell, &source).unwrap();
    shell
        .message_buttons
        .inputs
        .insert(ContainerKey::new(&source), entry);
    let metadata = source.channels_metadata(&shell, "choice", "pick").unwrap();
    let mounted = display::Mounted::new();
    assert_eq!(
        display_selected_users(&shell, &source, &metadata, &mounted),
        vec!["C00000001"]
    );
    (shell, source, metadata, mounted)
}
#[test]
fn channels_stage_cancel_max_and_full_sibling_commit_are_distinct_from_users() {
    let (mut shell, source, metadata, mounted) = setup();
    let mut draft = selection(&shell, &source, &metadata, &mounted).unwrap();
    toggle(&mut draft, "C00000007".into());
    toggle(&mut draft, "C00000008".into());
    assert!(error(&draft).unwrap().contains("at most 2 channels"));
    assert_eq!(
        display_selected_users(&shell, &source, &metadata, &mounted),
        vec!["C00000001"]
    );
    toggle(&mut draft, "C00000008".into());
    let mut captured = source
        .capture_channels(&shell, "choice", "pick", &draft.selected, &draft.proofs)
        .unwrap();
    assert!(captured.confirmation().is_some());
    captured.set_display(
        mounted
            .capture(&ContainerKey::new(&source), &field_key(&metadata).unwrap())
            .unwrap(),
    );
    let request = inputs::prepare(
        &mut shell,
        &captured,
        &super_platinum_core::slack::SlackClient::default(),
        &super::tests::session(),
        "channels-test",
    )
    .unwrap();
    assert!(!request.retry_safe());
    let super_platinum_core::slack::client::RequestBody::Form(fields) = request.body else {
        panic!("form")
    };
    let wire: serde_json::Value =
        serde_json::from_str(&fields.iter().find(|(k, _)| k == "state").unwrap().1).unwrap();
    assert_eq!(
        wire["values"]["choice"]["pick"],
        json!({"type":"multi_channels_select","selected_channels":["C00000001","C00000007"]})
    );
    assert!(wire["values"]["review"].get("other").is_none());
    assert_eq!(
        wire["values"]["review"]["environment"]["selected_option"]["value"],
        "prod"
    );
    assert!(
        inputs::prepare(
            &mut shell,
            &captured,
            &super_platinum_core::slack::SlackClient::default(),
            &super::tests::session(),
            "duplicate"
        )
        .is_err()
    );
}
#[test]
fn every_selection_revalidated_after_confirmation_and_remote_query_changes() {
    let (mut shell, source, _, _) = setup();
    let mut proofs = inputs::channels::Proofs::new();
    let remote = serde_json::from_value(
        json!({"id":"C99999999","name":"Remote","is_channel":true,"updated":1}),
    )
    .unwrap();
    let selected = vec!["C00000001".into(), "C99999999".into()];
    inputs::channels::refresh(&mut proofs, &selected, &[remote]);
    let action = source
        .capture_channels(&shell, "choice", "pick", &selected, &proofs)
        .unwrap();
    assert!(action.revalidate(&shell).is_ok());
    let remote_archived = serde_json::from_value(
        json!({"id":"C99999999","name":"Remote","is_channel":true,"is_archived":true,"updated":2}),
    )
    .unwrap();
    inputs::channels::refresh(&mut proofs, &selected, &[remote_archived]);
    assert!(
        source
            .capture_channels(&shell, "choice", "pick", &selected, &proofs)
            .is_err()
    );
    shell
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .channels
        .get_mut("C00000001")
        .unwrap()
        .is_archived = true;
    assert!(action.revalidate(&shell).is_err());
    assert!(
        source
            .capture_channels(
                &shell,
                "choice",
                "pick",
                &["C88888888".into()],
                &Default::default()
            )
            .is_err()
    );
}
#[test]
fn multi_channels_allow_enterprise_but_context_change_retires_capture() {
    let (mut shell, source, _, _) = setup();
    let mut session = super::tests::session();
    session.enterprise_id = Some("E_MULTI".into());
    shell.core.session = Some(super_platinum_core::config::Session {
        unified_user_client_enabled: Default::default(),
        d_cookie: "synthetic-cookie".into(),
        workspaces: [("T1".into(), session)].into(),
    });
    // The source captures account identity, so capture again after session installation.
    let w = &shell.core.workspaces["T1"];
    let m = source.message(&shell).unwrap();
    let row = crate::message_vm::message_vm(w, m, &shell.media);
    let source = Source::capture(&shell, &row, "C2", Bag::Timeline).unwrap();
    let action = source
        .capture_channels(
            &shell,
            "choice",
            "pick",
            &["C00000001".into()],
            &Default::default(),
        )
        .unwrap();
    assert!(action.revalidate(&shell).is_ok());
    shell
        .core
        .session
        .as_mut()
        .unwrap()
        .workspaces
        .get_mut("T1")
        .unwrap()
        .enterprise_id = Some("E_REPLACED".into());
    assert!(action.revalidate(&shell).is_err());
}

#[test]
fn accepted_channel_metadata_survives_cancel_reopen_without_committing_staged_wire() {
    let (mut shell, source, metadata, mounted) = setup();
    let selected = vec!["C99999999".into()];
    let remote = |archived, updated| {
        serde_json::from_value(json!({"id":"C99999999","name":"remote","is_channel":true,"is_archived":archived,"updated":updated})).unwrap()
    };
    let mut proofs = inputs::channels::Proofs::new();
    inputs::channels::refresh(&mut proofs, &selected, &[remote(false, 1)]);
    let captured = source
        .capture_channels(&shell, "choice", "pick", &selected, &proofs)
        .unwrap();
    inputs::prepare(
        &mut shell,
        &captured,
        &super_platinum_core::slack::SlackClient::default(),
        &super::tests::session(),
        "retained",
    )
    .unwrap();
    let remounted = display::Mounted::new();
    let mut staged = selection(&shell, &source, &metadata, &remounted).unwrap();
    assert_eq!(staged.selected, selected);
    let archived = remote(true, 2);
    inputs::channels::refresh_retained(&mut shell, source.owner(), &[archived.clone()]);
    inputs::channels::refresh(&mut staged.proofs, &staged.selected, &[archived]);
    assert!(
        source
            .capture_channels(&shell, "choice", "pick", &staged.selected, &staged.proofs)
            .is_err()
    );
    drop(staged); // Cancel publishes no selected values.
    let reopened = selection(&shell, &source, &metadata, &remounted).unwrap();
    assert_eq!(reopened.selected, selected);
    assert!(
        source
            .capture_channels(
                &shell,
                "choice",
                "pick",
                &reopened.selected,
                &reopened.proofs
            )
            .is_err()
    );
    let entry = &shell.message_buttons.inputs[&ContainerKey::new(&source)];
    assert_eq!(
        entry.model.saved_channels(&field_key(&metadata).unwrap()),
        Some(selected)
    );
    assert_eq!(
        display_selected_users(&shell, &source, &metadata, &mounted),
        vec!["C00000001"]
    );
}

#[test]
fn initial_prop_reset_preserves_saved_wire_but_never_revives_known_archived_remote_choice() {
    let (mut shell, source, _, _) = setup();
    let selected = vec!["C99999999".into()];
    let remote = |archived, updated| {
        serde_json::from_value(json!({"id":"C99999999","name":"remote","is_channel":true,"is_archived":archived,"updated":updated})).unwrap()
    };
    let mut proofs = inputs::channels::Proofs::new();
    inputs::channels::refresh(&mut proofs, &selected, &[remote(false, 1)]);
    let captured = source
        .capture_channels(&shell, "choice", "pick", &selected, &proofs)
        .unwrap();
    inputs::prepare(
        &mut shell,
        &captured,
        &super_platinum_core::slack::SlackClient::default(),
        &super::tests::session(),
        "saved-before-props",
    )
    .unwrap();
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
    message.blocks[1]["accessory"]["initial_channels"] = json!(["C00000007"]);
    let w = &shell.core.workspaces["T1"];
    let message = w.messages["C2"]
        .messages
        .iter()
        .find(|m| m.ts.as_deref() == Some(user_fixture::MAIN_TS))
        .unwrap();
    let row = crate::message_vm::message_vm(w, message, &shell.media);
    let source = Source::capture(&shell, &row, "C2", Bag::Timeline).unwrap();
    let entry = inputs::candidate(&shell, &source).unwrap();
    let metadata = source.channels_metadata(&shell, "choice", "pick").unwrap();
    let key = field_key(&metadata).unwrap();
    assert_eq!(
        entry.model.fields[&key].value,
        InputValue::Channels(vec!["C00000007".into()])
    );
    assert_eq!(entry.model.saved_channels(&key), Some(selected.clone()));
    shell
        .message_buttons
        .inputs
        .insert(ContainerKey::new(&source), entry);
    inputs::channels::refresh_retained(&mut shell, source.owner(), &[remote(true, 2)]);
    // Cancelling any picker cannot discard the accepted canonical metadata.
    cancel_local(&mut shell);
    let remounted = display::Mounted::new();
    let draft = selection(&shell, &source, &metadata, &remounted).unwrap();
    assert_eq!(draft.selected, selected);
    assert!(
        source
            .capture_channels(&shell, "choice", "pick", &draft.selected, &draft.proofs)
            .is_err()
    );
    // A different multi-channel action may commit, but cannot prune the saved
    // first field's invalidation proof while that field still has saved wire.
    let clear = source
        .capture_channels(&shell, "review", "other", &[], &Default::default())
        .unwrap();
    inputs::prepare(
        &mut shell,
        &clear,
        &super_platinum_core::slack::SlackClient::default(),
        &super::tests::session(),
        "other-clear",
    )
    .unwrap();
    let entry = &shell.message_buttons.inputs[&ContainerKey::new(&source)];
    assert_eq!(entry.model.saved_channels(&key), Some(selected));
    assert_eq!(
        entry.channel_choices["C99999999"]
            .canonical
            .as_ref()
            .unwrap()["is_archived"],
        json!(true)
    );
}
