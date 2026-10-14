use super::*;
use crate::message_buttons::{select_fixture, source::Bag};
use crate::state::ChannelOpen;
use serde_json::{Value, json};
use super_platinum_core::{
    config::WorkspaceSession,
    slack::{PreparedRequest, SlackClient, client::RequestBody, models::Message},
};

fn shell() -> ShellState {
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    select_fixture::install(&mut shell);
    shell
}
fn source(shell: &ShellState) -> Source {
    let workspace = &shell.core.workspaces[shell.core.active_team.as_ref().unwrap()];
    let message = workspace.messages["C2"]
        .messages
        .iter()
        .find(|message| message.ts.as_deref() == Some(select_fixture::MAIN_TS))
        .unwrap();
    let row = crate::message_vm::message_vm(workspace, message, &shell.media);
    Source::capture(shell, &row, "C2", Bag::Timeline).unwrap()
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
        .find(|message| message.ts.as_deref() == Some(select_fixture::MAIN_TS))
        .unwrap()
}
fn workspace(source: &Source) -> WorkspaceSession {
    let owner = source.owner();
    WorkspaceSession {
        team_id: owner.team_id.clone(),
        enterprise_id: None,
        user_id: owner.self_user_id.clone(),
        name: "Fixture".into(),
        url: "https://example.test".into(),
        token: "synthetic".into(),
    }
}
fn values(request: PreparedRequest) -> Value {
    assert!(!request.retry_safe());
    let RequestBody::Form(fields) = request.body else {
        panic!("form request")
    };
    let state = fields.iter().find(|(key, _)| key == "state").unwrap();
    serde_json::from_str::<Value>(&state.1).unwrap()["values"].clone()
}
fn submit(shell: &mut ShellState, source: &Source) -> Value {
    let button = source.capture_button(shell, "review", "submit").unwrap();
    values(
        prepare(
            shell,
            &button,
            &SlackClient::default(),
            &workspace(source),
            "synthetic-submit",
        )
        .unwrap(),
    )
}
fn select(
    shell: &mut ShellState,
    source: &Source,
    block: &str,
    action: &str,
    value: Option<&str>,
) -> Value {
    let select = source.capture_select(shell, block, action, value).unwrap();
    values(
        prepare(
            shell,
            &select,
            &SlackClient::default(),
            &workspace(source),
            "synthetic-select",
        )
        .unwrap(),
    )
}

#[test]
fn first_button_initializes_complete_message_state_without_a_prior_select() {
    let mut shell = shell();
    let source = source(&shell);
    assert!(shell.message_buttons.inputs.is_empty());
    let values = submit(&mut shell, &source);
    assert_eq!(values["choice"]["pick"]["selected_option"]["value"], "a");
    assert_eq!(
        values["review"]["environment"]["selected_option"]["value"],
        "prod"
    );
    assert_eq!(shell.message_buttons.inputs.len(), 1);
    assert_eq!(
        shell.message_buttons.inputs[&ContainerKey::new(&source)]
            .model
            .values()
            .unwrap(),
        Some(values)
    );
}

#[test]
fn stateless_button_can_submit_at_stateful_container_capacity() {
    let mut shell = shell();
    let stateful = source(&shell);
    let entry = candidate(&shell, &stateful).unwrap();
    for index in 0..MAX_CONTAINERS {
        let mut key = ContainerKey::new(&stateful);
        key.ts = format!("capacity-{index}");
        shell.message_buttons.inputs.insert(key, entry.clone());
    }
    assert!(candidate(&shell, &stateful).is_err());

    canonical(&mut shell).blocks = vec![json!({
        "type":"actions", "block_id":"review", "elements":[{
            "type":"button", "action_id":"submit", "value":"submit-choices",
            "text":{"type":"plain_text", "text":"Submit choices"}
        }]
    })];
    let stateless = source(&shell);
    assert_eq!(submit(&mut shell, &stateless), json!({}));
    assert_eq!(shell.message_buttons.inputs.len(), MAX_CONTAINERS);
    assert!(
        !shell
            .message_buttons
            .inputs
            .contains_key(&ContainerKey::new(&stateless))
    );
}

