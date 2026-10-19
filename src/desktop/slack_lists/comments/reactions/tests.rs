use super::*;
use serde_json::json;

const TS: &str = "1700000001.000001";
fn setup(tail: bool) -> ShellState {
    let mut shell = super::super::tests::shell();
    shell.core.workspaces.get_mut("T1").unwrap().rt_generation = 7;
    let (owner, serial, target, cursor) = controller::begin(&mut shell, false).unwrap();
    let message = json!({"type":"message","ts":TS,"thread_ts":target.root_ts(),"user":"U1",
        "text":"Keep this reply","reactions":[{"name":"wave","count":2,"users":["U1","U2"]}]});
    let page = Page::parse(
        &target,
        json!({"ok":true,"messages":if tail {json!([])} else {json!([message.clone()])},
        "has_more":true,"response_metadata":{"next_cursor":"next"}}),
    )
    .unwrap();
    controller::finish(
        &mut shell,
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
        let messages = vec![serde_json::from_value(message).unwrap()];
        let rows = accepted::rows(
            &shell,
            &owner,
            entry.page.as_ref().unwrap(),
            entry.projection.as_ref().unwrap(),
            &messages,
        )
        .unwrap();
        let entry = shell.slack_lists.comments.entry.as_mut().unwrap();
        entry.accepted = messages;
        entry.rows = rows;
        entry.reveal = Some((1, TS.into()));
    }
    shell
}
fn event(channel: &str, ts: &str, added: bool) -> RtEvent {
    super_platinum_core::slack::realtime::parse_event(&json!({
        "type":if added {"reaction_added"}else{"reaction_removed"},"user":"U3","reaction":"wave",
        "item":{"type":"message","channel":channel,"ts":ts}
    }).to_string()).unwrap()
}
fn count(shell: &ShellState) -> u32 {
    presentation(shell).unwrap().rows[0].reactions[0].count
}
#[test]
fn loaded_and_accepted_rows_apply_idempotent_events_without_navigation_or_read_changes() {
    for tail in [false, true] {
        let mut shell = setup(tail);
        let previous = shell.slack_lists.comments.entry.as_ref().unwrap().clone();
        let selected = shell.active_channel;
        let navigation = shell.channel_generation;
        assert!(apply(&mut shell, "T1", 7, &event("CCOMMENT01", TS, true)));
        assert_eq!(count(&shell), 3);
        let rows = presentation(&shell).unwrap().rows;
        assert!(!apply(&mut shell, "T1", 7, &event("CCOMMENT01", TS, true)));
        assert!(Arc::ptr_eq(&rows, &presentation(&shell).unwrap().rows));
        // The production ingress must not turn a reaction into a List read.
        assert!(!crate::slack_lists::events::prepare(
            &mut shell,
            "T1",
            7,
            &event("CCOMMENT01", TS, false)
        ));
        assert_eq!(count(&shell), 2);
        let entry = shell.slack_lists.comments.entry.as_ref().unwrap();
        assert_eq!(
            entry.page.as_ref().unwrap().raw(),
            previous.page.as_ref().unwrap().raw()
        );
        assert_eq!(entry.page.as_ref().unwrap().cursor(), Some("next"));
        assert_eq!(entry.cursors, previous.cursors);
        assert_eq!(entry.owner.serial, previous.owner.serial);
        assert_eq!(entry.read, previous.read);
        assert_eq!(entry.loading, previous.loading);
        assert_eq!(entry.reveal, previous.reveal);
        assert_eq!(shell.active_channel, selected);
        assert_eq!(shell.channel_generation, navigation);
    }
}
#[test]
fn foreign_socket_team_channel_timestamp_and_missing_user_are_ignored() {
    let mut shell = setup(false);
    let rows = presentation(&shell).unwrap().rows;
    for (team, generation, channel, ts) in [
        ("T2", 7, "CCOMMENT01", TS),
        ("T1", 6, "CCOMMENT01", TS),
        ("T1", 8, "CCOMMENT01", TS),
        ("T1", 7, "COTHER", TS),
        ("T1", 7, "CCOMMENT01", "1700000999.000001"),
    ] {
        assert!(!apply(
            &mut shell,
            team,
            generation,
            &event(channel, ts, true)
        ));
        assert!(Arc::ptr_eq(&rows, &presentation(&shell).unwrap().rows));
    }
    let mut invalid = event("CCOMMENT01", TS, true);
    if let RtEvent::ReactionAdded { user, .. } = &mut invalid {
        user.clear();
    }
    assert!(!apply(&mut shell, "T1", 7, &invalid));
    assert!(Arc::ptr_eq(&rows, &presentation(&shell).unwrap().rows));
}
#[test]
fn account_replacement_and_closed_reader_cannot_receive_reactions() {
    for close in [false, true] {
        let mut shell = setup(true);
        let rows = shell
            .slack_lists
            .comments
            .entry
            .as_ref()
            .unwrap()
            .rows
            .clone();
        if close {
            controller::close_shell(&mut shell);
        } else {
            shell.core.account_epoch += 1;
        }
        assert!(!apply(&mut shell, "T1", 7, &event("CCOMMENT01", TS, true)));
        assert!(Arc::ptr_eq(
            &rows,
            &shell.slack_lists.comments.entry.as_ref().unwrap().rows
        ));
    }
}
#[test]
fn rows_excluded_by_the_observed_projection_are_untouched() {
    let mut shell = setup(false);
    shell.slack_lists.comments.entry.as_mut().unwrap().rows = Arc::new(Vec::new());
    assert!(!apply(&mut shell, "T1", 7, &event("CCOMMENT01", TS, true)));
    assert_eq!(
        shell
            .slack_lists
            .comments
            .entry
            .as_ref()
            .unwrap()
            .page
            .as_ref()
            .unwrap()
            .messages()[0]
            .reactions[0]
            .count,
        2
    );
}

