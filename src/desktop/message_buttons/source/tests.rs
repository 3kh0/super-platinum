use super::*;
use crate::media::MediaRegistry;

mod select;

const TS: &str = "1719800000.000001";
const ROOT: &str = "1719800000.000000";

fn message() -> Message {
    serde_json::from_value(json!({
        "ts":TS,"channel":"C2","bot_id":"B1","team":"T_EXTERNAL",
        "bot_profile":{"id":"B1","app_id":"A1","team_id":"TAPPHOME1"},
        "blocks":[{"type":"actions","block_id":"review","elements":[{
            "type":"button","action_id":"approve","text":{"type":"plain_text","text":"Approve"},
            "url":"https://example.test/approve","value":"item-1",
            "confirm":{"title":{"type":"plain_text","text":"Confirm"},
                "text":{"type":"mrkdwn","text":"Approve *this* item?"},
                "confirm":{"type":"plain_text","text":"Approve"},
                "deny":{"type":"plain_text","text":"Cancel"}}
        }]}]
    }))
    .unwrap()
}
fn shell() -> ShellState {
    let mut shell = ShellState::fixture(MediaRegistry::default());
    shell.channel_tab = ChannelTab::Messages;
    shell
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .messages
        .entry("C2".into())
        .or_default()
        .upsert(message());
    shell
}
fn row(shell: &ShellState, message: &Message) -> MessageVm {
    crate::message_vm::message_vm(&shell.core.workspaces["T1"], message, &shell.media)
}
fn capture(shell: &ShellState) -> Source {
    Source::capture(shell, &row(shell, &message()), "C2", Bag::Timeline).unwrap()
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
        .find(|message| message.ts.as_deref() == Some(TS))
        .unwrap()
}

#[test]
fn captures_canonical_app_confirmation_and_wire_identity_without_author_team_inference() {
    let mut shell = shell();
    canonical(&mut shell).bot_profile.as_mut().unwrap().id = Some("B_CACHED_PROFILE".into());
    let source = capture(&shell);
    let captured = source.capture_button(&shell, "review", "approve").unwrap();
    assert_eq!(source.owner().team_id, "T1");
    assert_eq!(captured.app_id(), "A1");
    assert_eq!(captured.app_team_id(), Some("TAPPHOME1"));
    assert_eq!(captured.url(), Some("https://example.test/approve"));
    assert_eq!(
        captured
            .confirmation()
            .unwrap()
            .title
            .as_ref()
            .unwrap()
            .text,
        "Confirm"
    );
    assert_eq!(captured.metadata().action_id.as_deref(), Some("approve"));
    assert_eq!(captured.source().owner(), source.owner());
    let current = captured.revalidate(&shell).unwrap();
    assert_eq!(current.service_id(), "B1");
    assert_eq!(current.action()["block_id"], "review");
    assert_eq!(current.action()["action_id"], "approve");
    assert!(current.action().get("url").is_none());
    assert_eq!(current.container()["channel_id"], "C2");
}

#[test]
fn rendered_revision_rejects_edit_before_click_and_confirmation_rejects_later_edit() {
    let mut shell = shell();
    let source = capture(&shell);
    let captured = source.capture_button(&shell, "review", "approve").unwrap();
    // Reactions are independent of action identity and should not retire it.
    canonical(&mut shell).reactions =
        serde_json::from_value(json!([{"name":"eyes","count":1,"users":["U1"]}])).unwrap();
    assert!(source.current(&shell));
    assert!(captured.revalidate(&shell).is_ok());
    canonical(&mut shell).blocks[0]["elements"][0]["value"] = json!("replacement");
    assert!(!source.current(&shell));
    assert_eq!(
        source.capture_button(&shell, "review", "approve").err(),
        Some(MessageActionError::StaleMessage)
    );
    assert_eq!(
        captured.revalidate(&shell).err(),
        Some(MessageActionError::StaleMessage)
    );
}

