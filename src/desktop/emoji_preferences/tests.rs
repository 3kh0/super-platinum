use super::*;
use boundary::Work;
use super_platinum_core::slack::Error;
#[path = "test_support.rs"]
mod support;
use support::*;

#[test]
fn leading_and_latest_trailing_are_independent_of_a_slow_first_response() {
    let h = Harness::new();
    h.record("Efirst", "first");
    h.boundary.pump();
    assert_eq!(h.boundary.requests(), 1);
    assert_eq!(h.boundary.at(0), Duration::ZERO);
    assert_eq!(
        h.boundary
            .form(0)
            .iter()
            .map(|(k, _)| k.as_str())
            .collect::<Vec<_>>(),
        ["token", "name", "value"]
    );
    assert_eq!(
        h.boundary.form(0)[0],
        ("token".into(), "synthetic-token".into())
    );
    assert_eq!(h.boundary.form(0)[1], ("name".into(), "frecency".into()));
    assert!(h.boundary.wire(0).starts_with('{'));
    assert_eq!(h.boundary.sent(0)["first"]["count"], 1);
    h.boundary.advance(1_000);
    h.record("Emiddle", "middle");
    h.boundary.pump();
    h.boundary.advance(1_000);
    h.record("Elatest", "latest");
    h.boundary.pump();
    h.boundary.advance(57_999);
    assert_eq!(h.boundary.requests(), 1);
    h.boundary.advance(1);
    assert_eq!(h.boundary.requests(), 2);
    assert_eq!(h.boundary.at(1), Duration::from_secs(60));
    let latest = h.boundary.sent(1);
    assert_eq!(latest["middle"]["count"], 1);
    assert_eq!(latest["latest"]["count"], 1);
    // First response is deliberately still pending at the second dispatch.
    let before = h.value();
    h.boundary.reply(
        1,
        Ok(serde_json::json!({"ok":true,"prefs":{"frecency":"stale-response"}})),
    );
    h.boundary.reply(0, Ok(serde_json::json!({"ok":true})));
    assert_eq!(h.value(), before);
    h.boundary.advance(120_000);
    assert_eq!(h.boundary.requests(), 2);
}

#[test]
fn owner_and_credentials_retire_before_first_poll_for_reads_and_records() {
    for kind in ROTATIONS {
        let mut h = Harness::new();
        h.record("Eone", "one");
        h.rotate(kind);
        h.boundary.pump();
        assert_eq!(h.boundary.requests(), 0, "record {kind}");
        assert!(h.value().is_none());
        let mut h = Harness::new();
        h.unloaded();
        ensure_with(h.boundary.clone(), h.state);
        h.rotate(kind);
        h.boundary.pump();
        assert_eq!(h.boundary.requests(), 0, "read {kind}");
    }
}

#[test]
fn history_and_request_worker_awaits_cannot_dispatch_for_rotated_owners() {
    for kind in ROTATIONS {
        for work in [Work::History, Work::Request] {
            let mut h = Harness::new();
            h.boundary.hold_next(work);
            h.record("Eone", "one");
            h.boundary.pump();
            assert_eq!(h.boundary.blocked(), 1);
            h.rotate(kind);
            h.boundary.release(0);
            assert_eq!(h.boundary.requests(), 0, "{kind} {work:?}");
            assert!(
                h.state.peek().core.workspaces["OTHER"]
                    .emoji_preferences
                    .frecency()
                    .is_none()
            );
        }
    }
}

#[test]
fn timer_and_response_awaits_retire_without_replaying_or_foreign_mutation() {
    for kind in ROTATIONS {
        let mut h = Harness::new();
        h.record("Eleading", "leading");
        h.boundary.pump();
        h.boundary.advance(1);
        h.record("Etrailing", "trailing");
        h.boundary.pump();
        h.rotate(kind);
        h.boundary.advance(60_000);
        assert_eq!(h.boundary.requests(), 1, "timer {kind}");
        let value = h.value();
        h.boundary
            .reply(0, Err(Error::Transport("synthetic unknown outcome".into())));
        assert_eq!(h.boundary.failures(), 0);
        assert_eq!(h.value(), value);
    }
}