#[test]
fn live_delta_retires_older_refresh_and_paging_without_losing_rows_or_cursor() {
    for tail in [false, true] {
        for paging in [false, true] {
            let mut shell = setup(tail);
            let original = shell.slack_lists.comments.entry.as_ref().unwrap().clone();
            let (owner, serial, _, cursor) = controller::begin(&mut shell, paging).unwrap();
            assert!(owner.reading(&shell, serial));
            assert!(apply(&mut shell, "T1", 7, &event("CCOMMENT01", TS, true)));
            assert!(!owner.reading(&shell, serial));
            assert_eq!(count(&shell), 3);
            let rows = presentation(&shell).unwrap().rows;
            let retained = shell.slack_lists.comments.entry.as_ref().unwrap();
            assert!(!retained.loading);
            assert_eq!(retained.owner.serial, original.owner.serial);
            assert_eq!(retained.page.as_ref().unwrap().cursor(), Some("next"));
            assert_eq!(retained.cursors, original.cursors);
            assert_eq!(retained.reveal, original.reveal);
            assert!(
                retained
                    .error
                    .as_deref()
                    .unwrap()
                    .contains("Newer comment updates")
            );
            // The original raw response would erase the newer pill if it published.
            controller::finish(
                &mut shell,
                &owner,
                serial,
                cursor.clone(),
                Ok(network::Loaded {
                    page: original.page.clone().unwrap(),
                    projection: original.projection.clone().unwrap(),
                }),
            );
            assert!(Arc::ptr_eq(&rows, &presentation(&shell).unwrap().rows));
            assert_eq!(count(&shell), 3);
            // Old success/error callbacks cannot terminate a later explicit retry.
            let (new_owner, new_serial, _, _) = controller::begin(&mut shell, false).unwrap();
            controller::finish(
                &mut shell,
                &owner,
                serial,
                cursor,
                Err(Error::Offline("old".into())),
            );
            assert!(new_owner.reading(&shell, new_serial));
            assert_eq!(count(&shell), 3);
        }
    }
}