fn overflow_only() -> Vec<Value> {
    vec![json!({"type":"actions","block_id":"review","elements":[{
        "type":"overflow","action_id":"more","options":[{
            "text":{"type":"plain_text","text":"Save review"},"value":"save"
        }]
    }]})]
}

#[test]
fn stateless_overflow_can_submit_at_full_draft_capacity_without_retention() {
    let mut shell = shell();
    let stateful = source(&shell);
    let entry = candidate(&shell, &stateful).unwrap();
    for index in 0..MAX_CONTAINERS {
        let mut key = ContainerKey::new(&stateful);
        key.ts = format!("capacity-{index}");
        shell.message_buttons.inputs.insert(key, entry.clone());
    }
    canonical(&mut shell).blocks = overflow_only();
    let stateless = source(&shell);
    // Exercise the same publication path as ensure(). An empty model must
    // neither consume a slot nor evict any existing draft.
    let candidate = candidate(&shell, &stateless).unwrap();
    retain_entry(&mut shell, &stateless, candidate);
    assert!(crate::message_buttons::overflow::stateless(
        &shell, &stateless
    ));
    let metadata = stateless
        .overflow_metadata(&shell, "review", "more")
        .unwrap();
    assert!(
        crate::message_buttons::overflow::presentation(&shell, &stateless, &metadata, true).enabled
    );
    let action = stateless
        .capture_overflow(&shell, "review", "more", "save")
        .unwrap();
    let request = prepare(
        &mut shell,
        &action,
        &SlackClient::default(),
        &workspace(&stateless),
        "synthetic-overflow",
    )
    .unwrap();
    assert_eq!(values(request), json!({}));
    assert_eq!(shell.message_buttons.inputs.len(), MAX_CONTAINERS);
    assert!(
        shell
            .message_buttons
            .inputs
            .values()
            .all(|retained| retained.model == entry.model)
    );
    assert!(revision(&shell, &stateless).is_none());
}

#[test]
fn repeated_stateless_ensure_does_not_grow_or_evict_drafts() {
    let mut shell = shell();
    let stateful = source(&shell);
    let entry = candidate(&shell, &stateful).unwrap();
    retain_entry(&mut shell, &stateful, entry.clone());
    let mut template = canonical(&mut shell).clone();
    template.blocks = overflow_only();
    for index in 0..MAX_CONTAINERS * 3 {
        let mut message = template.clone();
        message.ts = Some(format!("1819930000.{index:06}"));
        shell
            .core
            .workspaces
            .get_mut("T1")
            .unwrap()
            .messages
            .get_mut("C2")
            .unwrap()
            .upsert(message.clone());
        let row =
            crate::message_vm::message_vm(&shell.core.workspaces["T1"], &message, &shell.media);
        let source = Source::capture(&shell, &row, "C2", Bag::Timeline).unwrap();
        let candidate = candidate(&shell, &source).unwrap();
        retain_entry(&mut shell, &source, candidate);
        assert_eq!(shell.message_buttons.inputs.len(), 1);
        assert!(revision(&shell, &source).is_none());
    }
    assert_eq!(
        shell.message_buttons.inputs[&ContainerKey::new(&stateful)].model,
        entry.model
    );
}