#[test]
fn remote_update_during_history_work_rebases_once_and_keeps_event_time() {
    let mut h = Harness::new();
    h.boundary.hold_next(Work::History);
    h.record("Eone", "one");
    h.boundary.pump();
    h.boundary.advance(12_000);
    h.remote(REMOTE);
    h.boundary.release(0);
    assert_eq!(h.boundary.requests(), 1);
    let body = h.boundary.sent(0);
    assert_eq!(body["server"]["opaque"]["keep"], "yes");
    assert_eq!(body["one"]["count"], 1);
    assert_eq!(body["one"]["visits"], serde_json::json!([1700000000000u64]));
}

#[test]
fn remote_update_during_request_work_rebuilds_unsent_wire_before_dispatch() {
    let mut h = Harness::new();
    h.boundary.hold_next(Work::Request);
    h.record("Eone", "one");
    h.boundary.pump();
    assert_eq!(h.boundary.requests(), 0);
    h.remote(REMOTE);
    h.boundary.release(0);
    assert_eq!(h.boundary.requests(), 1);
    let body = h.boundary.sent(0);
    assert_eq!(body["server"]["count"], 8);
    assert_eq!(body["one"]["count"], 1);
}

#[test]
fn trailing_remote_rebase_preserves_unsent_visits_and_never_replays_dispatched_visit() {
    let mut h = Harness::new();
    h.record("Esent", "sent");
    h.boundary.pump();
    h.boundary.advance(1_000);
    h.record("Epending", "pending");
    h.boundary.pump();
    h.remote(REMOTE);
    h.boundary.advance(59_000);
    assert_eq!(h.boundary.requests(), 2);
    let body = h.boundary.sent(1);
    assert!(body.get("Esent").is_none());
    assert_eq!(body["server"]["opaque"]["keep"], "yes");
    assert_eq!(body["pending"]["count"], 1);
    assert_eq!(
        body["pending"]["visits"],
        serde_json::json!([1700000001000u64])
    );
}

#[test]
fn concurrent_records_preserve_ack_order_before_history_work() {
    let h = Harness::new();
    h.boundary.hold_next(Work::History);
    h.boundary.hold_next(Work::History);
    h.record("Ea", "a");
    h.record("Eb", "b");
    h.boundary.pump();
    assert_eq!(h.boundary.blocked(), 1);
    assert_eq!(h.boundary.requests(), 0);
    h.boundary.release(0);
    assert_eq!(h.boundary.blocked(), 2);
    assert_eq!(h.boundary.requests(), 1);
    h.boundary.release(1);
    h.boundary.advance(60_000);
    assert_eq!(h.boundary.requests(), 2);
    let history = h.history();
    assert_eq!(history["a"]["count"], 1);
    assert_eq!(history["b"]["count"], 1);
}

#[test]
fn same_record_visits_stay_ordered_and_cancelled_middle_does_not_skip_predecessor() {
    let h = Harness::new();
    h.boundary.hold_next(Work::History);
    h.record("Esame", "same");
    h.boundary.pump();
    h.boundary.advance(1_000);
    let cancelled = writer::record_with(
        h.boundary.clone(),
        h.state,
        Usage {
            id: "Ecancelled".into(),
            query: "same".into(),
        },
    );
    h.record("Esame", "same");
    h.boundary.pump();
    drop(cancelled);
    h.boundary.pump();
    assert_eq!(h.boundary.requests(), 0);
    assert_eq!(h.boundary.blocked(), 1);
    h.boundary.release(0);
    let history = h.history();
    assert_eq!(history["same"]["count"], 2);
    assert_eq!(
        history["same"]["visits"],
        serde_json::json!([1700000000000u64, 1700000001000u64])
    );
    assert!(history.get("Ecancelled").is_none());
    for _ in 0..9 {
        h.boundary.advance(1_000);
        h.record("Esame", "same");
        h.boundary.pump();
    }
    let history = h.history();
    assert_eq!(history["same"]["count"], 11);
    assert_eq!(history["same"]["visits"][0], 1700000001000u64);
    assert_eq!(history["same"]["visits"][9], 1700000010000u64);
}

