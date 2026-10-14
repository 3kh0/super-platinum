use super::*;

fn shell() -> (ShellState, String, String) {
    let shell = ShellState::fixture(crate::media::MediaRegistry::default());
    let team = shell.core.active_team.clone().unwrap();
    let channel = shell.core.active_channel.clone().unwrap();
    (shell, team, channel)
}

fn cleared(channel: &str) -> CountsPage {
    serde_json::from_value(json!({
        "channels":[{"id":channel,"has_unreads":false,"last_read":"900.000000"}],
        "activity_v2":{"at_user":0}
    }))
    .unwrap()
}

#[test]
fn unchanged_readback_accepts_server_cursor_including_undo_regression() {
    let (mut shell, team, channel) = shell();
    let baseline = Snapshot::capture(&shell, &team);
    assert!(apply_counts(
        &mut shell,
        &team,
        &baseline,
        cleared(&channel)
    ));
    assert_eq!(
        shell.core.workspaces[&team].read_cursor(&channel),
        Some("900.000000")
    );
    let baseline = Snapshot::capture(&shell, &team);
    let unread = serde_json::from_value(json!({"channels":[{
        "id":channel,"has_unreads":true,"last_read":"100.000000","unread_count":3
    }]}))
    .unwrap();
    assert!(apply_counts(&mut shell, &team, &baseline, unread));
    assert_eq!(
        shell.core.workspaces[&team].read_cursor(&channel),
        Some("100.000000")
    );
    assert_eq!(
        shell.core.workspaces[&team].messages[&channel].unread_count,
        3
    );
}

#[test]
fn cursor_aba_before_readback_cannot_erase_explicit_unread() {
    let (mut shell, team, channel) = shell();
    let baseline = Snapshot::capture(&shell, &team);
    let bag = shell
        .core
        .workspaces
        .get_mut(&team)
        .unwrap()
        .messages
        .entry(channel.clone())
        .or_default();
    // Exact cursor value returned to its baseline, but two ordered marks ran.
    bag.read_revision = bag.read_revision.wrapping_add(2);
    let before = bag_state(bag);
    assert!(!apply_counts(
        &mut shell,
        &team,
        &baseline,
        cleared(&channel)
    ));
    assert_eq!(
        bag_state(&shell.core.workspaces[&team].messages[&channel]),
        before
    );
}

#[test]
fn new_message_during_bulk_write_keeps_new_unread() {
    let (mut shell, team, channel) = shell();
    let baseline = Snapshot::capture(&shell, &team);
    let bag = shell
        .core
        .workspaces
        .get_mut(&team)
        .unwrap()
        .messages
        .entry(channel.clone())
        .or_default();
    bag.upsert(serde_json::from_value(json!({"ts":"901.000000","text":"new arrival"})).unwrap());
    bag.unread_count += 1;
    assert!(!apply_counts(
        &mut shell,
        &team,
        &baseline,
        cleared(&channel)
    ));
    assert!(
        shell.core.workspaces[&team].messages[&channel]
            .messages
            .iter()
            .any(|m| m.ts.as_deref() == Some("901.000000"))
    );
}

#[test]
fn activity_or_thread_cursor_change_blocks_older_zero_counts() {
    let (mut shell, team, channel) = shell();
    let baseline = Snapshot::capture(&shell, &team);
    shell
        .core
        .activity
        .ensure_scope(shell.core.account_epoch, &team);
    shell.core.activity.upsert(
        serde_json::from_value(json!({
            "key":"new","feed_ts":"901.000000","is_unread":true,
            "item":{"type":"at_user","message":{"channel":channel,"ts":"901.000000"}}
        }))
        .unwrap(),
    );
    assert!(!apply_counts(
        &mut shell,
        &team,
        &baseline,
        cleared(&channel)
    ));
    assert!(
        shell
            .core
            .activity
            .items
            .iter()
            .find(|item| item.key == "new")
            .unwrap()
            .is_unread
    );
    let baseline = Snapshot::capture(&shell, &team);
    shell.core.apply_thread_mark(
        &(team.clone(), channel.clone(), "800.000000".into()),
        "801.000000",
    );
    assert!(!apply_counts(
        &mut shell,
        &team,
        &baseline,
        cleared(&channel)
    ));
}

#[test]
fn same_count_message_edit_is_also_a_stale_history_barrier() {
    let (mut shell, team, channel) = shell();
    let bag = shell
        .core
        .workspaces
        .get_mut(&team)
        .unwrap()
        .messages
        .entry(channel.clone())
        .or_default();
    bag.upsert(serde_json::from_value(json!({"ts":"901.000000","text":"original"})).unwrap());
    let baseline = Snapshot::capture(&shell, &team);
    shell
        .core
        .workspaces
        .get_mut(&team)
        .unwrap()
        .messages
        .get_mut(&channel)
        .unwrap()
        .upsert(serde_json::from_value(json!({"ts":"901.000000","text":"edited"})).unwrap());
    assert!(!apply_counts(
        &mut shell,
        &team,
        &baseline,
        cleared(&channel)
    ));
}

#[test]
fn unrelated_cached_edit_is_allowed_and_is_never_overwritten_by_counts() {
    let (mut shell, team, channel) = shell();
    let bag = shell
        .core
        .workspaces
        .get_mut(&team)
        .unwrap()
        .messages
        .entry("other".into())
        .or_default();
    bag.upsert(serde_json::from_value(json!({"ts":"901.000000","text":"original"})).unwrap());
    let baseline = Snapshot::capture(&shell, &team);
    shell
        .core
        .workspaces
        .get_mut(&team)
        .unwrap()
        .messages
        .get_mut("other")
        .unwrap()
        .upsert(serde_json::from_value(json!({"ts":"901.000000","text":"edited"})).unwrap());
    assert!(apply_counts(
        &mut shell,
        &team,
        &baseline,
        cleared(&channel)
    ));
    assert_eq!(
        shell.core.workspaces[&team].messages["other"].messages[0]
            .text
            .as_deref(),
        Some("edited")
    );
}

#[test]
fn bounded_history_keeps_tombstones_cursors_and_paging() {
    let mut bag = ChannelMessages::default();
    bag.last_read = Some("100.000000".into());
    bag.read_revision = 7;
    bag.has_more_older = true;
    bag.record_remote_deletion("102.000000");
    let page = serde_json::from_value(json!({"messages":[
        {"ts":"101.000000","text":"still here"},
        {"ts":"102.000000","text":"stale deleted message"}
    ],"has_more":false}))
    .unwrap();
    merge_history(&mut bag, page, None);
    assert_eq!(bag.messages.len(), 1);
    assert_eq!(bag.messages[0].ts.as_deref(), Some("101.000000"));
    assert_eq!(bag.last_read.as_deref(), Some("100.000000"));
    assert_eq!(bag.read_revision, 7);
    assert!(bag.has_more_older);
}
