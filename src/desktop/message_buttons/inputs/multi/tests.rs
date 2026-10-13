use super::*;
use crate::message_buttons::{radio_fixture, source::Bag};
use serde_json::json;
use super_platinum_core::{
    block_actions::form::parse_message_multi,
    config::WorkspaceSession,
    slack::{SlackClient, client::RequestBody, models::Message},
};

fn option(value: &str) -> Value {
    json!({"value":value,"text":{"type":"plain_text","text":value},"description":{"type":"plain_text","text":"Preserved raw description"}})
}
fn key() -> InputKey {
    InputKey {
        block_id: "choice".into(),
        action_id: "pick".into(),
    }
}
fn owner() -> ContainerKey {
    ContainerKey {
        epoch: 1,
        user: "U1".into(),
        team: "T1".into(),
        channel: "C2".into(),
        ts: "1.0".into(),
    }
}
fn field(kind: &str, initial: Value) -> super_platinum_core::block_actions::form::FormField {
    parse_message_multi(&json!({"type":kind,"action_id":"pick","initial_options":initial,"options":[option("a"),option("b")]})).unwrap()
}
#[test]
fn mounts_preserve_raw_independent_display_and_empty_saved_array() {
    for kind in ["checkboxes", "multi_static_select"] {
        let field = field(kind, json!([option("a")]));
        let a = display::Mounted::new();
        let b = display::Mounted::new();
        for mount in [&a, &b] {
            assert_eq!(
                mount.sync_multi(owner(), key(), &field, None),
                vec![option("a")]
            );
        }
        a.capture(&owner(), &key())
            .unwrap()
            .commit_multi(&[option("b")]);
        assert_eq!(
            a.sync_multi(owner(), key(), &field, Some(&[option("b")])),
            vec![option("b")]
        );
        assert_eq!(
            b.sync_multi(owner(), key(), &field, Some(&[option("b")])),
            vec![option("a")]
        );
        assert!(
            display::Mounted::new()
                .sync_multi(owner(), key(), &field, Some(&[]))
                .is_empty()
        );
    }
}
#[test]
fn checkbox_ignores_props_multi_only_resets_nonnull_initial_and_never_prunes_raw() {
    for kind in ["checkboxes", "multi_static_select"] {
        let mount = display::Mounted::new();
        mount.sync_multi(owner(), key(), &field(kind, json!([option("a")])), None);
        let mut changed = field(kind, Value::Null);
        changed.raw["options"] = json!([option("b")]);
        assert_eq!(
            mount.sync_multi(owner(), key(), &changed, None),
            vec![option("a")]
        );
        changed.raw["initial_options"] = json!([]);
        let value = mount.sync_multi(owner(), key(), &changed, None);
        assert_eq!(
            value,
            if kind == "checkboxes" {
                vec![option("a")]
            } else {
                vec![]
            }
        );
    }
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
        .find(|m| m.ts.as_deref() == Some(radio_fixture::MAIN_TS))
        .unwrap()
}
fn setup() -> (ShellState, Source, BlockElementMetadata, display::Mounted) {
    setup_initial(true)
}
fn setup_initial(initial: bool) -> (ShellState, Source, BlockElementMetadata, display::Mounted) {
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    radio_fixture::install(&mut shell);
    canonical(&mut shell).blocks[1]["accessory"] = json!({"type":"multi_static_select","action_id":"pick","options":[option("a"),option("b")],"initial_options":[option("a")],"max_selected_items":1});
    if !initial {
        canonical(&mut shell).blocks[1]["accessory"]
            .as_object_mut()
            .unwrap()
            .remove("initial_options");
    }
    let workspace = &shell.core.workspaces["T1"];
    let message = workspace.messages["C2"]
        .messages
        .iter()
        .find(|m| m.ts.as_deref() == Some(radio_fixture::MAIN_TS))
        .unwrap();
    let row = crate::message_vm::message_vm(workspace, message, &shell.media);
    let source = Source::capture(&shell, &row, "C2", Bag::Timeline).unwrap();
    let entry = candidate(&shell, &source).unwrap();
    shell
        .message_buttons
        .inputs
        .insert(ContainerKey::new(&source), entry);
    let metadata = source.multi_metadata(&shell, "choice", "pick").unwrap();
    let mounted = display::Mounted::new();
    display_selected(&shell, &source, &metadata, &mounted);
    (shell, source, metadata, mounted)
}
fn install_picker(
    shell: &mut ShellState,
    source: &Source,
    metadata: &BlockElementMetadata,
    mount: &display::Mounted,
) {
    shell.message_buttons.multi_picker = Some(Picker {
        source: source.clone(),
        metadata: metadata.clone(),
        display: mount.capture(&ContainerKey::new(source), &key()).unwrap(),
        selected: mount
            .retained_multi(&ContainerKey::new(source), &key())
            .unwrap(),
        selected_present: mount
            .options_present(&ContainerKey::new(source), &key())
            .unwrap(),
        retained: mount
            .retained_multi(&ContainerKey::new(source), &key())
            .unwrap(),
        input_revision: revision(shell, source).unwrap(),
        revision: next_id(),
    });
    shell.overlay = Some(Overlay::MessageMultiSelect);
}
#[test]
fn draft_cancel_and_over_limit_do_not_commit_and_source_changes_invalidate() {
    let (mut shell, source, metadata, mount) = setup();
    install_picker(&mut shell, &source, &metadata, &mount);
    shell
        .message_buttons
        .multi_picker
        .as_mut()
        .unwrap()
        .selected
        .push(option("b"));
    assert!(!picker(&shell).unwrap().enabled);
    assert!(
        shell.message_buttons.inputs[&ContainerKey::new(&source)]
            .model
            .values()
            .unwrap()
            .is_none()
    );
    cancel_local(&mut shell);
    assert_eq!(
        display_selected(&shell, &source, &metadata, &mount),
        vec![option("a")]
    );
    install_picker(&mut shell, &source, &metadata, &mount);
    shell.channel_generation += 1;
    assert!(poll(&mut shell));
    assert!(shell.overlay.is_none());
    assert!(
        shell.message_buttons.inputs[&ContainerKey::new(&source)]
            .model
            .values()
            .unwrap()
            .is_none()
    );
}
#[test]
fn prepared_request_commits_only_origin_and_unmounted_attempt_cannot_publish() {
    let (mut shell, source, metadata, mount) = setup();
    let other = display::Mounted::new();
    display_selected(&shell, &source, &metadata, &other);
    let captured = capture(
        &shell,
        &source,
        &metadata,
        &[option("b")],
        &[option("a")],
        mount.capture(&ContainerKey::new(&source), &key()).unwrap(),
    )
    .unwrap();
    let session = WorkspaceSession {
        team_id: "T1".into(),
        enterprise_id: None,
        user_id: source.owner().self_user_id.clone(),
        name: "Fixture".into(),
        url: "https://example.test".into(),
        token: "synthetic".into(),
    };
    let request = prepare(
        &mut shell,
        &captured,
        &SlackClient::default(),
        &session,
        "synthetic",
    )
    .unwrap();
    let RequestBody::Form(fields) = request.body else {
        panic!()
    };
    let state: Value =
        serde_json::from_str(&fields.iter().find(|(k, _)| k == "state").unwrap().1).unwrap();
    assert_eq!(
        state["values"]["choice"]["pick"]["selected_options"],
        json!([option("b")])
    );
    assert_eq!(
        display_selected(&shell, &source, &metadata, &mount),
        vec![option("b")]
    );
    assert_eq!(
        display_selected(&shell, &source, &metadata, &other),
        vec![option("a")]
    );
    let late = capture(
        &shell,
        &source,
        &metadata,
        &[],
        &[option("a")],
        other.capture(&ContainerKey::new(&source), &key()).unwrap(),
    )
    .unwrap();
    other.unmount();
    assert!(
        prepare(
            &mut shell,
            &late,
            &SlackClient::default(),
            &session,
            "synthetic"
        )
        .is_err()
    );
    assert_eq!(
        shell.message_buttons.inputs[&ContainerKey::new(&source)]
            .model
            .values()
            .unwrap()
            .unwrap()["choice"]["pick"]["selected_options"],
        json!([option("b")])
    );
}

