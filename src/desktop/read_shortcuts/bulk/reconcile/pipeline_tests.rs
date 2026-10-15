//! Full async production pipeline with only HTTP execution substituted.
use super::{
    ShellState,
    fixture::{
        CHANNEL, LATEST, METHODS, REPLY, ROOT, Script, TEAM, clear, failed, harness, key, retry,
        run, undo,
    },
};
use dioxus::prelude::*;
#[path = "pipeline_races.rs"]
mod races;
#[path = "subscription_tests.rs"]
mod subscription_tests;

#[tokio::test(flavor = "current_thread")]
async fn clear_then_undo_refreshes_all_five_surfaces_with_authoritative_cursors() {
    let (_dom, state) = harness();
    let clear = clear(state);
    let script = Script::new(&state.peek(), true);
    run(state, &clear, &script).await;
    script.assert_methods(&METHODS);
    assert!(!failed(state, &clear));
    assert_read_state(state, true);
    assert_wire(&script);
    let undo = undo(state);
    let script = Script::new(&state.peek(), false);
    run(state, &undo, &script).await;
    script.assert_methods(&METHODS);
    assert!(!failed(state, &undo));
    assert_read_state(state, false);
    assert_wire(&script);
    assert!(
        state.peek().bulk_read.entries[&undo.context.owner]
            .notice
            .as_ref()
            .unwrap()
            .receipt
            .is_none()
    );
}

fn assert_read_state(state: Signal<ShellState>, cleared: bool) {
    let s = state.peek();
    let w = &s.core.workspaces[TEAM];
    let cursor = if cleared { LATEST } else { ROOT };
    assert_eq!(w.read_cursor(CHANNEL), Some(cursor));
    assert_eq!(
        w.messages[CHANNEL].unread_count,
        if cleared { 0 } else { 2 }
    );
    assert_eq!(w.activity_unread_count, Some(if cleared { 0 } else { 2 }));
    let totals = w.thread_counts.snapshot.as_ref().unwrap();
    assert_eq!(totals.has_unreads, !cleared);
    assert_eq!(totals.mention_count, if cleared { 0 } else { 1 });
    // One fetched Activity row is not a substitute for the authoritative total2.
    assert_eq!(s.core.activity.items.len(), 1);
    assert_eq!(s.core.activity.items[0].is_unread, !cleared);
    assert_eq!(
        s.core.activity.next_cursor.as_deref(),
        Some("next-activity")
    );
    assert!(!s.core.activity.loading);
    assert_eq!(s.core.thread_cursor(&key()), Some(cursor));
    assert_eq!(
        s.core.threads[&key()].unread_count,
        if cleared { 0 } else { 2 }
    );
    assert!(s.core.threads[&key()].has_more_older);
    assert!(s.core.threads[&key()].was_deleted("1719800403.000000"));
    assert!(
        !s.core.threads[&key()]
            .messages
            .iter()
            .any(|m| m.ts.as_deref() == Some("1719800403.000000"))
    );
    assert!(s.core.threads[&key()].messages.iter().any(
        |m| m.ts.as_deref() == Some(REPLY) && m.text.as_deref() == Some("Authoritative reply")
    ));
    assert!(w.messages[CHANNEL].messages.iter().any(
        |m| m.ts.as_deref() == Some(ROOT) && m.text.as_deref() == Some("Authoritative parent")
    ));
    assert!(!s.core.threads_view.loading);
    assert!(s.core.threads_view.has_more);
    assert!(s.core.session.is_none() && s.core.transport.is_none() && s.core.cache.is_none());
}

fn assert_wire(script: &Script) {
    let ledger = script.ledger.borrow();
    let fields = |index: usize| {
        serde_json::from_value::<Vec<(String, String)>>(ledger[index]["fields"].clone()).unwrap()
    };
    assert!(fields(0).is_empty());
    assert_eq!(
        fields(1),
        vec![
            ("limit".into(), "30".into()),
            ("fetch_threads_state".into(), "true".into()),
            ("priority_mode".into(), "all".into())
        ]
    );
    let activity = fields(2);
    assert_eq!(
        activity.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>(),
        [
            "limit",
            "types",
            "mode",
            "archive_only",
            "unread_only",
            "priority_only",
            "only_salesforce_channels",
            "exclude_automations",
            "automations_only",
            "is_activity_inbox"
        ]
    );
    assert_eq!(activity[0].1, "20");
    assert_eq!(activity[2].1, "chrono_v1");
    assert!(activity[3..9].iter().all(|(_, v)| v == "false"));
    assert_eq!(activity[9].1, "true");
    assert!(activity[1].1.split(',').any(|kind| kind == "thread_v2"));
    assert_eq!(
        fields(3),
        vec![
            ("channel".into(), CHANNEL.into()),
            ("limit".into(), "100".into())
        ]
    );
    assert_eq!(
        fields(4),
        vec![
            ("channel".into(), CHANNEL.into()),
            ("ts".into(), ROOT.into()),
            ("limit".into(), "100".into())
        ]
    );
    assert!(
        !serde_json::to_string(&*ledger)
            .unwrap()
            .contains("fixture-not-a-token")
    );
}

