use super::*;
use serde_json::json;
use super_platinum_core::block_actions::form::{parse_message_radio, parse_message_static_select};

fn owner() -> ContainerKey {
    ContainerKey {
        epoch: 1,
        user: "U1".into(),
        team: "T1".into(),
        channel: "C1".into(),
        ts: "1.000001".into(),
    }
}
fn key() -> InputKey {
    InputKey {
        block_id: "choice".into(),
        action_id: "pick".into(),
    }
}
fn option(value: &str) -> Value {
    json!({"value":value,"text":{"type":"plain_text","text":format!("Choice {value}")}})
}
fn field(radio: bool, initial: &str, values: &[&str]) -> FormField {
    let raw = json!({"type":if radio {"radio_buttons"} else {"static_select"},
        "action_id":"pick","options":values.iter().map(|value|option(value)).collect::<Vec<_>>(),"initial_option":option(initial)});
    if radio {
        parse_message_radio(&raw).unwrap()
    } else {
        parse_message_static_select(&raw).unwrap()
    }
}
fn sync(mounted: &Mounted, field: &FormField, saved: Option<&Value>) -> Option<String> {
    mounted.sync(owner(), key(), field, saved)
}

#[test]
fn simultaneous_copies_keep_independent_display_but_new_mount_reads_shared_wire() {
    for radio in [false, true] {
        let field = field(radio, "a", &["a", "b"]);
        let first = Mounted::new();
        let second = Mounted::new();
        assert_eq!(sync(&first, &field, None).as_deref(), Some("a"));
        assert_eq!(sync(&second, &field, None).as_deref(), Some("a"));
        assert!(first.capture(&owner(), &key()).unwrap().commit(Some("b")));
        let saved = option("b");
        assert_eq!(sync(&first, &field, Some(&saved)).as_deref(), Some("b"));
        assert_eq!(sync(&second, &field, Some(&saved)).as_deref(), Some("a"));
        assert_eq!(
            sync(&Mounted::new(), &field, Some(&saved)).as_deref(),
            Some("b")
        );
    }
}

#[test]
fn radio_empty_choice_is_local_but_remount_uses_initial_fallback_unlike_select() {
    for radio in [false, true] {
        let field = field(radio, "a", &["", "a"]);
        let mounted = Mounted::new();
        sync(&mounted, &field, None);
        assert!(mounted.capture(&owner(), &key()).unwrap().commit(Some("")));
        let empty = option("");
        assert_eq!(sync(&mounted, &field, Some(&empty)).as_deref(), Some(""));
        assert_eq!(
            sync(&Mounted::new(), &field, Some(&empty)).as_deref(),
            Some(if radio { "a" } else { "" })
        );
    }
}

#[test]
fn clearing_a_mount_does_not_reset_it_but_null_shared_state_seeds_initial_on_remount() {
    for radio in [false, true] {
        let field = field(radio, "a", &["a", "b"]);
        let mounted = Mounted::new();
        sync(&mounted, &field, None);
        assert!(mounted.capture(&owner(), &key()).unwrap().commit(None));
        assert_eq!(sync(&mounted, &field, None), None);
        assert_eq!(sync(&Mounted::new(), &field, None).as_deref(), Some("a"));
    }
}

#[test]
fn mounted_radio_ignores_new_initial_while_remount_uses_current_initial_or_saved_option() {
    let mounted = Mounted::new();
    sync(&mounted, &field(true, "a", &["a", "b"]), None);
    let changed = field(true, "b", &["a", "b"]);
    assert_eq!(sync(&mounted, &changed, None).as_deref(), Some("a"));
    assert_eq!(sync(&Mounted::new(), &changed, None).as_deref(), Some("b"));
    assert_eq!(
        sync(&Mounted::new(), &changed, Some(&option("a"))).as_deref(),
        Some("a")
    );
}

#[test]
fn mounted_select_resets_initial_and_disappeared_option_without_rewriting_saved_wire() {
    let mounted = Mounted::new();
    sync(&mounted, &field(false, "a", &["a", "b"]), None);
    assert!(mounted.capture(&owner(), &key()).unwrap().commit(Some("b")));
    let saved = option("b");
    let removed = field(false, "a", &["a"]);
    assert_eq!(sync(&mounted, &removed, Some(&saved)), None);
    // Constructor accepts retained state even when the option disappeared.
    assert_eq!(
        sync(&Mounted::new(), &removed, Some(&saved)).as_deref(),
        Some("b")
    );
    let initial_changed = field(false, "c", &["a", "c"]);
    assert_eq!(
        sync(&mounted, &initial_changed, Some(&saved)).as_deref(),
        Some("c")
    );
    assert_eq!(saved, option("b"));
}

