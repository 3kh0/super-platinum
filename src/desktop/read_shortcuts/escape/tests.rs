use super::*;
use crate::media::MediaRegistry;
use super_platinum_core::slack::models::Message;

fn shell() -> ShellState {
    let mut shell = ShellState::fixture(MediaRegistry::default());
    shell.core.active_account = Some("isolated-shortcut-fixture".into());
    shell.main_view = MainView::Home;
    let team = shell.core.active_team.clone().unwrap();
    let channel = shell.core.active_channel.clone().unwrap();
    let workspace = shell.core.workspaces.get_mut(&team).unwrap();
    let meta = workspace.channels.get_mut(&channel).unwrap();
    meta.extra.insert("is_member".into(), true.into());
    meta.unread_count = Some(3);
    meta.last_read = Some("10.000001".into());
    meta.has_unreads = true;
    let bag = workspace.messages.get_mut(&channel).unwrap();
    bag.last_read = Some("10.000001".into());
    bag.latest = Some("20.000001".into());
    bag.unread_count = 3;
    bag.messages = vec![Message {
        ts: Some("15.000001".into()),
        ..Default::default()
    }];
    shell.unread_anchor = None;
    shell
}

fn channel_decision(shell: &ShellState) -> Option<Decision> {
    decision(shell, &Target::Primary(shell.timeline_scope()))
}

#[test]
fn cold_metadata_marks_latest_without_loaded_history_or_automatic_read_mode() {
    let mut shell = shell();
    let team = shell.core.active_team.clone().unwrap();
    let channel = shell.core.active_channel.clone().unwrap();
    let workspace = shell.core.workspaces.get_mut(&team).unwrap();
    workspace.read_preferences = Default::default();
    workspace
        .messages
        .get_mut(&channel)
        .unwrap()
        .messages
        .clear();
    workspace.messages.get_mut(&channel).unwrap().loaded = false;
    assert!(matches!(channel_decision(&shell),Some(Decision::Channel{ts,..}) if ts=="20.000001"));
    shell
        .core
        .workspaces
        .get_mut(&team)
        .unwrap()
        .messages
        .remove(&channel);
    shell
        .core
        .workspaces
        .get_mut(&team)
        .unwrap()
        .channels
        .get_mut(&channel)
        .unwrap()
        .extra
        .insert("latest".into(), serde_json::json!({"ts":"30.000001"}));
    assert!(matches!(channel_decision(&shell),Some(Decision::Channel{ts,..}) if ts=="30.000001"));
}

#[test]
fn ghost_is_consumed_without_rewinding_or_fabricating_a_cursor() {
    let mut shell = shell();
    let team = shell.core.active_team.clone().unwrap();
    let channel = shell.core.active_channel.clone().unwrap();
    let workspace = shell.core.workspaces.get_mut(&team).unwrap();
    workspace.channels.get_mut(&channel).unwrap().unread_count = Some(0);
    workspace.channels.get_mut(&channel).unwrap().has_unreads = false;
    let bag = workspace.messages.get_mut(&channel).unwrap();
    bag.unread_count = 0;
    bag.last_read = Some("30.000001".into());
    shell.unread_anchor = Some((channel, "10.000001".into()));
    assert!(
        matches!(channel_decision(&shell),Some(Decision::ClearChannel{cursor,repair:false,..}) if cursor=="30.000001")
    );
    shell.unread_anchor = None;
    assert_eq!(channel_decision(&shell), None);
}

#[test]
fn pending_target_and_manual_unread_ordering_block_escape_admission() {
    let mut shell = shell();
    let target = ReadTarget::Conversation {
        team: shell.core.active_team.clone().unwrap(),
        channel: shell.core.active_channel.clone().unwrap(),
    };
    shell
        .core
        .pending_marks
        .insert((target.clone(), "15.000001".into()));
    assert_eq!(channel_decision(&shell), None);
    shell.core.pending_marks.clear();
    shell
        .read_shortcuts
        .pending
        .insert((shell.core.account_epoch, target), 1);
    assert_eq!(channel_decision(&shell), None);
}

#[test]
fn subscribed_thread_reads_real_tail_then_clears_divider_before_close() {
    let mut shell = shell();
    crate::thread_unread::fixture(&mut shell);
    shell.thread_loading = false;
    let key = crate::thread_unread::key(&shell).unwrap();
    let bag = shell.core.threads.get_mut(&key).unwrap();
    bag.messages[0]
        .extra
        .insert("subscribed".into(), true.into());
    bag.messages.push(Message {
        ts: Some("9999999999.000001".into()),
        extra: [("is_ephemeral".into(), true.into())].into(),
        ..Default::default()
    });
    bag.messages.push(Message {
        ts: Some("9999999999.000002".into()),
        ..Default::default()
    });
    bag.pending.push("9999999999.000002".into());
    shell.core.thread_composer.text = "keep this thread draft".into();
    let target = Target::Thread(shell.thread_scope());
    let Some(Decision::Thread { ts, .. }) = decision(&shell, &target) else {
        panic!("subscribed thread should read")
    };
    assert_eq!(ts, "1719800404.000000");
    shell.core.apply_thread_mark(&key, &ts);
    assert_eq!(decision(&shell, &target), Some(Decision::ClearThread));
    crate::thread_unread::clear_shortcut_divider(&mut shell);
    assert_eq!(decision(&shell, &target), Some(Decision::CloseThread));
    assert_eq!(shell.core.thread_composer.text, "keep this thread draft");
    shell.thread_generation += 1;
    assert_eq!(decision(&shell, &target), None);
}

#[test]
fn unknown_counts_do_not_borrow_default_zero_and_unfocused_thread_never_closes() {
    let mut shell = shell();
    let team = shell.core.active_team.clone().unwrap();
    let channel = shell.core.active_channel.clone().unwrap();
    let meta = shell
        .core
        .workspaces
        .get_mut(&team)
        .unwrap()
        .channels
        .get_mut(&channel)
        .unwrap();
    meta.unread_count = None;
    meta.unread_count_display = None;
    meta.has_unreads = false;
    shell.thread_root = Some("10.000000".into());
    assert_eq!(channel_decision(&shell), None);
    shell.main_view = MainView::Dms;
    shell.surfaces.insert(
        MainView::Dms,
        crate::state::SurfaceTarget {
            channel,
            thread_root: None,
        },
    );
    assert!(
        prepare(&shell, "workspace", None).is_none(),
        "sidebar focus cannot close a primary pane"
    );
}