#[test]
fn checkbox_confirmation_cancel_keeps_display_and_wire_unchanged() {
    let (mut shell, _, _, _) = setup();
    let raw = &mut canonical(&mut shell).blocks[1]["accessory"];
    raw["type"] = json!("checkboxes");
    raw.as_object_mut().unwrap().remove("max_selected_items");
    raw["confirm"] = json!({"title":{"type":"plain_text","text":"Apply choices?"},"text":{"type":"plain_text","text":"Review choices"},"confirm":{"type":"plain_text","text":"Go"},"deny":{"type":"plain_text","text":"Cancel"}});
    let workspace = &shell.core.workspaces["T1"];
    let message = workspace.messages["C2"]
        .messages
        .iter()
        .find(|m| m.ts.as_deref() == Some(radio_fixture::MAIN_TS))
        .unwrap();
    let row = crate::message_vm::message_vm(workspace, message, &shell.media);
    let source = Source::capture(&shell, &row, "C2", Bag::Timeline).unwrap();
    let entry = candidate(&shell, &source).unwrap();
    shell
        .message_buttons
        .inputs
        .insert(ContainerKey::new(&source), entry);
    let metadata = source.multi_metadata(&shell, "choice", "pick").unwrap();
    let mounted = display::Mounted::new();
    display_selected(&shell, &source, &metadata, &mounted);
    let captured = capture(
        &shell,
        &source,
        &metadata,
        &[option("b")],
        &[option("a")],
        mounted
            .capture(&ContainerKey::new(&source), &key())
            .unwrap(),
    )
    .unwrap();
    assert!(captured.confirmation().is_some());
    let pending = Pending {
        captured,
        revision: next_id(),
        input_revision: revision(&shell, &source),
    };
    assert!(pending_current(&shell, &pending));
    shell.message_buttons.confirmation = Some(pending);
    shell.overlay = Some(Overlay::ButtonConfirmation);
    super::super::super::cancel_local(&mut shell);
    assert_eq!(
        display_selected(&shell, &source, &metadata, &mounted),
        vec![option("a")]
    );
    assert!(
        shell.message_buttons.inputs[&ContainerKey::new(&source)]
            .model
            .values()
            .unwrap()
            .is_none()
    );
}