#[test]
fn same_query_distinct_ids_retain_ack_order_even_when_second_is_polled_first() {
    let h = Harness::new();
    let first = writer::record_with(
        h.boundary.clone(),
        h.state,
        Usage {
            id: "Efirst".into(),
            query: "same".into(),
        },
    );
    h.boundary.advance(1_000);
    let second = writer::record_with(
        h.boundary.clone(),
        h.state,
        Usage {
            id: "Esecond".into(),
            query: "same".into(),
        },
    );
    h.boundary.spawn(Box::pin(second));
    h.boundary.pump();
    assert_eq!(h.boundary.requests(), 0);
    h.boundary.spawn(Box::pin(first));
    h.boundary.pump();
    let history = h.history();
    assert_eq!(history["same"][0]["id"], "Efirst");
    assert_eq!(history["same"][1]["id"], "Esecond");
}

#[test]
fn old_get_cannot_commit_or_retire_new_read_after_credential_aba() {
    let mut queued = Harness::new();
    queued.unloaded();
    ensure_with(queued.boundary.clone(), queued.state);
    queued.rotate("token");
    ensure_with(queued.boundary.clone(), queued.state);
    queued
        .state
        .write()
        .core
        .session
        .as_mut()
        .unwrap()
        .workspaces
        .get_mut("T1")
        .unwrap()
        .token = "synthetic-token".into();
    ensure_with(queued.boundary.clone(), queued.state);
    queued.boundary.pump();
    assert_eq!(queued.boundary.requests(), 1);

    let mut h = Harness::new();
    h.unloaded();
    ensure_with(h.boundary.clone(), h.state);
    h.boundary.pump();
    h.rotate("token");
    ensure_with(h.boundary.clone(), h.state);
    h.boundary.pump();
    h.state
        .write()
        .core
        .session
        .as_mut()
        .unwrap()
        .workspaces
        .get_mut("T1")
        .unwrap()
        .token = "synthetic-token".into();
    ensure_with(h.boundary.clone(), h.state);
    h.boundary.pump();
    assert_eq!(h.boundary.requests(), 3);
    h.boundary.reply(
        0,
        Ok(serde_json::json!({"ok":true,"prefs":{"frecency":"{}"}})),
    );
    assert!(
        !h.state.peek().core.workspaces["T1"]
            .emoji_preferences
            .loaded()
    );
    assert!(h.value().is_none());
    ensure_with(h.boundary.clone(), h.state);
    h.boundary.pump();
    assert_eq!(h.boundary.requests(), 3);
    h.boundary.reply(1, Err(Error::Transport("stale B".into())));
    assert_eq!(h.boundary.failures(), 0);
    h.boundary.reply(
        2,
        Ok(serde_json::json!({"ok":true,"prefs":{"frecency":REMOTE}})),
    );
    assert!(
        h.state.peek().core.workspaces["T1"]
            .emoji_preferences
            .loaded()
    );
    assert_eq!(h.value(), Some(Value::String(REMOTE.into())));
}

#[test]
fn old_writer_response_cannot_rate_limit_replacement_after_credential_aba() {
    let mut h = Harness::new();
    h.record("Ea", "a");
    h.boundary.pump();
    h.rotate("token");
    h.record("Eb", "b");
    h.boundary.pump();
    h.state
        .write()
        .core
        .session
        .as_mut()
        .unwrap()
        .workspaces
        .get_mut("T1")
        .unwrap()
        .token = "synthetic-token".into();
    h.record("Ec", "c");
    h.boundary.pump();
    assert_eq!(h.boundary.requests(), 3);
    h.boundary.reply(
        0,
        Err(Error::RateLimited {
            retry_after_secs: Some(125),
        }),
    );
    assert_eq!(h.boundary.failures(), 0);
    h.boundary.advance(1_000);
    h.record("Ed", "d");
    h.boundary.pump();
    h.boundary.advance(59_000);
    assert_eq!(h.boundary.requests(), 4);
    assert_eq!(h.boundary.at(3), Duration::from_secs(60));
}