#[test]
fn mounted_radio_retains_missing_option_and_type_replacement_constructs_from_shared_state() {
    let mounted = Mounted::new();
    sync(&mounted, &field(true, "a", &["a", "b"]), None);
    mounted.capture(&owner(), &key()).unwrap().commit(Some("b"));
    assert_eq!(
        sync(&mounted, &field(true, "a", &["a"]), Some(&option("b"))).as_deref(),
        Some("b")
    );
    // Reuse of a keyed host cannot carry radio's empty-value display into a
    // newly mounted select constructor (or vice versa).
    assert_eq!(
        sync(&mounted, &field(false, "a", &["", "a"]), Some(&option(""))).as_deref(),
        Some("")
    );
    assert_eq!(
        sync(&mounted, &field(true, "a", &["", "a"]), Some(&option(""))).as_deref(),
        Some("a")
    );
}

#[test]
fn cancel_does_not_change_display_and_unmount_or_rebind_retires_confirmation() {
    let field = field(false, "a", &["a", "b"]);
    let mounted = Mounted::new();
    sync(&mounted, &field, None);
    let canceled = mounted.capture(&owner(), &key()).unwrap();
    assert!(canceled.current());
    drop(canceled);
    assert_eq!(sync(&mounted, &field, None).as_deref(), Some("a"));
    let stale = mounted.capture(&owner(), &key()).unwrap();
    let mut other = owner();
    other.epoch += 1;
    mounted.sync(other, key(), &field, None);
    assert!(!stale.current());
    assert!(!stale.commit(Some("b")));
    sync(&mounted, &field, None);
    let pending = mounted.capture(&owner(), &key()).unwrap();
    mounted.clone().unmount();
    assert!(!pending.current());
    assert!(!pending.commit(Some("b")));
    assert!(mounted.capture(&owner(), &key()).is_none());
}

