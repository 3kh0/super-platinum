use super::*;
use serde_json::json;

const TS: &str = "1700000001.000001";
fn load(shell: &mut ShellState, tail: bool) {
    shell.core.workspaces.get_mut("T1").unwrap().rt_generation = 7;
    let (owner, serial, target, cursor) = controller::begin(shell, false).unwrap();
    let message = json!({"type":"message","ts":TS,"thread_ts":target.root_ts(),
        "user":"U1","text":"Delete this reply","reactions":[{"name":"wave","count":2,"users":["U1","U2"]}]});
    let page = Page::parse(
        &target,
        json!({"ok":true,
        "messages":if tail {json!([])} else {json!([message.clone()])},
        "has_more":true,"response_metadata":{"next_cursor":"next"}}),
    )
    .unwrap();
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
    if tail {
        let entry = shell.slack_lists.comments.entry.as_ref().unwrap();
        let accepted = vec![serde_json::from_value(message).unwrap()];
        let rows = accepted::rows(
            shell,
            &owner,
            entry.page.as_ref().unwrap(),
            entry.projection.as_ref().unwrap(),
            &accepted,
        )
        .unwrap();
        let entry = shell.slack_lists.comments.entry.as_mut().unwrap();
        entry.accepted = accepted;
        entry.rows = rows;
        entry.reveal = Some((1, TS.into()));
    }
}
fn setup(tail: bool) -> ShellState {
    let mut shell = super::super::tests::shell();
    load(&mut shell, tail);
    shell
}
fn event(channel: &str, ts: &str) -> RtEvent {
    super_platinum_core::slack::realtime::parse_event(
        &json!({
            "type":"message","subtype":"message_deleted","channel":channel,"deleted_ts":ts
        })
        .to_string(),
    )
    .unwrap()
}

#[test]
fn parsed_deletion_removes_page_and_accepted_tail_without_replacing_visit_or_pagination() {
    for tail in [false, true] {
        let mut shell = setup(tail);
        let before = shell.slack_lists.comments.entry.as_ref().unwrap().clone();
        let selected = shell.active_channel;
        let navigation = shell.channel_generation;
        assert!(!crate::slack_lists::events::prepare(
            &mut shell,
            "T1",
            7,
            &event("CCOMMENT01", TS)
        ));
        let e = shell.slack_lists.comments.entry.as_ref().unwrap();
        assert!(e.rows.is_empty() && e.accepted.is_empty());
        assert!(
            !e.page
                .as_ref()
                .unwrap()
                .messages()
                .iter()
                .any(|m| m.ts.as_deref() == Some(TS))
        );
        assert_eq!(
            e.page.as_ref().unwrap().raw(),
            before.page.as_ref().unwrap().raw()
        );
        assert_eq!(e.page.as_ref().unwrap().cursor(), Some("next"));
        assert_eq!(e.cursors, before.cursors);
        assert_eq!(e.read, before.read);
        assert_eq!(e.owner.serial, before.owner.serial);
        assert!(e.deleted.contains(TS));
        assert!(e.reveal.is_none());
        assert_eq!(shell.active_channel, selected);
        assert_eq!(shell.channel_generation, navigation);
        let rows = e.rows.clone();
        assert!(!apply(&mut shell, "T1", 7, &event("CCOMMENT01", TS)));
        assert!(Arc::ptr_eq(&rows, &presentation(&shell).unwrap().rows));
    }
}

