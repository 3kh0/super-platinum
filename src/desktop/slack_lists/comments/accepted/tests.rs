use super::*;
use serde_json::json;
use super_platinum_core::slack_lists::comments::{ReplyIntent, ReplyPolicy};

const SENT: &str = "1700000003.000001";

#[test]
fn late_ack_preserves_live_reaction_delta_including_removal_to_empty() {
    for removed in [false, true] {
        let (mut shell, owner, record, mut raw) = setup();
        shell.core.workspaces.get_mut("T1").unwrap().rt_generation = 7;
        let mut socket = raw.clone();
        if removed {
            socket["reactions"] = json!([{"name":"wave","count":1,"users":["U1"]}]);
            raw["reactions"] = socket["reactions"].clone();
        }
        assert!(super::super::incoming::apply(
            &mut shell,
            "T1",
            7,
            &super_platinum_core::slack::events::RtEvent::Message(
                serde_json::from_value(socket).unwrap()
            )
        ));
        let delta=super_platinum_core::slack::realtime::parse_event(&json!({
            "type":if removed {"reaction_removed"} else {"reaction_added"},"user":"U1","reaction":"wave",
            "item":{"type":"message","channel":record.target().channel_id(),"ts":SENT}
        }).to_string()).unwrap();
        assert!(super::super::reactions::apply(&mut shell, "T1", 7, &delta));
        let current = serde_json::to_value(
            &shell.slack_lists.comments.entry.as_ref().unwrap().live[0].reactions,
        )
        .unwrap();
        assert!(publish(&mut shell, &owner, &record, &raw));
        let e = shell.slack_lists.comments.entry.as_ref().unwrap();
        assert!(e.live.is_empty() && e.live_reactions.is_empty());
        assert_eq!(
            serde_json::to_value(&e.accepted[0].reactions).unwrap(),
            current
        );
        assert_eq!(
            e.rows
                .iter()
                .find(|r| r.ts == SENT)
                .unwrap()
                .reactions
                .len(),
            usize::from(!removed)
        );
    }
}

#[test]
fn socket_and_ack_in_either_order_show_one_row_without_extra_reveal_or_settlement() {
    for socket_first in [false, true] {
        let (mut shell, owner, record, raw) = setup();
        shell.core.workspaces.get_mut("T1").unwrap().rt_generation = 7;
        let event = super_platinum_core::slack::events::RtEvent::Message(
            serde_json::from_value(raw.clone()).unwrap(),
        );
        if socket_first {
            assert!(super::super::incoming::apply(&mut shell, "T1", 7, &event));
            let e = shell.slack_lists.comments.entry.as_ref().unwrap();
            assert!(e.accepted.is_empty() && e.reveal.is_none());
            assert_eq!(e.live.len(), 1);
        }
        assert!(publish(&mut shell, &owner, &record, &raw));
        let reveal = shell
            .slack_lists
            .comments
            .entry
            .as_ref()
            .unwrap()
            .reveal
            .clone();
        assert!(!super::super::incoming::apply(&mut shell, "T1", 7, &event));
        let e = shell.slack_lists.comments.entry.as_ref().unwrap();
        assert!(e.live.is_empty());
        assert_eq!(e.accepted.len(), 1);
        assert_eq!(e.rows.iter().filter(|r| r.ts == SENT).count(), 1);
        assert_eq!(e.reveal, reveal);
    }
}

#[test]
fn socket_deleted_reply_cannot_be_republished_by_a_late_settlement() {
    let (mut shell, owner, record, raw) = setup();
    shell.core.workspaces.get_mut("T1").unwrap().rt_generation = 7;
    assert!(publish(&mut shell, &owner, &record, &raw));
    let event = super_platinum_core::slack::events::RtEvent::MessageDeleted {
        channel: record.target().channel_id().into(),
        deleted_ts: SENT.into(),
    };
    assert!(super::super::deletions::apply(&mut shell, "T1", 7, &event));
    let surviving: Vec<_> = presentation(&shell)
        .unwrap()
        .rows
        .iter()
        .map(|row| row.ts.clone())
        .collect();
    assert!(!surviving.iter().any(|ts| ts == SENT));
    assert!(!publish(&mut shell, &owner, &record, &raw));
    assert_eq!(
        presentation(&shell)
            .unwrap()
            .rows
            .iter()
            .map(|row| row.ts.clone())
            .collect::<Vec<_>>(),
        surviving
    );
}