#[test]
fn retry_after_extends_trailing_deadline_without_retrying_an_old_request() {
    let h = Harness::new();
    h.record("Efirst", "first");
    h.boundary.pump();
    h.boundary.advance(1_000);
    h.record("Epending", "pending");
    h.boundary.pump();
    h.boundary.reply(
        0,
        Err(Error::RateLimited {
            retry_after_secs: Some(125),
        }),
    );
    h.boundary.advance(59_000);
    assert_eq!(h.boundary.requests(), 1);
    h.boundary.advance(65_999);
    assert_eq!(h.boundary.requests(), 1);
    h.boundary.advance(1);
    assert_eq!(h.boundary.requests(), 2);
    assert_eq!(h.boundary.at(1), Duration::from_secs(126));
    assert_eq!(h.boundary.sent(1)["pending"]["count"], 1);
    assert_eq!(h.boundary.failures(), 1);
    h.boundary
        .reply(1, Err(Error::Transport("synthetic response lost".into())));
    let retained = h.value();
    h.boundary.advance(500_000);
    assert_eq!(h.boundary.requests(), 2);
    assert_eq!(h.value(), retained);
    assert_eq!(h.boundary.failures(), 2);
}

#[test]
fn failure_does_not_overwrite_a_newer_remote_pref_or_increment_emoji_use() {
    let mut h = Harness::new();
    h.state
        .write()
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .emoji_preferences
        .apply_change("emoji_use", serde_json::json!({"rocket":9}));
    h.record("Eone", "one");
    h.boundary.pump();
    h.remote(REMOTE);
    let value = h.value();
    h.boundary.reply(
        0,
        Err(Error::HttpStatus {
            status: 500,
            retry_after_secs: None,
        }),
    );
    assert_eq!(h.value(), value);
    assert_eq!(h.boundary.failures(), 1);
    assert_eq!(
        h.state.peek().core.workspaces["T1"]
            .emoji_preferences
            .emoji_use(),
        Some(&serde_json::json!({"rocket":9}))
    );
    h.boundary.advance(60_000);
    assert_eq!(h.boundary.requests(), 1);
}

#[test]
fn unloaded_and_malformed_or_order_lost_history_never_become_empty_history() {
    for value in [
        None,
        Some(Value::String("[]".into())),
        Some(serde_json::json!({})),
    ] {
        let mut h = Harness::new();
        match value {
            None => h.unloaded(),
            Some(v) => {
                h.state
                    .write()
                    .core
                    .workspaces
                    .get_mut("T1")
                    .unwrap()
                    .emoji_preferences
                    .apply_change("frecency", v);
            }
        }
        let before = h.value();
        h.record("Eone", "one");
        h.boundary.pump();
        assert_eq!(h.boundary.requests(), 0);
        assert_eq!(h.value(), before);
    }
}

#[test]
fn get_uses_exact_keys_preserves_newer_partial_prefs_and_obeys_retry_after() {
    let mut h = Harness::new();
    h.unloaded();
    ensure_with(h.boundary.clone(), h.state);
    h.boundary.pump();
    assert_eq!(
        h.boundary.form(0),
        vec![
            ("token".into(), "synthetic-token".into()),
            (
                "prefs".into(),
                "preferred_skin_tone,emoji_mode,emoji_use,frecency".into()
            )
        ]
    );
    h.remote(REMOTE);
    h.boundary.reply(
        0,
        Ok(serde_json::json!({"ok":true,"prefs":{"frecency":"{}"}})),
    );
    assert_eq!(h.value(), Some(Value::String(REMOTE.into())));
    assert!(
        !h.state.peek().core.workspaces["T1"]
            .emoji_preferences
            .loaded()
    );
    ensure_with(h.boundary.clone(), h.state);
    h.boundary.pump();
    assert_eq!(h.boundary.requests(), 2);
    h.boundary.reply(
        1,
        Err(Error::RateLimited {
            retry_after_secs: Some(125),
        }),
    );
    h.boundary.advance(30_000);
    ensure_with(h.boundary.clone(), h.state);
    h.boundary.pump();
    assert_eq!(h.boundary.requests(), 2);
    h.boundary.advance(95_000);
    ensure_with(h.boundary.clone(), h.state);
    h.boundary.pump();
    assert_eq!(h.boundary.requests(), 3);
    h.boundary.reply(
        2,
        Ok(serde_json::json!({"ok":true,"prefs":{"frecency":REMOTE,"emoji_mode":"as_text"}})),
    );
    assert!(
        h.state.peek().core.workspaces["T1"]
            .emoji_preferences
            .loaded()
    );
}

