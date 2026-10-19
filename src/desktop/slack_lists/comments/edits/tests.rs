use super::*;
use serde_json::json;
use super_platinum_core::slack_lists::comments::{ReplyIntent, ReplyPolicy, ReplyRecord};

const TS: &str = "1700000001.000001";
const ROOT: &str = "1700000000.000001";
fn raw() -> Value {
    json!({"type":"message","channel":"CCOMMENT01","ts":TS,"thread_ts":ROOT,"user":"U1","text":"Before"})
}
fn frame(message: Value) -> RtEvent {
    frame_kind(message, "message_changed")
}
fn frame_kind(message: Value, subtype: &str) -> RtEvent {
    super_platinum_core::slack::realtime::parse_event(
        &json!({"type":"message","subtype":subtype,"channel":"CCOMMENT01","message":message})
            .to_string(),
    )
    .unwrap()
}
fn edit(text: &str) -> RtEvent {
    frame(json!({"ts":TS,"thread_ts":ROOT,"text":text}))
}
fn read(shell: &mut ShellState, messages: Value) {
    let (owner, serial, target, cursor) = controller::begin(shell, false).unwrap();
    controller::finish(shell,&owner,serial,cursor,Ok(network::Loaded {
        page:Page::parse(&target,json!({"ok":true,"messages":messages,"has_more":true,"response_metadata":{"next_cursor":"cursor"}})).unwrap(),
        projection:Projection::observed(json!({"ok":true,"assignments":{}})).unwrap(),
    }));
}
fn setup(kind: u8) -> ShellState {
    let mut shell = super::super::tests::shell();
    shell.core.workspaces.get_mut("T1").unwrap().rt_generation = 7;
    read(
        &mut shell,
        if kind == 0 { json!([raw()]) } else { json!([]) },
    );
    if kind > 0 {
        assert!(incoming::apply(
            &mut shell,
            "T1",
            7,
            &RtEvent::Message(serde_json::from_value(raw()).unwrap())
        ));
        if kind == 1 {
            let e = shell.slack_lists.comments.entry.as_mut().unwrap();
            e.accepted = std::mem::take(&mut e.live);
        }
    }
    shell
}
fn current(shell: &ShellState) -> Message {
    let e = shell.slack_lists.comments.entry.as_ref().unwrap();
    e.page
        .as_ref()
        .unwrap()
        .messages()
        .iter()
        .chain(&e.accepted)
        .chain(&e.live)
        .find(|m| m.ts.as_deref() == Some(TS))
        .unwrap()
        .clone()
}
fn proof(shell: &ShellState) -> Value {
    let e = shell.slack_lists.comments.entry.as_ref().unwrap();
    json!({"page":e.page.as_ref().unwrap().raw(),"accepted":e.accepted,"live":e.live,
        "read":e.read,"loading":e.loading,"error":e.error,"reveal":e.reveal,"cursors":e.cursors,
        "edits":e.live_edits.len(),"draft":draft::fixture_snapshot(shell)})
}

#[test]
fn page_accepted_and_live_stage_exact_delta_without_replacing_raw_cursor_or_reveal() {
    for kind in 0..3 {
        let mut shell = setup(kind);
        let before = proof(&shell);
        let selected = shell.channel_generation;
        assert!(!crate::slack_lists::events::prepare(
            &mut shell,
            "T1",
            7,
            &edit("Changed")
        ));
        assert_eq!(current(&shell).text.as_deref(), Some("Changed"));
        let after = proof(&shell);
        for key in ["page", "read", "loading", "reveal", "cursors", "draft"] {
            assert_eq!(after[key], before[key], "{kind}/{key}");
        }
        assert_eq!(after["edits"], usize::from(kind == 2));
        assert_eq!(shell.channel_generation, selected);
    }
}

#[test]
fn projected_delta_retires_only_page_reads_and_equal_write_keeps_rows_and_read() {
    for preflight in [false, true] {
        let mut shell = setup(2);
        // Establish the source-generated ordinary fields before testing an
        // equal-current projection, rather than assuming raw HTTP is projected.
        assert!(apply(&mut shell, "T1", 7, &edit("Before")));
        let (owner, serial, _, _) = if preflight {
            controller::begin_preflight(&mut shell)
        } else {
            controller::begin(&mut shell, false)
        }
        .unwrap();
        let rows = presentation(&shell).unwrap().rows;
        assert!(!apply(&mut shell, "T1", 7, &edit("Before")));
        assert!(owner.reading(&shell, serial));
        assert!(Arc::ptr_eq(&rows, &presentation(&shell).unwrap().rows));
        assert_eq!(
            shell
                .slack_lists
                .comments
                .entry
                .as_ref()
                .unwrap()
                .live_edits
                .len(),
            1
        );
        assert!(apply(&mut shell, "T1", 7, &edit("After")));
        assert_eq!(owner.reading(&shell, serial), preflight);
        assert_eq!(presentation(&shell).unwrap().error.is_some(), !preflight);
    }
}

