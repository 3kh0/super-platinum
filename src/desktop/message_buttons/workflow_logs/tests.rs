use super::*;
use serde_json::{Value, json};
use super_platinum_core::slack::models::Message;

const TS: &str = "1719800000.000001";
fn setup(value: Value) -> (ShellState, Source, BlockElementMetadata) {
    let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
    s.channel_tab = crate::state::ChannelTab::Messages;
    s.overlay = None;
    let button = json!({"type":"button","action_id":WORKFLOW_LOGS_ACTION,
        "text":{"type":"plain_text","text":"View logs"},"value":value});
    // Slackbot has no generic app/service identity. Navigation must still work.
    let message: Message =
        serde_json::from_value(json!({"ts":TS,"channel":"C2","user":"USLACKBOT",
        "blocks":[{"type":"actions","block_id":"logs","elements":[button]}]}))
        .unwrap();
    s.core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .messages
        .entry("C2".into())
        .or_default()
        .upsert(message.clone());
    let row = crate::message_vm::message_vm(&s.core.workspaces["T1"], &message, &s.media);
    let source = Source::capture(&s, &row, "C2", source::Bag::Timeline).unwrap();
    let metadata =
        crate::blocks::action::metadata(&button, Some("logs"), BlockElementSource::Button);
    (s, source, metadata)
}
fn value() -> Value {
    json!(json!({"type":"wfb","wf_id":"WF_REVIEW","execution_id":"EXECUTION"}).to_string())
}
fn canonical(s: &mut ShellState) -> &mut Message {
    s.core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .messages
        .get_mut("C2")
        .unwrap()
        .messages
        .iter_mut()
        .find(|m| m.ts.as_deref() == Some(TS))
        .unwrap()
}

#[test]
fn canonical_slackbot_action_is_enabled_without_generic_service() {
    let (s, source, metadata) = setup(value());
    let pending = capture(&s, &source, &metadata).unwrap();
    assert_eq!(pending.action.workflow_id(), "WF_REVIEW");
    assert_eq!(pending.action.execution_id(), Some("EXECUTION"));
    assert!(pending.current(&s));
    assert!(presentation(&s, &source, &metadata).unwrap().enabled);
    assert!(
        source
            .capture_button(&s, "logs", WORKFLOW_LOGS_ACTION)
            .is_err()
    );
}

#[test]
fn malformed_exact_actions_stay_recognized_and_unavailable_without_generic_fallback() {
    for raw in [
        json!("{"),
        json!(null),
        json!(json!({"type":"wfb","wf_id":"WF_REVIEW","execution_id":7}).to_string()),
    ] {
        let (s, source, metadata) = setup(raw);
        assert!(recognizes(&metadata));
        assert!(capture(&s, &source, &metadata).is_none());
        assert!(!presentation(&s, &source, &metadata).unwrap().enabled);
        assert!(
            super_platinum_core::block_actions::validation::CapturedMessageButton::capture(
                source.owner().clone(),
                source.client_message(&s).unwrap(),
                "logs",
                WORKFLOW_LOGS_ACTION
            )
            .is_err()
        );
        assert!(s.message_buttons.operations.is_empty());
    }
    let (s, source, mut metadata) = setup(value());
    metadata.action_id = Some(format!("{WORKFLOW_LOGS_ACTION}_extra"));
    assert!(!recognizes(&metadata));
    assert!(presentation(&s, &source, &metadata).is_none());
}

#[test]
fn rendered_metadata_cannot_replace_the_canonical_target() {
    let (s, source, mut metadata) = setup(value());
    metadata.value = Some(json!({"type":"wfb","wf_id":"OTHER"}).to_string());
    assert!(capture(&s, &source, &metadata).is_none());
}

#[test]
fn pending_action_rejects_message_navigation_account_and_socket_retirement() {
    for change in 0..5 {
        let (mut s, source, metadata) = setup(value());
        let pending = capture(&s, &source, &metadata).unwrap();
        match change {
            0 => canonical(&mut s).blocks.clear(),
            1 => s.channel_generation += 1,
            2 => s.core.account_epoch += 1,
            3 => s.core.workspaces.get_mut("T1").unwrap().rt_generation += 1,
            _ => s.core.active_channel = Some("OTHER".into()),
        }
        assert!(!pending.current(&s), "retirement {change}");
        assert!(!presentation(&s, &source, &metadata).unwrap().enabled);
    }
}

#[test]
fn duplicate_canonical_buttons_and_same_timestamp_bags_refuse_capture() {
    let (mut s, _, metadata) = setup(value());
    let message = canonical(&mut s);
    let duplicate = message.blocks[0]["elements"][0].clone();
    message.blocks[0]["elements"]
        .as_array_mut()
        .unwrap()
        .push(duplicate);
    let raw = canonical(&mut s).clone();
    let row = crate::message_vm::message_vm(&s.core.workspaces["T1"], &raw, &s.media);
    let source = Source::capture(&s, &row, "C2", source::Bag::Timeline).unwrap();
    assert!(capture(&s, &source, &metadata).is_none());
    s.core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .messages
        .get_mut("C2")
        .unwrap()
        .messages
        .push(raw);
    assert!(source.client_message(&s).is_err());
}