#[test]
fn exact_timeline_rejects_missing_pending_duplicate_and_conflicting_ownership() {
    for case in 0..6 {
        let mut shell = shell();
        let vm = row(&shell, &message());
        match case {
            0 => {
                shell
                    .core
                    .workspaces
                    .get_mut("T1")
                    .unwrap()
                    .messages
                    .remove("C2");
            }
            1 => shell
                .core
                .workspaces
                .get_mut("T1")
                .unwrap()
                .messages
                .get_mut("C2")
                .unwrap()
                .pending
                .push(TS.into()),
            2 => shell
                .core
                .workspaces
                .get_mut("T1")
                .unwrap()
                .messages
                .get_mut("C2")
                .unwrap()
                .messages
                .push(message()),
            3 => canonical(&mut shell).channel = Some("C_OTHER".into()),
            4 => {
                shell
                    .core
                    .workspaces
                    .get_mut("T1")
                    .unwrap()
                    .messages
                    .get_mut("C2")
                    .unwrap()
                    .remote_deletions
                    .insert(TS.into());
            }
            _ => shell.core.active_channel = Some("C_OTHER".into()),
        }
        assert!(Source::capture(&shell, &vm, "C2", Bag::Timeline).is_err());
    }
    let shell = shell();
    let mut vm = row(&shell, &message());
    vm.pending = true;
    assert!(Source::capture(&shell, &vm, "C2", Bag::Timeline).is_err());
}

#[test]
fn thread_and_pins_resolve_only_the_exact_declared_bag() {
    let mut shell = shell();
    shell.thread_root = Some(ROOT.into());
    let bag = Bag::Thread { root: ROOT.into() };
    let vm = row(&shell, &message());
    // A same-ts timeline message is not a thread owner or a fallback.
    assert!(Source::capture(&shell, &vm, "C2", bag.clone()).is_err());
    let mut reply = message();
    reply.thread_ts = Some(ROOT.into());
    shell
        .core
        .threads
        .entry(("T1".into(), "C2".into(), ROOT.into()))
        .or_default()
        .upsert(reply);
    let source = Source::capture(&shell, &vm, "C2", bag).unwrap();
    canonical(&mut shell).blocks.clear();
    assert!(source.capture_button(&shell, "review", "approve").is_ok());
    shell.thread_root = Some("OTHER".into());
    assert!(!source.current(&shell));
    assert!(source.revision_current(&shell));

    shell.channel_tab = ChannelTab::Pins;
    let key = format!("message:{TS}");
    let bag = Bag::Pins {
        item_key: key.clone(),
    };
    assert!(Source::capture(&shell, &vm, "C2", bag.clone()).is_err());
    shell.core.pins.ensure_scope(shell.core.account_epoch, "T1");
    shell.core.pins.channel = Some("C2".into());
    shell.core.pins.items.insert(
        key.clone(),
        serde_json::from_value(json!({"type":"message","channel":"C2","message":message()}))
            .unwrap(),
    );
    let source = Source::capture(&shell, &vm, "C2", bag).unwrap();
    assert!(source.capture_button(&shell, "review", "approve").is_ok());
    shell.core.pins.items.get_mut(&key).unwrap().channel = Some("C_OTHER".into());
    assert!(!source.current(&shell));
}

#[test]
fn visits_expire_confirmation_but_do_not_unlock_the_same_uncertain_action() {
    let mut shell = shell();
    let source = capture(&shell);
    shell.channel_generation += 1;
    shell.core.active_channel = Some("C1".into());
    shell.main_view = MainView::Activity;
    shell.channel_tab = ChannelTab::Pins;
    shell.core.workspaces.get_mut("T1").unwrap().rt_generation += 1;
    assert!(!source.current(&shell));
    assert!(source.revision_current(&shell));
    canonical(&mut shell).blocks[0]["elements"][0]["text"]["text"] = json!("Changed action");
    assert!(!source.revision_current(&shell));
}

#[test]
fn same_action_lock_survives_original_bag_eviction_and_ordinary_thread_annotations() {
    let mut shell = shell();
    let original = capture(&shell);
    shell
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .messages
        .remove("C2");
    shell.channel_generation += 1;
    shell.thread_root = Some(ROOT.into());
    shell.thread_generation += 1;
    let mut reply = message();
    reply.channel = None;
    reply.thread_ts = Some(ROOT.into());
    reply.edited = Some(json!({"ts":"1719800001.000000"}));
    reply.bot_profile.as_mut().unwrap().id = Some("B_DIFFERENT_PROFILE".into());
    shell
        .core
        .threads
        .entry(("T1".into(), "C2".into(), ROOT.into()))
        .or_default()
        .upsert(reply.clone());
    let candidate = Source::capture(
        &shell,
        &row(&shell, &reply),
        "C2",
        Bag::Thread { root: ROOT.into() },
    )
    .unwrap();
    assert!(!original.current(&shell));
    assert!(!original.revision_current(&shell));
    assert!(
        candidate
            .capture_button(&shell, "review", "approve")
            .is_ok()
    );
    assert!(
        original.same_action_revision(&candidate),
        "missing original storage must not unlock the same operation"
    );
    let bag = shell
        .core
        .threads
        .get_mut(&("T1".into(), "C2".into(), ROOT.into()))
        .unwrap();
    bag.messages[0].blocks[0]["elements"][0]["value"] = json!("new-action");
    let changed = Source::capture(
        &shell,
        &row(&shell, &reply),
        "C2",
        Bag::Thread { root: ROOT.into() },
    )
    .unwrap();
    assert!(!original.same_action_revision(&changed));
}