#[test]
fn unsupported_keeps_known_data_and_active_canonical_read_with_refresh_guidance() {
    for field in ["files", "attachments"] {
        let mut shell = setup(2);
        let (owner, serial, _, _) = controller::begin(&mut shell, false).unwrap();
        let before = serde_json::to_value(current(&shell)).unwrap();
        let mut value = json!({"ts":TS,"thread_ts":ROOT,"text":"Unsupported"});
        value[field] = if field == "files" {
            json!([{"id":"F_UNSUPPORTED"}])
        } else {
            json!([{"id":1,"text":"Unsupported attachment"}])
        };
        assert!(!apply(&mut shell, "T1", 7, &frame(value)));
        assert!(owner.reading(&shell, serial));
        assert_eq!(serde_json::to_value(current(&shell)).unwrap(), before);
        let e = shell.slack_lists.comments.entry.as_ref().unwrap();
        assert!(e.live_edits.is_empty());
        assert_eq!(e.error.as_deref(), Some(NOTICE));
    }
}

#[test]
fn foreign_missing_identity_replied_root_hidden_and_retired_owners_are_noops() {
    for mode in 0..13 {
        let mut shell = setup(2);
        let mut value = json!({"ts":TS,"thread_ts":ROOT,"text":"Must not apply"});
        let mut team = "T1";
        let mut generation = 7;
        let mut subtype = "message_changed";
        match mode {
            0 => team = "T2",
            1 => generation = 6,
            2 => value["channel"] = json!("OTHER"),
            3 => value["thread_ts"] = json!("1700000099.000001"),
            4 => {
                value.as_object_mut().unwrap().remove("thread_ts");
            }
            5 => value["ts"] = json!("1700000099.000001"),
            6 => value["ts"] = json!(ROOT),
            7 => subtype = "message_replied",
            8 => shell.core.account_epoch += 1,
            9 => shell.slack_lists.comments.open = false,
            10 => {
                shell
                    .slack_lists
                    .comments
                    .entry
                    .as_mut()
                    .unwrap()
                    .deleted
                    .insert(TS.into());
            }
            11 => shell.slack_lists.comments.entry.as_mut().unwrap().rows = Arc::new(vec![]),
            _ => {
                shell
                    .slack_lists
                    .comments
                    .entry
                    .as_mut()
                    .unwrap()
                    .projection = None
            }
        }
        let before = proof(&shell);
        assert!(
            !apply(&mut shell, team, generation, &frame_kind(value, subtype)),
            "{mode}"
        );
        assert_eq!(proof(&shell), before, "{mode}");
    }
}

#[test]
fn typed_identity_tampering_or_legacy_event_without_witness_is_refused() {
    for missing in [true, false] {
        let mut shell = setup(2);
        let mut event = edit("After");
        if let RtEvent::MessageChanged {
            message,
            original_update,
            ..
        } = &mut event
        {
            if missing {
                *original_update = None;
            } else {
                message.ts = Some("1700000002.000001".into());
            }
        }
        let before = proof(&shell);
        assert!(!apply(&mut shell, "T1", 7, &event));
        assert_eq!(proof(&shell), before);
    }
}

fn ack_setup() -> (ShellState, Owner, ReplyRecord, Value) {
    let mut shell = setup(0);
    read(&mut shell, json!([]));
    let e = shell.slack_lists.comments.entry.as_ref().unwrap();
    let owner = e.owner.clone();
    let target = e.target.clone();
    let root = json!({"type":"message","ts":ROOT,"subtype":"list_record_comment","slack_list":{"list_id":target.list_id(),"list_record_id":target.record_id()}});
    let page = Page::parse(&target, json!({"ok":true,"messages":[root]})).unwrap();
    let intent=ReplyIntent::capture_rich(&owner.scope.team,&owner.scope.stable_owner().2,&target,page.root().unwrap(),
        &ReplyPolicy::observed(json!({"id":target.channel_id()}),false,true),
        vec![json!({"type":"rich_text","elements":[{"type":"rich_text_section","elements":[{"type":"text","text":"Original ACK"}]}]})],
        &uuid::Uuid::new_v4().to_string(),"1700000000.900001").unwrap();
    let record = intent.record().clone();
    let raw = json!({"type":"message","channel":target.channel_id(),"ts":TS,"thread_ts":ROOT,"user":record.user(),"client_msg_id":record.client_msg_id(),"text":record.text(),"blocks":record.blocks()});
    assert!(incoming::apply(
        &mut shell,
        "T1",
        7,
        &RtEvent::Message(serde_json::from_value(raw.clone()).unwrap())
    ));
    (shell, owner, record, raw)
}

