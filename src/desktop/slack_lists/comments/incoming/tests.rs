use super::*;
use serde_json::json;
use super_platinum_core::slack::models::Message;

const TS: &str = "1700000001.000001";
fn install(shell: &mut ShellState) {
    shell.core.workspaces.get_mut("T1").unwrap().rt_generation = 7;
    let (owner, serial, target, cursor) = controller::begin(shell, false).unwrap();
    controller::finish(
        shell,
        &owner,
        serial,
        cursor,
        Ok(network::Loaded {
            page: Page::parse(
                &target,
                json!({"ok":true,"messages":[],"has_more":true,
            "response_metadata":{"next_cursor":"next"}}),
            )
            .unwrap(),
            projection: Projection::observed(json!({"ok":true,"assignments":{}})).unwrap(),
        }),
    );
}
fn setup() -> ShellState {
    let mut shell = super::super::tests::shell();
    install(&mut shell);
    shell
}
fn raw(ts: &str) -> Value {
    json!({"type":"message","channel":"CCOMMENT01","ts":ts,"thread_ts":"1700000000.000001",
        "user":"U1","text":"Rich arrival","blocks":[{"type":"rich_text","elements":[{
            "type":"rich_text_section","elements":[{"type":"text","text":"Rich arrival","style":{"bold":true}}]}]}],
        "opaque":{"retained":[2,1]}})
}
fn frame(value: Value) -> RtEvent {
    super_platinum_core::slack::realtime::parse_event(&value.to_string()).unwrap()
}
fn finish(shell: &mut ShellState, messages: Value, paging: bool) {
    let (owner, serial, target, cursor) = controller::begin(shell, paging).unwrap();
    controller::finish(
        shell,
        &owner,
        serial,
        cursor,
        Ok(network::Loaded {
            page: Page::parse(
                &target,
                json!({"ok":true,"messages":messages,"has_more":true,
            "response_metadata":{"next_cursor":"next2"}}),
            )
            .unwrap(),
            projection: Projection::observed(json!({"ok":true,"assignments":{}})).unwrap(),
        }),
    );
}

#[test]
fn parsed_arrival_preserves_evidence_pagination_owner_and_duplicate_rows() {
    let mut shell = setup();
    let before = shell.slack_lists.comments.entry.as_ref().unwrap().clone();
    let selected = shell.active_channel;
    let navigation = shell.channel_generation;
    assert!(!crate::slack_lists::events::prepare(
        &mut shell,
        "T1",
        7,
        &frame(raw(TS))
    ));
    let e = shell.slack_lists.comments.entry.as_ref().unwrap();
    assert_eq!(e.live.len(), 1);
    assert_eq!(e.rows.len(), 1);
    assert!(e.accepted.is_empty());
    let expected: Message = serde_json::from_value(raw(TS)).unwrap();
    assert_eq!(
        serde_json::to_value(&e.live[0]).unwrap(),
        serde_json::to_value(expected).unwrap()
    );
    assert_eq!(
        e.page.as_ref().unwrap().raw(),
        before.page.as_ref().unwrap().raw()
    );
    assert_eq!(e.page.as_ref().unwrap().cursor(), Some("next"));
    assert_eq!(e.owner.serial, before.owner.serial);
    assert_eq!(e.read, before.read);
    assert_eq!(e.cursors, before.cursors);
    assert_eq!(e.reveal, before.reveal);
    assert_eq!(shell.active_channel, selected);
    assert_eq!(shell.channel_generation, navigation);
    let rows = e.rows.clone();
    let (owner, serial, _, _) = controller::begin(&mut shell, false).unwrap();
    for conflicting in [false, true] {
        let mut value = raw(TS);
        if conflicting {
            value["text"] = json!("Must not replace");
            value["blocks"] = json!([]);
        }
        assert!(!apply(&mut shell, "T1", 7, &frame(value)));
        assert!(owner.reading(&shell, serial));
        assert!(Arc::ptr_eq(&rows, &presentation(&shell).unwrap().rows));
    }
}