#[test]
fn get_response_is_ignored_after_same_transport_credential_rotation() {
    let mut h = Harness::new();
    h.unloaded();
    ensure_with(h.boundary.clone(), h.state);
    h.boundary.pump();
    h.rotate("token");
    h.boundary.reply(
        0,
        Ok(serde_json::json!({"ok":true,"prefs":{"frecency":"{}"}})),
    );
    assert!(h.value().is_none());
    assert!(
        !h.state.peek().core.workspaces["T1"]
            .emoji_preferences
            .loaded()
    );
    assert_eq!(h.boundary.requests(), 1);
}

#[test]
fn unrelated_pref_revisions_never_reapply_pending_history_visits() {
    let mut h = Harness::new();
    h.record("Esent", "sent");
    h.boundary.pump();
    h.boundary.advance(1_000);
    h.record("Ep", "pending");
    h.boundary.pump();
    h.state
        .write()
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .emoji_preferences
        .apply_change("emoji_mode", Value::String("as_text".into()));
    h.record("En", "next");
    h.boundary.pump();
    h.state
        .write()
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .emoji_preferences
        .apply_change("preferred_skin_tone", Value::String("4".into()));
    h.boundary.advance(59_000);
    assert_eq!(h.boundary.requests(), 2);
    let body = h.boundary.sent(1);
    assert_eq!(body["pending"]["count"], 1);
    assert_eq!(body["next"]["count"], 1);

    let mut h = Harness::new();
    h.boundary.hold_next(Work::Request);
    h.record("Ep", "pending");
    h.boundary.pump();
    h.state
        .write()
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .emoji_preferences
        .apply_change("emoji_use", serde_json::json!({"rocket":9}));
    h.boundary.release(0);
    assert_eq!(h.boundary.requests(), 1);
    assert_eq!(h.boundary.sent(0)["pending"]["count"], 1);
}

#[test]
fn service_unavailable_retry_after_gates_reads_and_unsent_writes() {
    let unavailable = || Error::HttpStatus {
        status: 503,
        retry_after_secs: Some(125),
    };
    let h = Harness::new();
    h.record("Esent", "sent");
    h.boundary.pump();
    h.boundary.advance(1_000);
    h.record("Epending", "pending");
    h.boundary.pump();
    let retained = h.value();
    h.boundary.reply(0, Err(unavailable()));
    assert_eq!(h.value(), retained);
    h.boundary.advance(59_000);
    assert_eq!(h.boundary.requests(), 1);
    h.boundary.advance(65_999);
    assert_eq!(h.boundary.requests(), 1);
    h.boundary.advance(1);
    assert_eq!(h.boundary.requests(), 2);
    assert_eq!(h.boundary.at(1), Duration::from_secs(126));
    assert_eq!(h.boundary.sent(1)["sent"]["count"], 1);
    assert_eq!(h.boundary.sent(1)["pending"]["count"], 1);
    assert_eq!(h.boundary.failures(), 1);
    h.boundary.advance(500_000);
    assert_eq!(h.boundary.requests(), 2);

    let mut h = Harness::new();
    h.unloaded();
    ensure_with(h.boundary.clone(), h.state);
    h.boundary.pump();
    h.boundary.reply(0, Err(unavailable()));
    h.boundary.advance(30_000);
    ensure_with(h.boundary.clone(), h.state);
    h.boundary.pump();
    assert_eq!(h.boundary.requests(), 1);
    h.boundary.advance(94_999);
    ensure_with(h.boundary.clone(), h.state);
    h.boundary.pump();
    assert_eq!(h.boundary.requests(), 1);
    h.boundary.advance(1);
    ensure_with(h.boundary.clone(), h.state);
    h.boundary.pump();
    assert_eq!(h.boundary.requests(), 2);
    assert_eq!(h.boundary.at(1), Duration::from_secs(125));
    assert!(h.value().is_none());
    assert_eq!(h.boundary.failures(), 1);
}

#[test]
fn retained_preference_state_preserves_background_shell_send_boundary() {
    fn is_send<T: Send>() {}
    is_send::<State>();
    is_send::<ShellState>();
}
