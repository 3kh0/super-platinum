use super::super::{
    Context, Snapshot,
    fixture::{
        CHANNEL, LATEST, METHODS, REPLY, ROOT, Run, Script, TEAM, clear, failed, harness, key,
        retry, run, shell, undo,
    },
};
use super::assert_read_state;
use dioxus::prelude::*;
use serde_json::json;
use std::sync::Arc;

#[tokio::test(flavor = "current_thread")]
async fn arrival_before_bulk_ack_invalidates_pre_write_baseline_without_any_read() {
    let (_dom, mut state) = harness();
    let (context, baseline, id) = {
        let mut s = state.write();
        let context = Arc::new(Context::capture(&s).unwrap());
        let work = super::super::super::lifecycle::start_clear(&mut s, context, true).unwrap();
        let bag = s
            .core
            .workspaces
            .get_mut(TEAM)
            .unwrap()
            .messages
            .get_mut(CHANNEL)
            .unwrap();
        bag.upsert(serde_json::from_value(json!({"channel":CHANNEL,"ts":"1719800600.000000","text":"Arrival during bulk write"})).unwrap());
        bag.unread_count += 1;
        let id = super::super::super::finish_write(&mut s, &work, Ok(json!({"ok":true}))).unwrap();
        (work.context, work.baseline, id)
    };
    let work = Run {
        context,
        baseline,
        id,
    };
    let script = Script::new(&state.peek(), true);
    run(state, &work, &script).await;
    assert_eq!(script.count(), 0);
    assert!(failed(state, &work));
    assert!(
        state.peek().core.workspaces[TEAM].messages[CHANNEL]
            .messages
            .iter()
            .any(|m| m.text.as_deref() == Some("Arrival during bulk write"))
    );
}

#[tokio::test(flavor = "current_thread")]
async fn ownership_visit_and_generation_changes_during_reads_reject_old_responses() {
    for index in 0..METHODS.len() {
        for change in 0..5 {
            let (_dom, mut state) = harness();
            let work = clear(state);
            let script = Script::new(&state.peek(), true);
            let observed = std::rc::Rc::new(std::cell::RefCell::new(None));
            let after_change = observed.clone();
            script.step(index, |step| {
                step.during(move || {
                    let mut s = state.write();
                    match change {
                        0 => s.core.account_epoch += 1,
                        1 => s.core.workspaces.get_mut(TEAM).unwrap().rt_generation += 1,
                        2 => s.channel_generation += 1,
                        3 => s
                            .bulk_read
                            .entries
                            .values_mut()
                            .for_each(|e| e.reconciling = None),
                        _ => {
                            let mut replacement = shell();
                            replacement.core.active_account = Some("another-isolated-owner".into());
                            replacement.core.account_epoch = s.core.account_epoch + 1;
                            crate::read_preferences::settings::carry_pending(
                                &mut s,
                                &mut replacement,
                            );
                            *s = replacement;
                        }
                    }
                    *after_change.borrow_mut() = Some(Snapshot::capture(&s, TEAM));
                })
            });
            run(state, &work, &script).await;
            script.assert_methods(&METHODS[..=index]);
            assert!(!state.peek().core.threads_view.loading);
            assert!(!state.peek().core.activity.loading);
            // The held page is never projected after its authority changed.
            assert!(observed.borrow().as_ref() == Some(&Snapshot::capture(&state.peek(), TEAM)));
        }
    }
}