#[test]
fn same_action_identity_keeps_viewer_routing_and_ephemeral_thread_context() {
    let mut shell = shell();
    canonical(&mut shell)
        .extra
        .insert("is_ephemeral".into(), json!(true));
    let original = capture(&shell);
    canonical(&mut shell).thread_ts = Some(ROOT.into());
    assert!(!original.same_action_revision(&capture(&shell)));
    canonical(&mut shell).thread_ts = None;
    assert!(original.same_action_revision(&capture(&shell)));
    canonical(&mut shell)
        .bot_profile
        .as_mut()
        .unwrap()
        .extra
        .insert("team_id".into(), json!("TOTHER123"));
    assert!(!original.same_action_revision(&capture(&shell)));
    canonical(&mut shell)
        .bot_profile
        .as_mut()
        .unwrap()
        .extra
        .insert("team_id".into(), json!("TAPPHOME1"));
    shell.core.workspaces.get_mut("T1").unwrap().self_user_id = "U_OTHER".into();
    assert!(!original.same_action_revision(&capture(&shell)));
}

#[test]
fn account_team_user_and_generation_changes_invalidate_capture() {
    for case in 0..5 {
        let mut shell = shell();
        let source = capture(&shell);
        let captured = source.capture_button(&shell, "review", "approve").unwrap();
        match case {
            0 => shell.core.account_epoch += 1,
            1 => shell.core.active_team = Some("T_OTHER".into()),
            2 => shell.core.workspaces.get_mut("T1").unwrap().self_user_id = "OTHER".into(),
            3 => shell.core.workspaces.get_mut("T1").unwrap().rt_generation += 1,
            _ => shell.channel_generation += 1,
        }
        assert!(!source.current(&shell));
        assert!(captured.revalidate(&shell).is_err());
    }
}

#[test]
fn special_missing_app_identity_and_stateful_messages_are_inert() {
    for case in 0..6 {
        let mut shell = shell();
        let m = canonical(&mut shell);
        match case {
            0 => m.bot_id = Some("BSLACKBOT".into()),
            1 => m.bot_profile = None,
            2 => { m.bot_profile.as_mut().unwrap().extra.remove("app_id"); },
            3 => { m.bot_profile.as_mut().unwrap().extra.insert("app_id".into(), json!("")); },
            4 => { m.bot_profile.as_mut().unwrap().extra.insert("team_id".into(), json!([])); },
            _ => m.blocks.push(json!({"type":"actions","block_id":"selection","elements":[{"type":"static_select","action_id":"choice","options":[]}]})),
        }
        assert!(
            capture(&shell)
                .capture_button(&shell, "review", "approve")
                .is_err()
        );
    }
}

#[test]
fn cloned_sources_share_snapshots_but_still_reject_canonical_edits() {
    let mut shell = shell();
    let original = capture(&shell);
    let copied = original.clone();
    assert!(Arc::ptr_eq(&original.revision, &copied.revision));
    assert!(Arc::ptr_eq(&original.wire_revision, &copied.wire_revision));
    let fresh = capture(&shell);
    assert!(!Arc::ptr_eq(&original.wire_revision, &fresh.wire_revision));
    assert!(original.same_action_revision(&fresh));
    canonical(&mut shell).blocks[0]["elements"][0]["value"] = json!("changed");
    assert!(!original.current(&shell));
    assert!(!copied.current(&shell));
    assert!(!copied.same_action_revision(&capture(&shell)));
}