fn read(shell: &mut ShellState, raw: Value, paging: bool) {
    let (owner, serial, target, cursor) = controller::begin(shell, paging).unwrap();
    let page = Page::parse(&target, raw).unwrap();
    controller::finish(
        shell,
        &owner,
        serial,
        cursor,
        Ok(network::Loaded {
            page,
            projection: Projection::observed(json!({"ok":true,"assignments":{}})).unwrap(),
        }),
    );
}
fn setup() -> (ShellState, Owner, ReplyRecord, Value) {
    let mut shell = super::super::tests::shell();
    let owner = shell
        .slack_lists
        .comments
        .entry
        .as_ref()
        .unwrap()
        .owner
        .clone();
    let target = shell
        .slack_lists
        .comments
        .entry
        .as_ref()
        .unwrap()
        .target
        .clone();
    let root = json!({"type":"message","ts":target.root_ts(),"subtype":"list_record_comment",
        "slack_list":{"list_id":target.list_id(),"list_record_id":target.record_id()}});
    let page = Page::parse(&target, json!({"ok":true,"messages":[root.clone()]})).unwrap();
    let blocks = vec![
        json!({"type":"rich_text","elements":[{"type":"rich_text_section","elements":[
        {"type":"text","text":"Accepted rich reply","style":{"bold":true}},
        {"type":"text","text":"\nSecond line"}]}]}),
    ];
    let intent = ReplyIntent::capture_rich(
        &owner.scope.team,
        &owner.scope.stable_owner().2,
        &target,
        page.root().unwrap(),
        &ReplyPolicy::observed(json!({"id":target.channel_id()}), false, true),
        blocks,
        &uuid::Uuid::new_v4().to_string(),
        "1700000002.000001",
    )
    .unwrap();
    let record = intent.record().clone();
    let message = json!({"type":"message","channel":target.channel_id(),"thread_ts":target.root_ts(),
        "ts":SENT,"user":record.user(),"client_msg_id":record.client_msg_id(),"blocks":record.blocks(),"text":record.text()});
    let accepted = intent
        .accept(json!({"ok":true,"channel":target.channel_id(),"ts":SENT,"message":message}))
        .unwrap();
    let raw = serde_json::to_value(accepted.message()).unwrap();
    read(
        &mut shell,
        json!({"ok":true,"messages":[root],"has_more":true,"response_metadata":{"next_cursor":"next"}}),
        false,
    );
    (shell, owner, record, raw)
}

#[test]
fn accepted_reply_appears_once_without_mutating_server_page_or_cursor() {
    let (mut shell, owner, record, raw) = setup();
    let page = shell
        .slack_lists
        .comments
        .entry
        .as_ref()
        .unwrap()
        .page
        .as_ref()
        .unwrap()
        .raw()
        .clone();
    let selected = shell.active_channel.clone();
    let generation = shell.channel_generation;
    assert!(publish(&mut shell, &owner, &record, &raw));
    let entry = shell.slack_lists.comments.entry.as_ref().unwrap();
    assert_eq!(entry.rows.len(), 1);
    assert_eq!(entry.rows[0].ts, SENT);
    assert!(!entry.rows[0].pending);
    assert_eq!(entry.accepted[0].blocks, record.blocks());
    assert_eq!(entry.page.as_ref().unwrap().raw(), &page);
    assert!(presentation(&shell).unwrap().has_more);
    assert_eq!(shell.active_channel, selected);
    assert_eq!(shell.channel_generation, generation);
    let reveal = entry.reveal.clone();
    assert!(publish(&mut shell, &owner, &record, &raw));
    let entry = shell.slack_lists.comments.entry.as_ref().unwrap();
    assert_eq!(entry.rows.len(), 1);
    assert_eq!(entry.accepted.len(), 1);
    assert_eq!(entry.reveal, reveal);
}

