use super::*;
use crate::message_buttons::{inputs, multi_conversation_fixture, source::Bag, user_fixture};
use serde_json::json;

fn setup() -> (ShellState, Source, BlockElementMetadata, display::Mounted) {
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    multi_conversation_fixture::install(&mut shell);
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
    let metadata = source
        .conversations_metadata(&shell, "choice", "pick")
        .unwrap();
    let mounted = display::Mounted::new();
    assert_eq!(
        display_selected_users(&shell, &source, &metadata, &mounted),
        vec!["C00000001"]
    );
    (shell, source, metadata, mounted)
}

#[test]
fn conversation_array_prepares_member_ids_and_private_channels_in_order_with_full_siblings() {
    let (mut shell, source, metadata, mounted) = setup();
    let selected = vec!["G00000002".into(), "U00000002".into()];
    let mut captured = source
        .capture_conversations(&shell, "choice", "pick", &selected, &Default::default())
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
        "conversations",
    )
    .unwrap();
    assert!(!request.retry_safe());
    let super_platinum_core::slack::client::RequestBody::Form(fields) = request.body else {
        panic!("form");
    };
    let wire: serde_json::Value =
        serde_json::from_str(&fields.iter().find(|(k, _)| k == "state").unwrap().1).unwrap();
    assert_eq!(
        wire["values"]["choice"]["pick"],
        json!({"type":"multi_conversations_select","selected_conversations":selected})
    );
    assert_eq!(
        wire["values"]["review"]["environment"]["selected_option"]["value"],
        "prod"
    );
    assert!(wire["values"]["review"].get("other").is_none());
    assert_eq!(
        display_selected_users(&shell, &source, &metadata, &display::Mounted::new()),
        selected
    );
    assert_eq!(
        shell.core.workspaces["T1"]
            .channels
            .get("D00000006")
            .unwrap()
            .is_im,
        true
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
fn all_selected_canonical_entities_and_filter_are_rechecked_before_confirmation_dispatch() {
    let (mut shell, source, _, _) = setup();
    let selected = vec!["G00000002".into(), "U00000002".into()];
    let captured = source
        .capture_conversations(&shell, "choice", "pick", &selected, &Default::default())
        .unwrap();
    shell
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .users
        .get_mut("U00000002")
        .unwrap()
        .deleted = true;
    assert!(captured.revalidate(&shell).is_err());
    assert!(
        source
            .capture_conversations(
                &shell,
                "choice",
                "pick",
                &["U88888888".into()],
                &Default::default()
            )
            .is_err()
    );
    assert!(
        source
            .capture_conversations(
                &shell,
                "review",
                "other",
                &["C00000004".into()],
                &Default::default()
            )
            .is_err()
    );
    assert!(
        source
            .capture_conversations(
                &shell,
                "review",
                "other",
                &["U00000005".into()],
                &Default::default()
            )
            .is_err()
    );
    assert!(
        source
            .capture_conversations(
                &shell,
                "choice",
                "pick",
                &["D00000006".into()],
                &Default::default()
            )
            .is_err()
    );
    assert!(
        source
            .capture_conversations(&shell, "choice", "pick", &[], &Default::default())
            .is_err()
    );
    assert!(
        source
            .capture_conversations(&shell, "review", "other", &[], &Default::default())
            .is_ok()
    );
}
#[test]
fn ordinary_true_retains_supplied_initial_and_navigation_cannot_inject_a_default() {
    let (mut shell, _, _, _) = setup();
    let m = shell
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
    m.blocks[1]["accessory"]["default_to_current_conversation"] = json!(true);
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
    let metadata = source
        .conversations_metadata(&shell, "choice", "pick")
        .unwrap();
    assert!(presentation(&shell, &source, &metadata).enabled);
    assert_eq!(source.owner().channel_id, "C2");
    assert_eq!(shell.core.active_channel.as_deref(), Some("C2"));
    let mounted = display::Mounted::new();
    assert_eq!(
        display_selected_users(&shell, &source, &metadata, &mounted),
        vec!["C00000001"]
    );
    let captured = source
        .capture_conversations(
            &shell,
            "choice",
            "pick",
            &["C00000001".into()],
            &Default::default(),
        )
        .unwrap();
    // The source guard still owns dispatch. Navigation supplies neither a new
    // default nor permission to submit an action from a previous conversation.
    for later in [None, Some("C00000002"), Some("D00000006")] {
        shell.core.active_channel = later.map(str::to_owned);
        assert!(captured.revalidate(&shell).is_err());
        assert_eq!(
            shell.message_buttons.inputs[&ContainerKey::new(&source)]
                .model
                .fields[&field_key(&metadata).unwrap()]
                .initial_value,
            InputValue::Conversations(vec!["C00000001".into()])
        );
    }
    shell.core.active_channel = Some("C2".into());
    let request = inputs::prepare(
        &mut shell,
        &captured,
        &super_platinum_core::slack::SlackClient::default(),
        &super::tests::session(),
        "ordinary-flag",
    )
    .unwrap();
    let super_platinum_core::slack::client::RequestBody::Form(fields) = request.body else {
        panic!("form");
    };
    let wire: serde_json::Value =
        serde_json::from_str(&fields.iter().find(|(k, _)| k == "state").unwrap().1).unwrap();
    assert_eq!(
        wire["values"]["choice"]["pick"],
        json!({"type":"multi_conversations_select", "selected_conversations":["C00000001"]})
    );
    assert_eq!(
        wire["values"]["review"]["environment"]["selected_option"]["value"],
        "prod"
    );
    assert_eq!(
        display_selected_users(&shell, &source, &metadata, &display::Mounted::new()),
        vec!["C00000001"]
    );
}
#[test]
fn initial_prop_reset_preserves_saved_wire_but_never_revives_known_archived_remote_choice() {
    let (mut shell, source, _, _) = setup();
    let selected = vec!["U99999999".into()];
    let remote = |archived, updated| {
        serde_json::from_value(
            json!({"id":"U99999999","name":"remote","deleted":archived,"updated":updated}),
        )
        .unwrap()
    };
    let mut proofs = inputs::channels::Proofs::new();
    inputs::channels::refresh_users(&mut proofs, &selected, &[remote(false, 1)]);
    let captured = source
        .capture_conversations(&shell, "choice", "pick", &selected, &proofs)
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
    message.blocks[1]["accessory"]["initial_conversations"] = json!(["G00000002"]);
    let w = &shell.core.workspaces["T1"];
    let message = w.messages["C2"]
        .messages
        .iter()
        .find(|m| m.ts.as_deref() == Some(user_fixture::MAIN_TS))
        .unwrap();
    let row = crate::message_vm::message_vm(w, message, &shell.media);
    let source = Source::capture(&shell, &row, "C2", Bag::Timeline).unwrap();
    let entry = inputs::candidate(&shell, &source).unwrap();
    let metadata = source
        .conversations_metadata(&shell, "choice", "pick")
        .unwrap();
    let key = field_key(&metadata).unwrap();
    assert_eq!(
        entry.model.fields[&key].value,
        InputValue::Conversations(vec!["G00000002".into()])
    );
    assert_eq!(
        entry.model.saved_conversations(&key),
        Some(selected.clone())
    );
    shell
        .message_buttons
        .inputs
        .insert(ContainerKey::new(&source), entry);
    inputs::channels::refresh_retained_users(&mut shell, source.owner(), &[remote(true, 2)]);
    // Cancelling any picker cannot discard the accepted canonical metadata.
    cancel_local(&mut shell);
    let remounted = display::Mounted::new();
    let draft = selection(&shell, &source, &metadata, &remounted).unwrap();
    assert_eq!(draft.selected, selected);
    assert!(
        source
            .capture_conversations(&shell, "choice", "pick", &draft.selected, &draft.proofs)
            .is_err()
    );
    // A different multi-channel action may commit, but cannot prune the saved
    // first field's invalidation proof while that field still has saved wire.
    let clear = source
        .capture_conversations(&shell, "review", "other", &[], &Default::default())
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
    assert_eq!(entry.model.saved_conversations(&key), Some(selected));
    assert_eq!(
        entry.channel_choices["U99999999"]
            .canonical
            .as_ref()
            .unwrap()["deleted"],
        json!(true)
    );
}