#[test]
fn inline_app_identity_wins_over_a_conflicting_message_lookup_hint() {
    let mut shell = shell();
    canonical(&mut shell)
        .extra
        .insert("app_id".into(), json!("A_OTHER"));
    let source = capture(&shell);
    let captured = source.capture_button(&shell, "review", "approve").unwrap();
    assert_eq!(captured.app_id(), "A1");
    assert_eq!(captured.app_team_id(), Some("TAPPHOME1"));
    assert_eq!(captured.revalidate(&shell).unwrap().service_id(), "B1");
}

fn cache_profile(shell: &mut ShellState, app: &str, team: &str, seconds: u64) {
    use std::time::Duration;
    let cache = &mut shell.core.workspaces.get_mut("T1").unwrap().bots;
    let now = Duration::from_secs(seconds);
    let ticket = cache.begin_retry("B1", None, now).unwrap();
    let profile =
        serde_json::from_value(json!({"id":"B1","app_id":app,"team_id":team,"updated":seconds}))
            .unwrap();
    assert!(cache.complete(&ticket, Some(profile), now));
}

#[test]
fn cache_completion_enables_only_a_fresh_rendered_source_without_mutating_message_metadata() {
    let mut shell = shell();
    canonical(&mut shell).bot_profile = None;
    let before = capture(&shell);
    assert_eq!(before.lookup_target(&shell), Some(("B1".into(), None)));
    assert!(before.capture_button(&shell, "review", "approve").is_err());
    cache_profile(&mut shell, "A1", "TAPPHOME1", 1);
    assert!(canonical(&mut shell).bot_profile.is_none());
    assert!(!before.current(&shell));
    assert!(!before.revision_current(&shell));
    assert!(before.capture_button(&shell, "review", "approve").is_err());
    let fresh = capture(&shell);
    assert!(fresh.lookup_target(&shell).is_none());
    let captured = fresh.capture_button(&shell, "review", "approve").unwrap();
    assert_eq!(captured.app_id(), "A1");
    assert_eq!(captured.app_team_id(), Some("TAPPHOME1"));
    assert!(captured.revalidate(&shell).is_ok());
    assert!(canonical(&mut shell).bot_profile.is_none());
}

#[test]
fn updated_cached_app_or_team_invalidates_captured_action_without_a_message_edit() {
    for change_app in [false, true] {
        let mut shell = shell();
        canonical(&mut shell).bot_profile = None;
        cache_profile(&mut shell, "A1", "TAPPHOME1", 1);
        let source = capture(&shell);
        let captured = source.capture_button(&shell, "review", "approve").unwrap();
        let next_app = if change_app { "A2" } else { "A1" };
        let next_team = if change_app { "TAPPHOME1" } else { "TOTHER123" };
        cache_profile(&mut shell, next_app, next_team, 2);
        assert!(!source.current(&shell));
        assert!(!source.revision_current(&shell));
        assert_eq!(
            captured.revalidate(&shell).err(),
            Some(MessageActionError::StaleMessage)
        );
        let fresh = capture(&shell);
        assert!(!source.same_action_revision(&fresh));
        let next = fresh.capture_button(&shell, "review", "approve").unwrap();
        assert_eq!(next.app_id(), next_app);
        assert_eq!(next.app_team_id(), Some(next_team));
        assert!(canonical(&mut shell).bot_profile.is_none());
    }
}

#[test]
fn member_routing_change_invalidates_confirmation_without_a_message_edit() {
    let mut shell = shell();
    let message = canonical(&mut shell);
    message.user = Some("U_APP".into());
    message
        .bot_profile
        .as_mut()
        .unwrap()
        .extra
        .remove("team_id");
    let source = capture(&shell);
    let captured = source.capture_button(&shell, "review", "approve").unwrap();
    assert_eq!(captured.app_team_id(), Some("T1"));
    assert!(captured.confirmation().is_some());
    let member = serde_json::from_value(json!({"id":"U_APP","team_id":"TOTHER123"})).unwrap();
    shell
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .users
        .insert("U_APP".into(), member);
    assert!(!source.current(&shell));
    assert!(!source.revision_current(&shell));
    assert_eq!(
        captured.revalidate(&shell).err(),
        Some(MessageActionError::StaleMessage)
    );
    assert_eq!(
        capture(&shell)
            .capture_button(&shell, "review", "approve")
            .unwrap()
            .app_team_id(),
        Some("TOTHER123")
    );
}