#[test]
fn stale_read_cancels_but_later_omitting_pages_retain_live_and_actual_readback_deduplicates() {
    for paging in [false, true] {
        let mut shell = setup();
        let before = shell.slack_lists.comments.entry.as_ref().unwrap().clone();
        let (owner, serial, _, cursor) = controller::begin(&mut shell, paging).unwrap();
        assert!(apply(&mut shell, "T1", 7, &frame(raw(TS))));
        assert!(!owner.reading(&shell, serial));
        controller::finish(
            &mut shell,
            &owner,
            serial,
            cursor,
            Ok(network::Loaded {
                page: before.page.unwrap(),
                projection: before.projection.unwrap(),
            }),
        );
        assert_eq!(presentation(&shell).unwrap().rows[0].ts, TS);
        finish(&mut shell, json!([]), paging);
        let e = shell.slack_lists.comments.entry.as_ref().unwrap();
        assert_eq!(e.live.len(), 1);
        assert_eq!(e.rows.len(), 1);
        assert!(e.error.is_none());
        finish(&mut shell, json!([raw(TS)]), false);
        let e = shell.slack_lists.comments.entry.as_ref().unwrap();
        assert!(e.live.is_empty());
        assert_eq!(e.rows.len(), 1);
        assert_eq!(e.page.as_ref().unwrap().messages().len(), 1);
        assert!(!apply(&mut shell, "T1", 7, &frame(raw(TS))));
    }
}

#[test]
fn reaction_and_deletion_reach_live_overlay_and_stale_read_cannot_resurrect_it() {
    let mut shell = setup();
    assert!(apply(&mut shell, "T1", 7, &frame(raw(TS))));
    let reaction = frame(
        json!({"type":"reaction_added","user":"U2","reaction":"wave",
        "item":{"type":"message","channel":"CCOMMENT01","ts":TS}}),
    );
    assert!(reactions::apply(&mut shell, "T1", 7, &reaction));
    let e = shell.slack_lists.comments.entry.as_ref().unwrap();
    assert_eq!(e.live[0].reactions[0].count, 1);
    assert_eq!(e.rows[0].reactions[0].count, 1);
    let (owner, serial, _, _) = controller::begin(&mut shell, false).unwrap();
    assert!(!reactions::apply(&mut shell, "T1", 7, &reaction));
    assert!(owner.reading(&shell, serial));
    assert!(deletions::apply(
        &mut shell,
        "T1",
        7,
        &frame(json!({"type":"message","subtype":"message_deleted",
        "channel":"CCOMMENT01","deleted_ts":TS}))
    ));
    assert!(!owner.reading(&shell, serial));
    assert!(
        shell
            .slack_lists
            .comments
            .entry
            .as_ref()
            .unwrap()
            .live
            .is_empty()
    );
    assert!(!apply(&mut shell, "T1", 7, &frame(raw(TS))));
    finish(&mut shell, json!([raw(TS)]), false);
    assert!(presentation(&shell).unwrap().rows.is_empty());
}

#[test]
fn foreign_hidden_and_unavailable_owners_cannot_admit_or_cancel() {
    for mode in 0..12 {
        let mut shell = setup();
        let mut value = raw(TS);
        let (owner, serial, _, _) = controller::begin(&mut shell, false).unwrap();
        let mut team = "T1";
        let mut generation = 7;
        match mode {
            0 => team = "T2",
            1 => generation = 6,
            2 => value["channel"] = json!("OTHER"),
            3 => value["thread_ts"] = json!("1700000000.999999"),
            4 => value["ts"] = json!("bad"),
            5 => value["ts"] = json!("1700000000.000001"),
            6 => value["no_display"] = json!(true),
            7 => shell.core.account_epoch += 1,
            8 => shell.slack_lists.comments.open = false,
            9 => {
                shell
                    .slack_lists
                    .comments
                    .entry
                    .as_mut()
                    .unwrap()
                    .projection = None
            }
            10 => {
                value["subtype"] = json!("list_edit");
                value["list_edit"] = json!({});
                shell.slack_lists.comments.entry.as_mut().unwrap().projection=Some(Projection::observed(json!({"ok":true,
                    "assignments":{"lists_pilot":{"group":"on"},"lists_pilot_teams":{"group":"on"}}})).unwrap());
            }
            _ => {
                shell
                    .slack_lists
                    .comments
                    .entry
                    .as_mut()
                    .unwrap()
                    .deleted
                    .insert(TS.into());
            }
        }
        assert!(!apply(&mut shell, team, generation, &frame(value)));
        let e = shell.slack_lists.comments.entry.as_ref().unwrap();
        assert!(e.live.is_empty() && e.rows.is_empty());
        assert_eq!(e.read, serial);
        assert!(e.loading);
        if !matches!(mode, 7 | 8) {
            assert!(owner.reading(&shell, serial));
        }
    }
}