const ROOT: &str = "1719800000.000000";
fn thread_source(s: &mut ShellState) -> Source {
    let mut reply = canonical(s).clone();
    reply.thread_ts = Some(ROOT.into());
    s.thread_root = Some(ROOT.into());
    s.core
        .threads
        .entry(("T1".into(), "C2".into(), ROOT.into()))
        .or_default()
        .upsert(reply.clone());
    let row = crate::message_vm::message_vm(&s.core.workspaces["T1"], &reply, &s.media);
    Source::capture(s, &row, "C2", source::Bag::Thread { root: ROOT.into() }).unwrap()
}
fn pin_source(s: &mut ShellState) -> Source {
    // A pinned reply has Thread row presentation, but Pins action custody.
    let mut reply = canonical(s).clone();
    reply.thread_ts = Some(ROOT.into());
    let item = super_platinum_core::pins::PinnedItem {
        kind: "message".into(),
        channel: Some("C2".into()),
        message: Some(reply.clone()),
        ..Default::default()
    };
    let key = item.key().unwrap();
    s.core.pins.ensure_scope(s.core.account_epoch, "T1");
    let generation = s.core.pins.begin("C2");
    assert!(s.core.pins.apply(
        generation,
        super_platinum_core::pins::PinsPage { items: vec![item] }
    ));
    s.channel_tab = crate::state::ChannelTab::Pins;
    let row = crate::message_vm::message_vm(&s.core.workspaces["T1"], &reply, &s.media);
    Source::capture(s, &row, "C2", source::Bag::Pins { item_key: key }).unwrap()
}

#[test]
fn thread_logs_use_canonical_reply_and_distinct_key_without_timeline_fallback() {
    let (mut s, timeline, metadata) = setup(value());
    let thread = thread_source(&mut s);
    assert_ne!(timeline.client_key(&metadata), thread.client_key(&metadata));
    let pending = capture(&s, &thread, &metadata).unwrap();
    canonical(&mut s).blocks[0]["elements"][0]["value"] = json!("not the thread action");
    assert!(pending.current(&s));
    assert_eq!(pending.action.workflow_id(), "WF_REVIEW");
    // Keeping a matching timeline copy cannot rescue a removed thread source.
    canonical(&mut s).blocks[0]["elements"][0]["value"] = value();
    s.core
        .threads
        .remove(&("T1".into(), "C2".into(), ROOT.into()));
    assert!(!pending.current(&s));
    assert!(capture(&s, &thread, &metadata).is_none());
}

#[test]
fn thread_logs_retire_on_root_generation_or_canonical_action_change() {
    for change in 0..3 {
        let (mut s, _, metadata) = setup(value());
        let source = thread_source(&mut s);
        let pending = capture(&s, &source, &metadata).unwrap();
        match change {
            0 => s.thread_root = Some("another-root".into()),
            1 => s.thread_generation += 1,
            _ => s
                .core
                .threads
                .get_mut(&("T1".into(), "C2".into(), ROOT.into()))
                .unwrap()
                .messages[0]
                .blocks
                .clear(),
        }
        assert!(!pending.current(&s), "thread retirement {change}");
    }
}

#[test]
fn pinned_reply_logs_use_pin_copy_without_thread_or_transcript_dependency() {
    let (mut s, timeline, metadata) = setup(value());
    let pin = pin_source(&mut s);
    assert_ne!(timeline.client_key(&metadata), pin.client_key(&metadata));
    let pending = capture(&s, &pin, &metadata).unwrap();
    s.thread_root = None;
    s.core.threads.clear();
    s.core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .messages
        .remove("C2");
    assert!(pending.current(&s));
    assert!(presentation(&s, &pin, &metadata).unwrap().enabled);
    assert_eq!(pending.action.execution_id(), Some("EXECUTION"));
    s.core.pins.items.clear();
    assert!(!pending.current(&s));
}

#[test]
fn pinned_logs_retire_on_same_channel_refresh_tab_or_canonical_action_change() {
    for change in 0..3 {
        let (mut s, _, metadata) = setup(value());
        let source = pin_source(&mut s);
        let pending = capture(&s, &source, &metadata).unwrap();
        match change {
            0 => {
                s.core.pins.begin("C2");
            }
            1 => s.channel_tab = crate::state::ChannelTab::Messages,
            _ => s
                .core
                .pins
                .items
                .values_mut()
                .next()
                .unwrap()
                .message
                .as_mut()
                .unwrap()
                .blocks
                .clear(),
        }
        assert!(!pending.current(&s), "pin retirement {change}");
    }
}