#[test]
fn rebinding_or_unmounting_retires_raw_multi_authority() {
    let mount = display::Mounted::new();
    let field = field("checkboxes", json!([option("a")]));
    mount.sync_multi(owner(), key(), &field, None);
    let action = mount.capture(&owner(), &key()).unwrap();
    let mut replacement = owner();
    replacement.epoch += 1;
    mount.sync_multi(replacement, key(), &field, None);
    assert!(!action.current());
    assert!(!action.commit_multi(&[option("b")]));
    mount.sync_multi(owner(), key(), &field, None);
    let action = mount.capture(&owner(), &key()).unwrap();
    mount.unmount();
    assert!(!action.commit_multi(&[]));
}

#[test]
fn untouched_absence_omits_wire_selection_but_toggled_empty_is_explicit() {
    for touched in [false, true] {
        let (mut shell, source, metadata, mount) = setup_initial(false);
        assert_eq!(
            mount.options_present(&ContainerKey::new(&source), &key()),
            Some(false)
        );
        install_picker(&mut shell, &source, &metadata, &mount);
        let generation = shell
            .message_buttons
            .multi_picker
            .as_ref()
            .unwrap()
            .revision;
        if touched {
            toggle_local(&mut shell, generation, "a".into());
            toggle_local(&mut shell, generation, "a".into());
        }
        let picker = shell.message_buttons.multi_picker.take().unwrap();
        assert!(picker.selected.is_empty());
        assert_eq!(picker.selected_present, touched);
        let captured = capture_optional(
            &shell,
            &source,
            &metadata,
            picker
                .selected_present
                .then_some(picker.selected.as_slice()),
            &picker.retained,
            picker.display,
        )
        .unwrap();
        shell.overlay = None;
        let session = WorkspaceSession {
            team_id: "T1".into(),
            enterprise_id: None,
            user_id: source.owner().self_user_id.clone(),
            name: "Fixture".into(),
            url: "https://example.test".into(),
            token: "synthetic".into(),
        };
        let request = prepare(
            &mut shell,
            &captured,
            &SlackClient::default(),
            &session,
            "synthetic",
        )
        .unwrap();
        let RequestBody::Form(fields) = request.body else {
            panic!()
        };
        let field = |key: &str| -> Value {
            serde_json::from_str(&fields.iter().find(|(k, _)| k == key).unwrap().1).unwrap()
        };
        let actions = field("actions");
        let state = field("state");
        let expected = touched.then(|| json!([]));
        assert_eq!(actions[0].get("selected_options"), expected.as_ref());
        assert_eq!(
            state["values"]["choice"]["pick"].get("selected_options"),
            expected.as_ref()
        );
        assert_eq!(
            mount.options_present(&ContainerKey::new(&source), &key()),
            Some(touched)
        );
    }
}