#[test]
fn arrival_keeps_send_preflight_and_exact_durable_draft_current() {
    let (mut shell, binding, _local) = draft::tests::setup();
    install(&mut shell);
    let fields = |shell: &ShellState| {
        let e = draft::entry(shell, binding.id()).unwrap();
        json!({"id":e.id,"blocks":e.blocks,"sequence":e.sequence,"capture":e.capture,"generation":e.generation,
            "nativeSequence":e.native_sequence,"busy":e.busy,"saving":e.saving,"loading":e.loading,
            "composing":e.composing,"error":e.error,"status":e.status,
            "inventory":e.inventory.as_ref().map(|i|(i.blocks(),i.revision(),i.attempt()))})
    };
    let before = fields(&shell);
    let (owner, serial, _, _) = controller::begin_preflight(&mut shell).unwrap();
    assert!(apply(&mut shell, "T1", 7, &frame(raw(TS))));
    assert!(owner.reading(&shell, serial));
    assert!(binding.current(&shell));
    assert_eq!(fields(&shell), before);
    assert!(presentation(&shell).unwrap().error.is_none());
}

#[test]
fn bounded_overlay_retains_existing_rows_and_explicit_readback_frees_capacity() {
    let mut shell = setup();
    for i in 0..MAX_LIVE {
        assert!(apply(
            &mut shell,
            "T1",
            7,
            &frame(raw(&format!("1700000001.{i:06}")))
        ));
    }
    let rows = presentation(&shell).unwrap().rows;
    let before = shell.slack_lists.comments.entry.as_ref().unwrap().clone();
    let (owner, serial, _, cursor) = controller::begin(&mut shell, false).unwrap();
    assert!(!apply(
        &mut shell,
        "T1",
        7,
        &frame(raw("1700000002.000001"))
    ));
    assert!(Arc::ptr_eq(&rows, &presentation(&shell).unwrap().rows));
    assert_eq!(
        shell
            .slack_lists
            .comments
            .entry
            .as_ref()
            .unwrap()
            .live
            .len(),
        MAX_LIVE
    );
    assert!(!owner.reading(&shell, serial));
    controller::finish(
        &mut shell,
        &owner,
        serial,
        cursor,
        Ok(network::Loaded {
            page: before.page.unwrap(),
            projection: before.projection.unwrap(),
        }),
    );
    assert!(
        presentation(&shell)
            .unwrap()
            .error
            .unwrap()
            .contains("Refresh")
    );
    let messages =
        serde_json::to_value(&shell.slack_lists.comments.entry.as_ref().unwrap().live).unwrap();
    finish(&mut shell, messages, false);
    assert!(
        shell
            .slack_lists
            .comments
            .entry
            .as_ref()
            .unwrap()
            .live
            .is_empty()
    );
    assert!(apply(&mut shell, "T1", 7, &frame(raw("1700000002.000001"))));
    assert_eq!(presentation(&shell).unwrap().rows.len(), MAX_LIVE + 1);
}

#[test]
fn closing_and_reopening_does_not_transfer_live_overlay_to_successor() {
    let mut shell = setup();
    assert!(apply(&mut shell, "T1", 7, &frame(raw(TS))));
    controller::close_shell(&mut shell);
    let generation = record_detail::presentation(shell.slack_lists.view.as_ref().unwrap())
        .unwrap()
        .generation;
    assert!(controller::open_shell(&mut shell, generation));
    assert!(
        shell
            .slack_lists
            .comments
            .entry
            .as_ref()
            .unwrap()
            .live
            .is_empty()
    );
    assert!(!apply(&mut shell, "T1", 7, &frame(raw(TS)))); // no acquired successor Page
}