#[test]
fn pure_radio_request_publishes_only_origin_display_and_unmounted_confirmation_cannot_prepare() {
    use crate::message_buttons::{
        inputs, radio_fixture,
        source::{Bag, Source},
    };
    use crate::state::ShellState;
    use super_platinum_core::{config::WorkspaceSession, slack::SlackClient};
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    radio_fixture::install(&mut shell);
    let workspace = &shell.core.workspaces["T1"];
    let message = workspace.messages["C2"]
        .messages
        .iter()
        .find(|m| m.ts.as_deref() == Some(radio_fixture::MAIN_TS))
        .unwrap();
    let row = crate::message_vm::message_vm(workspace, message, &shell.media);
    let source = Source::capture(&shell, &row, "C2", Bag::Timeline).unwrap();
    let entry = inputs::candidate(&shell, &source).unwrap();
    shell
        .message_buttons
        .inputs
        .insert(ContainerKey::new(&source), entry);
    let mut captured = source
        .capture_radio(&shell, "choice", "pick", Some("b"))
        .unwrap();
    let first = Mounted::new();
    let second = Mounted::new();
    for mount in [&first, &second] {
        assert_eq!(
            inputs::display_selected(&shell, &source, captured.metadata(), mount).as_deref(),
            Some("a")
        );
    }
    let key = InputKey {
        block_id: "choice".into(),
        action_id: "pick".into(),
    };
    captured.set_display(first.capture(&ContainerKey::new(&source), &key).unwrap());
    let session = WorkspaceSession {
        team_id: "T1".into(),
        enterprise_id: None,
        user_id: source.owner().self_user_id.clone(),
        name: "Fixture".into(),
        url: "https://example.test".into(),
        token: "synthetic".into(),
    };
    let mut wrong = session.clone();
    wrong.user_id = "U_OTHER".into();
    assert!(
        inputs::prepare(
            &mut shell,
            &captured,
            &SlackClient::default(),
            &wrong,
            "synthetic"
        )
        .is_err()
    );
    assert_eq!(
        inputs::display_selected(&shell, &source, captured.metadata(), &first).as_deref(),
        Some("a")
    );
    inputs::prepare(
        &mut shell,
        &captured,
        &SlackClient::default(),
        &session,
        "synthetic",
    )
    .unwrap();
    assert_eq!(
        inputs::display_selected(&shell, &source, captured.metadata(), &first).as_deref(),
        Some("b")
    );
    assert_eq!(
        inputs::display_selected(&shell, &source, captured.metadata(), &second).as_deref(),
        Some("a")
    );
    assert_eq!(
        inputs::display_selected(&shell, &source, captured.metadata(), &Mounted::new()).as_deref(),
        Some("b")
    );
    assert!(
        captured.revalidate(&shell).is_ok(),
        "display commit must not retire transport operation"
    );
    let mut pending = source
        .capture_radio(&shell, "choice", "pick", Some("a"))
        .unwrap();
    pending.set_display(first.capture(&ContainerKey::new(&source), &key).unwrap());
    let confirmation = crate::message_buttons::Pending {
        captured: pending.clone(),
        revision: next_id(),
        input_revision: inputs::revision(&shell, &source),
    };
    assert!(inputs::pending_current(&shell, &confirmation));
    first.unmount();
    assert!(!inputs::pending_current(&shell, &confirmation));
    assert!(
        inputs::prepare(
            &mut shell,
            &pending,
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
            .unwrap()["choice"]["pick"]["selected_option"]["value"],
        "b"
    );

    // Explicit null remains on the wire but is absent from the saved-option
    // constructor prop. Do not reinterpret it as a persistent empty display.
    let clear = source
        .capture_radio(&shell, "choice", "pick", None)
        .unwrap();
    inputs::prepare(
        &mut shell,
        &clear,
        &SlackClient::default(),
        &session,
        "synthetic",
    )
    .unwrap();
    let model = &shell.message_buttons.inputs[&ContainerKey::new(&source)].model;
    assert!(model.saved_option(&key).is_none());
    assert_eq!(
        model.values().unwrap().unwrap()["choice"]["pick"]["selected_option"],
        Value::Null
    );
    assert_eq!(
        inputs::display_selected(&shell, &source, clear.metadata(), &Mounted::new()).as_deref(),
        Some("a")
    );
}

#[test]
fn date_mounts_are_independent_initial_updates_reset_and_unmount_retires_confirmation() {
    let raw = json!({"type":"datepicker","action_id":"pick","initial_date":"2026-10-13"});
    let field = super_platinum_core::block_actions::form::parse_message_date(&raw).unwrap();
    let first = Mounted::new();
    let second = Mounted::new();
    assert_eq!(sync(&first, &field, None).as_deref(), Some("2026-10-13"));
    sync(&second, &field, None);
    let action = first.capture(&owner(), &key()).unwrap();
    assert!(action.commit(Some("2026-10-14")));
    let saved = json!("2026-10-14");
    assert_eq!(
        sync(&first, &field, Some(&saved)).as_deref(),
        Some("2026-10-14")
    );
    assert_eq!(
        sync(&second, &field, Some(&saved)).as_deref(),
        Some("2026-10-13")
    );
    assert_eq!(
        sync(&Mounted::new(), &field, Some(&saved)).as_deref(),
        Some("2026-10-14")
    );
    let pending = first.capture(&owner(), &key()).unwrap();
    let mut newraw = raw;
    newraw["initial_date"] = json!("2026-11-01");
    let updated = super_platinum_core::block_actions::form::parse_message_date(&newraw).unwrap();
    assert_eq!(
        sync(&first, &updated, Some(&saved)).as_deref(),
        Some("2026-11-01")
    );
    assert!(!pending.current());
    newraw.as_object_mut().unwrap().remove("initial_date");
    let removed = super_platinum_core::block_actions::form::parse_message_date(&newraw).unwrap();
    assert_eq!(sync(&first, &removed, Some(&saved)), None);
    let pending = first.capture(&owner(), &key()).unwrap();
    first.unmount();
    assert!(!pending.current());
    assert!(!pending.commit(Some("2026-12-01")));
}

#[test]
fn time_mounts_keep_wall_time_on_timezone_change_and_reset_only_changed_initial() {
    let raw = json!({"type":"timepicker","action_id":"pick","initial_time":"09:30","timezone":"America/New_York"});
    let field = super_platinum_core::block_actions::form::parse_message_time(&raw).unwrap();
    let first = Mounted::new();
    let second = Mounted::new();
    sync(&first, &field, None);
    sync(&second, &field, None);
    assert!(
        first
            .capture(&owner(), &key())
            .unwrap()
            .commit(Some("10:45"))
    );
    let saved = json!("10:45");
    assert_eq!(
        sync(&second, &field, Some(&saved)).as_deref(),
        Some("09:30")
    );
    assert_eq!(
        sync(&Mounted::new(), &field, Some(&saved)).as_deref(),
        Some("10:45")
    );
    let mut changed = raw;
    changed["timezone"] = json!("Asia/Tokyo");
    let updated = super_platinum_core::block_actions::form::parse_message_time(&changed).unwrap();
    assert_eq!(
        sync(&first, &updated, Some(&saved)).as_deref(),
        Some("10:45")
    );
    let pending = first.capture(&owner(), &key()).unwrap();
    changed["initial_time"] = json!("12:00");
    let updated = super_platinum_core::block_actions::form::parse_message_time(&changed).unwrap();
    assert_eq!(
        sync(&first, &updated, Some(&saved)).as_deref(),
        Some("12:00")
    );
    assert!(!pending.current());
    changed.as_object_mut().unwrap().remove("initial_time");
    let removed = super_platinum_core::block_actions::form::parse_message_time(&changed).unwrap();
    assert_eq!(sync(&first, &removed, Some(&saved)), None);
    let pending = first.capture(&owner(), &key()).unwrap();
    first.unmount();
    assert!(!pending.commit(Some("13:15")));
}

#[test]
fn user_mount_uses_saved_identity_and_resets_only_when_initial_prop_changes() {
    use super_platinum_core::block_actions::form::parse_message_user;
    let field =
        parse_message_user(&json!({"type":"users_select","initial_user":"U00000001"})).unwrap();
    let mounted = Mounted::new();
    let saved = json!("W00000002");
    assert_eq!(
        sync(&mounted, &field, Some(&saved)).as_deref(),
        Some("W00000002")
    );
    assert_eq!(
        sync(&mounted, &field, Some(&json!("U00000003"))).as_deref(),
        Some("W00000002")
    );
    let pending = mounted.capture(&owner(), &key()).unwrap();
    let changed =
        parse_message_user(&json!({"type":"users_select","initial_user":"U00000004"})).unwrap();
    assert_eq!(
        sync(&mounted, &changed, Some(&saved)).as_deref(),
        Some("U00000004")
    );
    assert!(!pending.current());
    assert_eq!(
        sync(&Mounted::new(), &changed, Some(&saved)).as_deref(),
        Some("W00000002")
    );
    let pending = mounted.capture(&owner(), &key()).unwrap();
    mounted.unmount();
    assert!(!pending.current());
}

#[test]
fn multi_conversation_removed_or_null_initial_retains_mount_but_present_empty_resets() {
    use super_platinum_core::block_actions::form::parse_message_conversations;
    let mut raw = json!({"type":"multi_conversations_select","action_id":"pick","initial_conversations":["C00000001"]});
    let mounted = Mounted::new();
    let field = parse_message_conversations(&raw).unwrap();
    assert_eq!(
        mounted.sync_users(owner(), key(), &field, None),
        vec!["C00000001"]
    );
    assert!(
        mounted
            .capture(&owner(), &key())
            .unwrap()
            .commit_users(&["U00000002".into()])
    );
    raw.as_object_mut().unwrap().remove("initial_conversations");
    assert_eq!(
        mounted.sync_users(
            owner(),
            key(),
            &parse_message_conversations(&raw).unwrap(),
            None
        ),
        vec!["U00000002"]
    );
    raw["initial_conversations"] = Value::Null;
    assert_eq!(
        mounted.sync_users(
            owner(),
            key(),
            &parse_message_conversations(&raw).unwrap(),
            None
        ),
        vec!["U00000002"]
    );
    raw["initial_conversations"] = json!([]);
    assert!(
        mounted
            .sync_users(
                owner(),
                key(),
                &parse_message_conversations(&raw).unwrap(),
                None
            )
            .is_empty()
    );
    assert_eq!(
        Mounted::new().sync_users(
            owner(),
            key(),
            &parse_message_conversations(&raw).unwrap(),
            Some(vec!["U00000002".into()])
        ),
        vec!["U00000002"]
    );
}