#[test]
fn saved_empty_and_initial_empty_are_present_but_missing_multi_is_not() {
    let mount = display::Mounted::new();
    let absent = field("multi_static_select", Value::Null);
    mount.sync_multi(owner(), key(), &absent, None);
    assert_eq!(mount.options_present(&owner(), &key()), Some(false));
    let saved = display::Mounted::new();
    saved.sync_multi(owner(), key(), &absent, Some(&[]));
    assert_eq!(saved.options_present(&owner(), &key()), Some(true));
    mount.sync_multi(
        owner(),
        key(),
        &field("multi_static_select", json!([])),
        None,
    );
    assert_eq!(mount.options_present(&owner(), &key()), Some(true));
    let checkbox = display::Mounted::new();
    checkbox.sync_multi(owner(), key(), &field("checkboxes", Value::Null), None);
    assert_eq!(checkbox.options_present(&owner(), &key()), Some(true));
}

#[test]
fn retained_removed_option_is_removable_but_cannot_be_readded_as_current() {
    let (mut shell, _, _, mount) = setup();
    let raw = &mut canonical(&mut shell).blocks[1]["accessory"];
    raw["options"] = json!([option("b")]);
    raw.as_object_mut().unwrap().remove("initial_options");
    let workspace = &shell.core.workspaces["T1"];
    let message = workspace.messages["C2"]
        .messages
        .iter()
        .find(|m| m.ts.as_deref() == Some(radio_fixture::MAIN_TS))
        .unwrap();
    let row = crate::message_vm::message_vm(workspace, message, &shell.media);
    let source = Source::capture(&shell, &row, "C2", Bag::Timeline).unwrap();
    let entry = candidate(&shell, &source).unwrap();
    shell
        .message_buttons
        .inputs
        .insert(ContainerKey::new(&source), entry);
    let metadata = source.multi_metadata(&shell, "choice", "pick").unwrap();
    assert_eq!(
        display_selected(&shell, &source, &metadata, &mount),
        vec![option("a")]
    );
    install_picker(&mut shell, &source, &metadata, &mount);
    let view = picker(&shell).unwrap();
    assert_eq!(view.selected_options, vec![option("a")]);
    assert!(view.options.iter().all(|o| o.value != "a"));
    toggle_local(&mut shell, view.revision, "a".into());
    assert!(picker(&shell).unwrap().selected_options.is_empty());
    toggle_local(&mut shell, view.revision, "a".into());
    assert!(
        picker(&shell).unwrap().selected_options.is_empty(),
        "removed option must not be reintroduced from stale raw metadata"
    );
    cancel_local(&mut shell);
    assert_eq!(
        display_selected(&shell, &source, &metadata, &mount),
        vec![option("a")]
    );
    assert!(
        shell.message_buttons.inputs[&ContainerKey::new(&source)]
            .model
            .values()
            .unwrap()
            .is_none()
    );
}
