use super::*;
use crate::state::ChannelOpen;
use serde_json::json;

fn fixture() -> ShellState {
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    crate::slack_links_check::install(&mut shell);
    shell.main_view = MainView::Activity;
    shell.core.activity.items.clear();
    for (key, ts, root) in [
        ("A", "150.000002", Some("100.000001")),
        ("B", "160.000003", Some("100.000001")),
        ("channel", "170.000004", None),
    ] {
        shell.core.activity.upsert(
            serde_json::from_value(json!({
                "key":key,"feed_ts":"200.0","is_unread":true,
                "item":{"type":"mention","message":{"channel":"C2","ts":ts,"thread_ts":root}}
            }))
            .unwrap(),
        );
    }
    shell
}

fn select(shell: &mut ShellState, key: &str) -> Navigation {
    let item = shell
        .core
        .activity
        .items
        .iter()
        .find(|item| item.key == key)
        .unwrap()
        .clone();
    assert!(shell.select_activity_item(key.into(), item.channel(), item.ts(), item.thread_ts()));
    Navigation::capture(shell, key).unwrap()
}

fn visible_state(shell: &ShellState) -> Value {
    json!({
        "account":shell.core.active_account,"epoch":shell.core.account_epoch,
        "team":shell.core.active_team,"channel":shell.core.active_channel,
        "view":format!("{:?}",shell.main_view),"channel_generation":shell.channel_generation,
        "thread_generation":shell.thread_generation,"root":shell.thread_root,
        "target":shell.thread_target,"loading":shell.thread_loading,
        "selected":shell.core.activity.selected,"items":shell.core.activity.items,
        "surfaces":format!("{:?}",shell.surfaces),"overlay":format!("{:?}",shell.overlay),
    })
}

fn assert_retired(navigation: &Navigation, shell: &mut ShellState) {
    let before = visible_state(shell);
    assert!(navigation.admit(shell).is_none());
    assert_eq!(
        visible_state(shell),
        before,
        "retired start must not select, begin, or mark read"
    );
}

#[test]
fn exact_admission_preserves_activity_and_retires_identical_queued_ticket() {
    let mut shell = fixture();
    let navigation = select(&mut shell, "A");
    let duplicate = Navigation::capture(&shell, "A").unwrap();
    let scope = navigation.admit(&mut shell).unwrap();
    assert_eq!(scope, shell.thread_scope());
    assert_eq!(shell.main_view, MainView::Activity);
    assert_eq!(shell.core.active_channel.as_deref(), Some("C2"));
    assert_eq!(shell.thread_root.as_deref(), Some("100.000001"));
    assert_eq!(shell.thread_target.as_deref(), Some("150.000002"));
    assert!(!shell.thread_at_bottom);
    assert_retired(&navigation, &mut shell);
    assert_retired(&duplicate, &mut shell);
}

#[test]
fn queued_start_cannot_override_navigation_close_or_away_and_back_selection() {
    for change in 0..5 {
        let mut shell = fixture();
        let navigation = select(&mut shell, "A");
        match change {
            0 => {
                let index = shell.channels.iter().position(|c| c.id == "C1").unwrap();
                shell.select_channel(index, ChannelOpen::Global);
            }
            1 => {
                select(&mut shell, "B");
                select(&mut shell, "A");
            }
            2 => shell.close_thread(),
            3 => shell.main_view = MainView::Dms,
            _ => shell.overlay = Some(Overlay::Settings),
        }
        assert_retired(&navigation, &mut shell);
    }
}

#[test]
fn queued_start_rejects_account_viewer_credentials_and_transport_replacement() {
    for change in 0..9 {
        let mut shell = fixture();
        if change == 8 {
            shell.core.transport = Some(Arc::new(Transport::new("same-synthetic-cookie").unwrap()));
        }
        let navigation = select(&mut shell, "A");
        match change {
            0 => shell.core.account_epoch += 1,
            1 => shell.core.active_account = Some("replacement".into()),
            2 => shell.core.active_team = Some("other-team".into()),
            3 => shell.core.workspaces.get_mut("T1").unwrap().self_user_id = "other-user".into(),
            4 => shell.core.transport = Some(Arc::new(Transport::new("synthetic-cookie").unwrap())),
            5 => shell.core.session.as_mut().unwrap().d_cookie = "rotated-cookie".into(),
            6 => {
                shell
                    .core
                    .session
                    .as_mut()
                    .unwrap()
                    .workspaces
                    .get_mut("T1")
                    .unwrap()
                    .token = "rotated-token".into()
            }
            7 => {
                shell
                    .core
                    .session
                    .as_mut()
                    .unwrap()
                    .workspaces
                    .get_mut("T1")
                    .unwrap()
                    .user_id = "other-user".into()
            }
            _ => {
                shell.core.transport =
                    Some(Arc::new(Transport::new("same-synthetic-cookie").unwrap()));
            }
        }
        // All old channel/root IDs remain present; identity coincidence cannot
        // transfer a queued gesture's authority to the replacement owner.
        assert_retired(&navigation, &mut shell);
    }
}

#[test]
fn removed_or_replaced_notification_retires_queued_start_but_read_flag_does_not() {
    for change in 0..3 {
        let mut shell = fixture();
        let navigation = select(&mut shell, "A");
        let at = shell
            .core
            .activity
            .items
            .iter()
            .position(|item| item.key == "A")
            .unwrap();
        match change {
            0 => {
                shell.core.activity.items.remove(at);
            }
            1 => {
                shell.core.activity.items[at]
                    .item
                    .message
                    .as_mut()
                    .unwrap()
                    .ts = Some("replaced".into())
            }
            _ => shell.core.activity.items[at].feed_ts = "new-notification".into(),
        }
        assert_retired(&navigation, &mut shell);
    }
    let mut shell = fixture();
    let navigation = select(&mut shell, "A");
    shell
        .core
        .activity
        .items
        .iter_mut()
        .find(|item| item.key == "A")
        .unwrap()
        .is_unread = false;
    assert!(navigation.admit(&mut shell).is_some());
}

#[test]
fn bundle_and_channel_admission_keep_existing_destinations_and_are_one_shot() {
    let mut shell = fixture();
    shell.core.activity.upsert(
        serde_json::from_value(json!({
            "key":"bundle","item":{"type":"thread_v2","bundle_info":{"payload":{
                "thread_entry":{"channel_id":"C2","thread_ts":"100.000001","latest_ts":"900.0"}
            }}}
        }))
        .unwrap(),
    );
    let bundle = select(&mut shell, "bundle");
    assert!(bundle.admit(&mut shell).is_some());
    assert!(shell.thread_at_bottom);
    assert!(shell.thread_target.is_none());
    assert_retired(&bundle, &mut shell);
    let channel = select(&mut shell, "channel");
    assert!(channel.admit(&mut shell).is_some());
    assert_eq!(shell.main_view, MainView::Activity);
    assert_eq!(shell.core.active_channel.as_deref(), Some("C2"));
    assert!(shell.thread_root.is_none());
    assert_retired(&channel, &mut shell);
}