#[tokio::test(flavor = "current_thread")]
async fn hidden_transcript_and_closed_thread_skip_only_their_reads() {
    for visible in [false, true] {
        let (_dom, mut state) = harness();
        state.write().thread_root = None;
        if !visible {
            state.write().main_view = crate::state::MainView::Threads;
        }
        let work = clear(state);
        let script = Script::new(&state.peek(), true);
        run(state, &work, &script).await;
        script.assert_methods(&METHODS[..if visible { 4 } else { 3 }]);
        assert!(!failed(state, &work));
    }
}

#[tokio::test(flavor = "current_thread")]
async fn errors_at_each_stage_stop_the_tail_and_allow_explicit_retry() {
    for index in 0..METHODS.len() {
        let (_dom, state) = harness();
        let work = clear(state);
        let script = Script::new(&state.peek(), true);
        script.step(index, |step| step.error());
        run(state, &work, &script).await;
        script.assert_methods(&METHODS[..=index]);
        assert!(failed(state, &work));
        assert!(!state.peek().core.threads_view.loading);
        assert!(!state.peek().core.activity.loading);
        if index > 0 {
            assert_eq!(
                state.peek().core.workspaces[TEAM].read_cursor(CHANNEL),
                Some(LATEST)
            );
        }
        let retry = retry(state);
        let script = Script::new(&state.peek(), true);
        run(state, &retry, &script).await;
        script.assert_methods(&METHODS);
        assert!(!failed(state, &retry));
    }
}

#[tokio::test(flavor = "current_thread")]
async fn malformed_page_uses_shared_decode_failure_and_stops_the_tail() {
    for index in 0..METHODS.len() {
        let (_dom, state) = harness();
        let work = clear(state);
        let script = Script::new(&state.peek(), true);
        script.step(index, |step| step.malformed());
        run(state, &work, &script).await;
        script.assert_methods(&METHODS[..=index]);
        assert!(failed(state, &work));
        assert!(!state.peek().core.threads_view.loading);
        assert!(!state.peek().core.activity.loading);
    }
}

#[tokio::test(flavor = "current_thread")]
async fn stale_admission_makes_no_requests() {
    let (_dom, mut state) = harness();
    let work = clear(state);
    state.write().channel_generation += 1;
    let script = Script::new(&state.peek(), true);
    run(state, &work, &script).await;
    assert_eq!(script.count(), 0);
    assert!(failed(state, &work));
}

#[tokio::test(flavor = "current_thread")]
async fn successful_retry_replaces_old_failure_copy_for_clear_undo_and_expired_notice() {
    for (undone, retain_notice) in [(false, true), (true, true), (false, false)] {
        let (_dom, mut state) = harness();
        let initial = clear(state);
        let work = if undone { undo(state) } else { initial };
        let script = Script::new(&state.peek(), !undone);
        script.step(4, |step| step.error());
        run(state, &work, &script).await;
        assert!(failed(state, &work));
        assert!(
            state.peek().bulk_read.entries[&work.context.owner]
                .message
                .as_deref()
                .unwrap()
                .contains("current read state could not be refreshed")
        );
        if !retain_notice {
            state
                .write()
                .bulk_read
                .entries
                .get_mut(&work.context.owner)
                .unwrap()
                .notice = None;
        }
        let retry = retry(state);
        let script = Script::new(&state.peek(), !undone);
        run(state, &retry, &script).await;
        script.assert_methods(&METHODS);
        assert!(!failed(state, &retry));
        let s = state.peek();
        let entry = &s.bulk_read.entries[&work.context.owner];
        assert_eq!(
            entry.message.as_deref(),
            if !retain_notice {
                None
            } else if undone {
                Some("Unread state restored.")
            } else {
                Some("Unread messages and notifications cleared.")
            }
        );
        assert!(entry.reconciling.is_none());
        assert_eq!(entry.notice.is_some(), retain_notice);
    }
}