#[test]
fn canonical_read_deduplicates_and_replaces_local_content_without_revealing_again() {
    let (mut shell, owner, record, raw) = setup();
    assert!(publish(&mut shell, &owner, &record, &raw));
    let reveal = presentation(&shell).unwrap().reveal;
    let mut canonical = raw;
    canonical["blocks"] = json!([]);
    canonical["text"] = json!("Edited on server");
    canonical["reactions"] = json!([{"name":"wave","count":2,"users":["U1","U2"]}]);
    read(
        &mut shell,
        json!({"ok":true,"messages":[canonical],"has_more":true,"response_metadata":{"next_cursor":"next-two"}}),
        true,
    );
    let entry = shell.slack_lists.comments.entry.as_ref().unwrap();
    assert_eq!(entry.rows.len(), 1);
    assert_eq!(entry.rows[0].ts, SENT);
    assert_eq!(entry.rows[0].reactions.len(), 1);
    assert!(entry.accepted.is_empty());
    assert_eq!(
        entry.page.as_ref().unwrap().messages()[0].text.as_deref(),
        Some("Edited on server")
    );
    assert_eq!(entry.page.as_ref().unwrap().cursor(), Some("next-two"));
    assert_eq!(entry.cursors, ["next"]);
    assert_eq!(entry.reveal, reveal);
}

#[test]
fn partial_page_retains_accepted_reply_but_explicit_deletion_retires_it() {
    let (mut shell, owner, record, raw) = setup();
    assert!(publish(&mut shell, &owner, &record, &raw));
    read(
        &mut shell,
        json!({"ok":true,"messages":[],"has_more":true,"response_metadata":{"next_cursor":"next-two"}}),
        true,
    );
    assert_eq!(presentation(&shell).unwrap().rows[0].ts, SENT);
    read(
        &mut shell,
        json!({"ok":true,"messages":[],"deleted":[SENT],"has_more":false}),
        false,
    );
    assert!(presentation(&shell).unwrap().rows.is_empty());
    assert!(
        shell
            .slack_lists
            .comments
            .entry
            .as_ref()
            .unwrap()
            .accepted
            .is_empty()
    );
    assert!(presentation(&shell).unwrap().reveal.is_none());
    assert!(!publish(&mut shell, &owner, &record, &raw));
}

#[test]
fn hidden_canonical_row_wins_over_accepted_tail() {
    let (mut shell, owner, record, mut raw) = setup();
    assert!(publish(&mut shell, &owner, &record, &raw));
    raw["isNonExistent"] = json!(true);
    read(&mut shell, json!({"ok":true,"messages":[raw]}), false);
    assert!(presentation(&shell).unwrap().rows.is_empty());
    assert!(
        shell
            .slack_lists
            .comments
            .entry
            .as_ref()
            .unwrap()
            .accepted
            .is_empty()
    );
}

#[test]
fn wrong_acknowledgment_identity_is_never_projected() {
    for key in ["client_msg_id", "user", "thread_ts", "channel"] {
        let (mut shell, owner, record, mut raw) = setup();
        raw[key] = json!("foreign");
        assert!(!publish(&mut shell, &owner, &record, &raw));
        assert!(
            shell
                .slack_lists
                .comments
                .entry
                .as_ref()
                .unwrap()
                .accepted
                .is_empty()
        );
        assert!(presentation(&shell).unwrap().reveal.is_none());
    }
}

#[test]
fn old_owner_cannot_publish_after_account_epoch_or_same_item_reopen() {
    for reopened in [false, true] {
        let (mut shell, owner, record, raw) = setup();
        if reopened {
            controller::close_shell(&mut shell);
            let generation = record_detail::presentation(shell.slack_lists.view.as_ref().unwrap())
                .unwrap()
                .generation;
            assert!(controller::open_shell(&mut shell, generation));
        } else {
            shell.core.account_epoch += 1;
        }
        assert!(!publish(&mut shell, &owner, &record, &raw));
        let entry = shell.slack_lists.comments.entry.as_ref().unwrap();
        assert!(entry.accepted.is_empty());
        assert!(entry.reveal.is_none());
    }
}