#[test]
fn old_callbacks_and_later_stale_pages_cannot_resurrect_a_deleted_reply() {
    for tail in [false, true] {
        for paging in [false, true] {
            let mut shell = setup(tail);
            let original = shell.slack_lists.comments.entry.as_ref().unwrap().clone();
            let (owner, serial, target, cursor) = controller::begin(&mut shell, paging).unwrap();
            assert!(apply(&mut shell, "T1", 7, &event("CCOMMENT01", TS)));
            assert!(!owner.reading(&shell, serial));
            let stale = || network::Loaded {
                page: Page::parse(
                    &target,
                    json!({"ok":true,"messages":[{
                    "type":"message","ts":TS,"thread_ts":target.root_ts(),"text":"Stale reply"}],
                    "has_more":true,"response_metadata":{"next_cursor":"next"}}),
                )
                .unwrap(),
                projection: original.projection.clone().unwrap(),
            };
            controller::finish(&mut shell, &owner, serial, cursor, Ok(stale()));
            assert!(presentation(&shell).unwrap().rows.is_empty());
            let (new_owner, new_serial, _, new_cursor) =
                controller::begin(&mut shell, paging).unwrap();
            controller::finish(
                &mut shell,
                &owner,
                serial,
                None,
                Err(Error::Offline("old".into())),
            );
            assert!(new_owner.reading(&shell, new_serial));
            controller::finish(&mut shell, &new_owner, new_serial, new_cursor, Ok(stale()));
            let e = shell.slack_lists.comments.entry.as_ref().unwrap();
            assert!(e.rows.is_empty() && e.accepted.is_empty());
            assert!(e.page.as_ref().unwrap().messages().is_empty());
            assert!(e.deleted.contains(TS));
            assert!(!e.loading && e.error.is_none());
        }
    }
}

#[test]
fn foreign_unknown_root_closed_and_replaced_owner_events_leave_reads_intact() {
    for mode in 0..9 {
        let mut shell = setup(false);
        let (owner, serial, target, _) = controller::begin(&mut shell, false).unwrap();
        let before = shell.slack_lists.comments.entry.as_ref().unwrap().clone();
        let (team, generation, channel, ts) = match mode {
            0 => ("T2", 7, "CCOMMENT01", TS),
            1 => ("T1", 6, "CCOMMENT01", TS),
            2 => ("T1", 7, "COTHER", TS),
            3 => ("T1", 7, "CCOMMENT01", "unknown"),
            4 => ("T1", 7, "CCOMMENT01", target.root_ts().unwrap()),
            5 => ("T1", 7, "CCOMMENT01", ""),
            _ => ("T1", 7, "CCOMMENT01", TS),
        };
        if mode == 6 {
            shell.core.account_epoch += 1;
        }
        if mode == 7 {
            shell.slack_lists.comments.open = false;
        }
        if mode == 8 {
            shell.slack_lists.comments.entry.as_mut().unwrap().rows = Arc::new(Vec::new());
        }
        assert!(!apply(&mut shell, team, generation, &event(channel, ts)));
        let e = shell.slack_lists.comments.entry.as_ref().unwrap();
        assert_eq!(e.read, serial);
        assert!(e.loading && e.deleted.is_empty());
        assert_eq!(
            e.page.as_ref().unwrap().raw(),
            before.page.as_ref().unwrap().raw()
        );
        if mode < 6 {
            assert!(owner.reading(&shell, serial));
        }
    }
}

#[test]
fn deleting_a_comment_preserves_send_preflight_and_exact_durable_draft() {
    let (mut shell, binding, _local) = draft::tests::setup();
    load(&mut shell, false);
    let fields = |shell: &ShellState| {
        let e = draft::entry(shell, binding.id()).unwrap();
        json!({"id":e.id,"blocks":e.blocks,"sequence":e.sequence,"capture":e.capture,
            "generation":e.generation,"nativeSequence":e.native_sequence,"busy":e.busy,
            "saving":e.saving,"loading":e.loading,"composing":e.composing,"error":e.error,
            "status":e.status,"inventory":e.inventory.as_ref().map(|i|(i.blocks(),i.revision(),i.attempt()))})
    };
    let before = fields(&shell);
    let (owner, serial, _, _) = controller::begin_preflight(&mut shell).unwrap();
    assert!(apply(&mut shell, "T1", 7, &event("CCOMMENT01", TS)));
    assert!(owner.reading(&shell, serial));
    assert!(binding.current(&shell));
    assert_eq!(fields(&shell), before);
    assert!(presentation(&shell).unwrap().rows.is_empty());
    assert!(presentation(&shell).unwrap().error.is_none());
}