#[tokio::test(flavor = "current_thread")]
async fn cursor_aba_explicit_unread_message_edit_and_activity_arrival_win_during_await() {
    for index in 0..METHODS.len() {
        let (_dom, mut state) = harness();
        let work = clear(state);
        let script = Script::new(&state.peek(), true);
        script.step(index,|step|step.during(move||{
            let mut s=state.write();
            match index {
                0=>{s.core.workspaces.get_mut(TEAM).unwrap().messages.get_mut(CHANNEL).unwrap().read_revision+=2;}
                1=>s.core.apply_thread_mark(&key(),ROOT),
                2=>s.core.activity.upsert(serde_json::from_value(json!({"key":"new-live-item","feed_ts":"1719800500.000000","is_unread":true,"item":{"type":"at_user","message":{"channel":CHANNEL,"ts":"1719800500.000000"}}})).unwrap()),
                3=>{s.core.workspaces.get_mut(TEAM).unwrap().messages.get_mut(CHANNEL).unwrap().upsert(serde_json::from_value(json!({"channel":CHANNEL,"ts":ROOT,"text":"Newer live edit"})).unwrap());}
                _=>{s.core.threads.get_mut(&key()).unwrap().upsert(serde_json::from_value(json!({"channel":CHANNEL,"ts":REPLY,"thread_ts":ROOT,"text":"Newer thread edit"})).unwrap());}
            }
        }));
        run(state, &work, &script).await;
        script.assert_methods(&METHODS[..=index]);
        assert!(failed(state, &work));
        let s = state.peek();
        match index {
            1 => assert_eq!(s.core.thread_cursor(&key()), Some(ROOT)),
            2 => assert!(
                s.core
                    .activity
                    .items
                    .iter()
                    .any(|item| item.key == "new-live-item" && item.is_unread)
            ),
            3 => assert!(
                s.core.workspaces[TEAM].messages[CHANNEL]
                    .messages
                    .iter()
                    .any(|m| m.text.as_deref() == Some("Newer live edit"))
            ),
            4 => assert!(
                s.core.threads[&key()]
                    .messages
                    .iter()
                    .any(|m| m.text.as_deref() == Some("Newer thread edit"))
            ),
            _ => {}
        }
    }
}

#[tokio::test(flavor = "current_thread")]
async fn outstanding_individual_read_blocks_a_late_bulk_page() {
    let (_dom, mut state) = harness();
    let work = clear(state);
    let script = Script::new(&state.peek(), true);
    script.step(3, |step| {
        step.during(move || {
            state.write().core.pending_marks.insert((
                super_platinum_core::domain::ReadTarget::Conversation {
                    team: TEAM.into(),
                    channel: CHANNEL.into(),
                },
                LATEST.into(),
            ));
        })
    });
    run(state, &work, &script).await;
    script.assert_methods(&METHODS[..4]);
    assert!(failed(state, &work));
    assert_eq!(state.peek().core.pending_marks.len(), 1);
}

#[tokio::test(flavor = "current_thread")]
async fn undo_retires_waiting_feed_then_old_cleanup_allows_explicit_retry() {
    for index in [1, 2] {
        let (_dom, state) = harness();
        let work = clear(state);
        let old = Script::new(&state.peek(), true);
        let mut pause = None;
        old.step(index, |step| pause = Some(step.pause()));
        let (started, release) = pause.unwrap();
        tokio::join!(run(state, &work, &old), async {
            started.await.unwrap();
            let undo = undo(state);
            let restore = Script::new(&state.peek(), false);
            // The old request still owns this feed loading ticket. New counts
            // can apply, but refresh must fail conservatively rather than erase
            // that ticket or falsely claim the rest of the pipeline completed.
            run(state, &undo, &restore).await;
            restore.assert_methods(&METHODS[..index]);
            assert!(failed(state, &undo));
            release.send(()).unwrap();
        });
        old.assert_methods(&METHODS[..=index]);
        assert!(!state.peek().core.threads_view.loading);
        assert!(!state.peek().core.activity.loading);
        assert_eq!(
            state.peek().core.workspaces[TEAM].read_cursor(CHANNEL),
            Some(ROOT)
        );
        let retry = retry(state);
        let restore = Script::new(&state.peek(), false);
        run(state, &retry, &restore).await;
        restore.assert_methods(&METHODS);
        assert!(!failed(state, &retry));
        assert_read_state(state, false);
    }
}

#[tokio::test(flavor = "current_thread")]
async fn stale_feed_completion_cannot_clear_a_newer_loading_ticket() {
    for index in [1, 2] {
        let (_dom, mut state) = harness();
        let work = clear(state);
        let script = Script::new(&state.peek(), true);
        script.step(index, |step| {
            step.during(move || {
                let mut s = state.write();
                if index == 1 {
                    s.core.threads_view.loading = false;
                    assert!(s.core.threads_view.begin(false).is_some());
                } else {
                    s.core.activity.loading = false;
                    assert!(s.core.activity.begin(false).is_some());
                }
            })
        });
        run(state, &work, &script).await;
        script.assert_methods(&METHODS[..=index]);
        assert!(failed(state, &work));
        assert!(if index == 1 {
            state.peek().core.threads_view.loading
        } else {
            state.peek().core.activity.loading
        });
    }
}