#[test]
fn same_canonical_message_in_another_bag_preserves_input_revision_and_confirmation() {
    let mut shell = shell();
    let timeline = source(&shell);
    submit(&mut shell, &timeline);
    let before = shell.message_buttons.inputs[&ContainerKey::new(&timeline)].clone();
    let pending = Pending {
        captured: timeline
            .capture_select(&shell, "choice", "pick", Some("b"))
            .unwrap(),
        revision: next_id(),
        input_revision: Some(before.revision),
    };
    let message = canonical(&mut shell).clone();
    shell.thread_root = Some(select_fixture::MAIN_TS.into());
    shell
        .core
        .threads
        .entry(("T1".into(), "C2".into(), select_fixture::MAIN_TS.into()))
        .or_default()
        .upsert(message.clone());
    let row = crate::message_vm::message_vm(&shell.core.workspaces["T1"], &message, &shell.media);
    let thread = Source::capture(
        &shell,
        &row,
        "C2",
        Bag::Thread {
            root: select_fixture::MAIN_TS.into(),
        },
    )
    .unwrap();
    assert!(thread != timeline);
    assert!(pending_current(&shell, &pending));

    let refreshed = candidate(&shell, &thread).unwrap();
    assert_eq!(refreshed.model, before.model);
    assert_eq!(refreshed.revision, before.revision);
    assert!(refreshed.source == thread);
    shell
        .message_buttons
        .inputs
        .insert(ContainerKey::new(&thread), refreshed);
    assert!(pending_current(&shell, &pending));
    assert_eq!(
        candidate(&shell, &timeline).unwrap().revision,
        before.revision
    );
}

#[test]
fn selected_values_are_retained_for_later_button_and_explicit_clear() {
    let mut shell = shell();
    let source = source(&shell);
    let first = select(&mut shell, &source, "choice", "pick", Some("b"));
    assert_eq!(first["choice"]["pick"]["selected_option"]["value"], "b");
    assert_eq!(
        first["review"]["environment"]["selected_option"]["value"],
        "prod"
    );
    let selected = select(&mut shell, &source, "review", "environment", Some("test"));
    assert_eq!(submit(&mut shell, &source), selected);
    let cleared = select(&mut shell, &source, "review", "environment", None);
    assert_eq!(
        cleared["review"]["environment"]["selected_option"],
        Value::Null
    );
    assert_eq!(submit(&mut shell, &source), cleared);
}

#[test]
fn option_removal_resets_display_but_later_button_retains_committed_raw_option() {
    let mut shell = shell();
    let original = source(&shell);
    let committed = select(&mut shell, &original, "choice", "pick", Some("b"));
    let select = &mut canonical(&mut shell).blocks[1]["accessory"];
    select["option_groups"].as_array_mut().unwrap().pop(); // Remove selected B; initial A remains valid.
    let current = source(&shell);
    let values = submit(&mut shell, &current);
    assert_eq!(values["choice"]["pick"], committed["choice"]["pick"]);
    let entry = &shell.message_buttons.inputs[&ContainerKey::new(&current)];
    assert_eq!(
        entry.model.fields[&InputKey {
            block_id: "choice".into(),
            action_id: "pick".into()
        }]
            .value,
        InputValue::Selected(vec![])
    );
    assert!(
        current
            .capture_select(&shell, "choice", "pick", Some("b"))
            .is_err()
    );
    assert_eq!(
        submit(&mut shell, &current)["choice"]["pick"],
        committed["choice"]["pick"]
    );
}

#[test]
fn navigation_uses_fresh_source_and_preserves_owned_container_state() {
    let mut shell = shell();
    let original = source(&shell);
    let selected = select(&mut shell, &original, "choice", "pick", Some("b"));
    let away = shell
        .channels
        .iter()
        .position(|channel| channel.id == "C1")
        .unwrap();
    shell.select_channel(away, ChannelOpen::Global);
    assert!(original.message(&shell).is_err());
    let back = shell
        .channels
        .iter()
        .position(|channel| channel.id == "C2")
        .unwrap();
    shell.select_channel(back, ChannelOpen::Global);
    let current = source(&shell);
    assert!(current != original);
    assert_eq!(submit(&mut shell, &current), selected);
    assert_eq!(shell.message_buttons.inputs.len(), 1);
}

#[test]
fn account_and_viewer_changes_cannot_inherit_prior_selections() {
    for replace_viewer in [false, true] {
        let mut shell = shell();
        let old = source(&shell);
        select(&mut shell, &old, "choice", "pick", Some("b"));
        if replace_viewer {
            shell.core.workspaces.get_mut("T1").unwrap().self_user_id = "U_OTHER".into();
        } else {
            shell.core.account_epoch += 1;
        }
        prune(&mut shell);
        assert!(shell.message_buttons.inputs.is_empty());
        assert!(old.message(&shell).is_err());
        let current = source(&shell);
        assert_eq!(
            submit(&mut shell, &current)["choice"]["pick"]["selected_option"]["value"],
            "a"
        );
    }
}