#[test]
fn duplicate_foreign_and_noop_removal_do_not_cancel_a_valid_read() {
    let mut shell = setup(false);
    let (owner, serial, _, _) = controller::begin(&mut shell, false).unwrap();
    let mut duplicate = event("CCOMMENT01", TS, true);
    if let RtEvent::ReactionAdded { user, .. } = &mut duplicate {
        *user = "U1".into();
    }
    for (team, generation, frame) in [
        ("T1", 7, duplicate),
        ("T2", 7, event("CCOMMENT01", TS, true)),
        ("T1", 6, event("CCOMMENT01", TS, true)),
        ("T1", 7, event("COTHER", TS, true)),
        ("T1", 7, event("CCOMMENT01", "1700000999.000001", true)),
        // The shared reducer returns true for this unchanged removal; compare
        // actual reaction content before deciding to retire the pending read.
        ("T1", 7, event("CCOMMENT01", TS, false)),
    ] {
        assert!(!apply(&mut shell, team, generation, &frame));
        assert!(owner.reading(&shell, serial));
        assert_eq!(count(&shell), 2);
        assert!(presentation(&shell).unwrap().error.is_none());
    }
}

#[test]
fn cancelled_refresh_preserves_exact_durable_draft_binding_and_state() {
    let (mut shell, binding, _local) = draft::tests::setup();
    shell.core.workspaces.get_mut("T1").unwrap().rt_generation = 7;
    let (owner, serial, target, cursor) = controller::begin(&mut shell, false).unwrap();
    controller::finish(
        &mut shell,
        &owner,
        serial,
        cursor,
        Ok(network::Loaded {
            page: Page::parse(
                &target,
                json!({"ok":true,"messages":[{"type":"message","ts":TS,
            "thread_ts":target.root_ts(),"user":"U1","text":"Reply"}],"has_more":false}),
            )
            .unwrap(),
            projection: Projection::observed(json!({"ok":true,"assignments":{}})).unwrap(),
        }),
    );
    let fields = |shell: &ShellState| {
        let e = draft::entry(shell, binding.id()).unwrap();
        json!({"id":e.id,"blocks":e.blocks,"sequence":e.sequence,"capture":e.capture,
            "generation":e.generation,"nativeSequence":e.native_sequence,"busy":e.busy,
            "saving":e.saving,"loading":e.loading,"composing":e.composing,"error":e.error,
            "status":e.status,"inventory":e.inventory.as_ref().map(|i| (i.blocks(),i.revision(),i.attempt()))})
    };
    let before = fields(&shell);
    let (owner, serial, _, _) = controller::begin(&mut shell, false).unwrap();
    assert!(apply(&mut shell, "T1", 7, &event("CCOMMENT01", TS, true)));
    assert!(!owner.reading(&shell, serial));
    assert!(binding.current(&shell));
    assert_eq!(fields(&shell), before);
}

#[test]
fn reaction_does_not_cancel_send_preflight_which_never_replaces_visible_rows() {
    let mut shell = setup(false);
    let (owner, serial, _, _) = controller::begin_preflight(&mut shell).unwrap();
    assert!(apply(&mut shell, "T1", 7, &event("CCOMMENT01", TS, true)));
    assert!(owner.reading(&shell, serial));
    assert_eq!(count(&shell), 3);
    assert!(presentation(&shell).unwrap().error.is_none());
    // After preflight is finished, the next explicit reader regains publication
    // authority and must again be retired by a newer delta.
    shell.slack_lists.comments.entry.as_mut().unwrap().loading = false;
    let (owner, serial, _, _) = controller::begin(&mut shell, false).unwrap();
    assert!(apply(&mut shell, "T1", 7, &event("CCOMMENT01", TS, false)));
    assert!(!owner.reading(&shell, serial));
    assert_eq!(count(&shell), 2);
}