#[test]
fn late_ack_applies_composed_writes_then_freshest_reactions_and_duplicate_cannot_revert() {
    for remove in [false, true] {
        let (mut shell, owner, record, raw) = ack_setup();
        assert!(apply(&mut shell, "T1", 7, &edit("Edited text")));
        let second = json!({"ts":TS,"thread_ts":ROOT,"text":"Edited text","edited":{"user":"U1","ts":"1700000009.000001"},
            "reactions":if remove {json!([{"name":"wave","count":1,"users":["U2"]}])}else{json!([])}});
        apply(&mut shell, "T1", 7, &frame(second));
        let reaction=super_platinum_core::slack::realtime::parse_event(&json!({"type":if remove {"reaction_removed"}else{"reaction_added"},"user":"U2","reaction":"wave","item":{"type":"message","channel":"CCOMMENT01","ts":TS}}).to_string()).unwrap();
        assert!(reactions::apply(&mut shell, "T1", 7, &reaction));
        let expected = serde_json::to_value(current(&shell)).unwrap();
        assert!(accepted::publish(&mut shell, &owner, &record, &raw));
        assert_eq!(serde_json::to_value(current(&shell)).unwrap(), expected);
        let e = shell.slack_lists.comments.entry.as_ref().unwrap();
        assert!(e.live_edits.is_empty() && e.live_reactions.is_empty() && e.live.is_empty());
        let reveal = e.reveal.clone();
        assert!(accepted::publish(&mut shell, &owner, &record, &raw));
        assert_eq!(serde_json::to_value(current(&shell)).unwrap(), expected);
        assert_eq!(
            shell.slack_lists.comments.entry.as_ref().unwrap().reveal,
            reveal
        );
    }
}

#[test]
fn equal_current_edit_still_protects_live_row_from_older_ack() {
    let (mut shell, owner, record, raw) = ack_setup();
    // Earlier socket already has the newer body; the changed event writes the
    // same values but still owns them against the separately held original ACK.
    let event = edit("Already edited");
    let RtEvent::MessageChanged {
        original_update: Some(update),
        ..
    } = &event
    else {
        panic!("witness")
    };
    let e = shell.slack_lists.comments.entry.as_mut().unwrap();
    let projected = ReplyEdit::project(&e.target, &e.live[0], update)
        .unwrap()
        .apply(&e.live[0])
        .unwrap();
    e.live[0] = projected;
    assert!(!apply(&mut shell, "T1", 7, &edit("Already edited")));
    assert_eq!(
        shell
            .slack_lists
            .comments
            .entry
            .as_ref()
            .unwrap()
            .live_edits
            .len(),
        1
    );
    assert!(accepted::publish(&mut shell, &owner, &record, &raw));
    assert_eq!(current(&shell).text.as_deref(), Some("Already edited"));
    assert!(current(&shell).blocks.is_empty());
}

#[test]
fn canonical_adoption_deletion_and_visit_close_retire_custody() {
    for mode in 0..3 {
        let mut shell = setup(2);
        assert!(apply(&mut shell, "T1", 7, &edit("Edited")));
        match mode {
            0 => read(&mut shell, json!([raw()])),
            1 => {
                assert!(deletions::apply(
                    &mut shell,
                    "T1",
                    7,
                    &RtEvent::MessageDeleted {
                        channel: "CCOMMENT01".into(),
                        deleted_ts: TS.into()
                    }
                ));
            }
            _ => controller::close_shell(&mut shell),
        }
        assert!(
            shell
                .slack_lists
                .comments
                .entry
                .as_ref()
                .unwrap()
                .live_edits
                .is_empty()
        );
    }
}

#[test]
fn cancelled_page_cannot_restore_edited_content_and_omitting_read_keeps_live_custody() {
    let mut shell = setup(2);
    let (owner, serial, target, cursor) = controller::begin(&mut shell, false).unwrap();
    let old = network::Loaded {
        page: Page::parse(&target, json!({"ok":true,"messages":[raw()]})).unwrap(),
        projection: Projection::observed(json!({"ok":true,"assignments":{}})).unwrap(),
    };
    assert!(apply(&mut shell, "T1", 7, &edit("Current edit")));
    let proof_after = proof(&shell);
    controller::finish(&mut shell, &owner, serial, cursor, Ok(old));
    assert_eq!(proof(&shell), proof_after);
    read(&mut shell, json!([]));
    assert_eq!(current(&shell).text.as_deref(), Some("Current edit"));
    assert_eq!(
        shell
            .slack_lists
            .comments
            .entry
            .as_ref()
            .unwrap()
            .live_edits
            .len(),
        1
    );
}

#[test]
fn edit_during_send_preflight_preserves_actual_draft_and_disk_inventory() {
    let (mut shell, binding, _local) = draft::tests::setup();
    shell.core.workspaces.get_mut("T1").unwrap().rt_generation = 7;
    read(&mut shell, json!([raw()]));
    let fields = |s: &ShellState| {
        let e = draft::entry(s, binding.id()).unwrap();
        json!({"id":e.id,"blocks":e.blocks,"sequence":e.sequence,"capture":e.capture,
            "generation":e.generation,"nativeSequence":e.native_sequence,"busy":e.busy,
            "saving":e.saving,"loading":e.loading,"composing":e.composing,"error":e.error,"status":e.status,
            "inventory":e.inventory.as_ref().map(|i|(i.blocks(),i.revision(),i.attempt()))})
    };
    let before = fields(&shell);
    let (owner, serial, _, _) = controller::begin_preflight(&mut shell).unwrap();
    assert!(apply(&mut shell, "T1", 7, &edit("Edited during preflight")));
    assert!(owner.reading(&shell, serial));
    assert!(binding.current(&shell));
    assert_eq!(fields(&shell), before);
}