#[test]
fn equal_message_coordinates_in_another_workspace_do_not_share_selections() {
    let mut shell = shell();
    let first = source(&shell);
    select(&mut shell, &first, "choice", "pick", Some("b"));
    let mut second = shell.core.workspaces["T1"].clone();
    second.team_id = "T2".into();
    shell.core.workspaces.insert("T2".into(), second);
    let mut row = shell
        .workspaces
        .iter()
        .find(|workspace| workspace.id == "T1")
        .unwrap()
        .clone();
    row.id = "T2".into();
    shell.workspaces.push(row);
    let index = shell
        .workspaces
        .iter()
        .position(|workspace| workspace.id == "T2")
        .unwrap();
    assert!(shell.select_workspace(index));
    let index = shell
        .channels
        .iter()
        .position(|channel| channel.id == "C2")
        .unwrap();
    shell.select_channel(index, ChannelOpen::Global);
    let second = source(&shell);
    assert_eq!(
        submit(&mut shell, &second)["choice"]["pick"]["selected_option"]["value"],
        "a"
    );
    assert_eq!(shell.message_buttons.inputs.len(), 2);
    assert_eq!(
        shell.message_buttons.inputs[&ContainerKey::new(&first)]
            .model
            .values()
            .unwrap()
            .unwrap()["choice"]["pick"]["selected_option"]["value"],
        "b"
    );
}

#[test]
fn pending_confirmation_retires_when_local_selection_revision_changes() {
    let mut shell = shell();
    let source = source(&shell);
    let entry = candidate(&shell, &source).unwrap();
    shell
        .message_buttons
        .inputs
        .insert(ContainerKey::new(&source), entry);
    let captured = source
        .capture_select(&shell, "choice", "pick", Some("b"))
        .unwrap();
    let pending = Pending {
        captured,
        revision: next_id(),
        input_revision: revision(&shell, &source),
    };
    assert!(pending_current(&shell, &pending));
    let before = shell.message_buttons.inputs[&ContainerKey::new(&source)]
        .model
        .clone();
    // Merely preparing the confirmation candidate commits no state.
    assert_eq!(before.values().unwrap(), None);
    select(&mut shell, &source, "review", "environment", Some("test"));
    assert!(pending.captured.revalidate(&shell).is_ok()); // Canonical source is unchanged.
    assert!(!pending_current(&shell, &pending));
}

#[test]
fn failed_request_preparation_is_transactional_for_local_inputs() {
    let mut shell = shell();
    let source = source(&shell);
    submit(&mut shell, &source);
    let before = shell.message_buttons.inputs[&ContainerKey::new(&source)]
        .model
        .clone();
    let captured = source
        .capture_select(&shell, "choice", "pick", Some("b"))
        .unwrap();
    let mut wrong = workspace(&source);
    wrong.user_id = "U_WRONG".into();
    assert!(
        prepare(
            &mut shell,
            &captured,
            &SlackClient::default(),
            &wrong,
            "synthetic"
        )
        .is_err()
    );
    assert_eq!(
        shell.message_buttons.inputs[&ContainerKey::new(&source)].model,
        before
    );
}

#[test]
fn unsupported_remote_sibling_does_not_submit_retained_state_against_old_model() {
    let mut shell = shell();
    let old = source(&shell);
    select(&mut shell, &old, "choice", "pick", Some("b"));
    let before = shell.message_buttons.inputs[&ContainerKey::new(&old)]
        .model
        .clone();
    canonical(&mut shell).blocks.push(json!({"type":"actions","block_id":"future","elements":[{"type":"multi_static_select","action_id":"other"}]}));
    let current = source(&shell);
    assert!(candidate(&shell, &current).is_err());
    assert!(current.capture_button(&shell, "review", "submit").is_err());
    assert_eq!(
        shell.message_buttons.inputs[&ContainerKey::new(&old)].model,
        before
    );
}
