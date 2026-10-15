use super::super::fixture::{
    LATEST, METHODS, ROOT, Script, clear, failed, harness, key, retry, run,
};
use dioxus::prelude::*;
use serde_json::{Value, json};

#[tokio::test(flavor = "current_thread")]
async fn replies_requires_own_boolean_subscription_and_adopts_false_as_well_as_true() {
    for subscribed in [
        None,
        Some(Value::Null),
        Some(json!("true")),
        Some(json!(false)),
        Some(json!(true)),
    ] {
        let (_dom, state) = harness();
        let work = clear(state);
        let script = Script::new(&state.peek(), true);
        let value = subscribed.clone();
        script.step(4, |step| {
            step.response(|raw| {
                let root = &mut raw["messages"][0];
                root["last_read"] = json!(ROOT);
                root["text"] = json!("Only supported replies authority may replace this parent");
                if let Some(value) = value {
                    root["subscribed"] = value;
                } else {
                    root.as_object_mut().unwrap().remove("subscribed");
                }
            })
        });
        run(state, &work, &script).await;
        script.assert_methods(&METHODS);
        let expected = subscribed.as_ref().and_then(Value::as_bool);
        assert_eq!(failed(state, &work), expected.is_none());
        let s = state.peek();
        let epoch = s.core.account_epoch;
        assert_eq!(s.core.thread_subscriptions.active(epoch, &key()), expected);
        assert_eq!(
            s.core.thread_cursor(&key()),
            Some(if expected.is_some() { ROOT } else { LATEST })
        );
        if let Some(active) = expected {
            let root = s.core.threads[&key()]
                .messages
                .iter()
                .find(|m| m.ts.as_deref() == Some(ROOT))
                .unwrap();
            assert_eq!(root.extra.get("subscribed"), Some(&json!(active)));
            assert_eq!(
                s.core
                    .threads_view
                    .items
                    .iter()
                    .any(|item| item.root_ts().map(String::as_str) == Some(ROOT)),
                active
            );
        } else {
            assert!(
                !s.core.threads[&key()]
                    .messages
                    .iter()
                    .any(|m| m.text.as_deref()
                        == Some("Only supported replies authority may replace this parent"))
            );
        }
    }
}

#[tokio::test(flavor = "current_thread")]
async fn invalid_subscription_cursor_does_not_merge_content_or_adopt_metadata() {
    for cursor in [
        Value::Null,
        json!("bad"),
        json!("1e4"),
        json!("100"),
        json!("100.1234567"),
        json!("-1.0"),
    ] {
        let (_dom, state) = harness();
        let work = clear(state);
        let script = Script::new(&state.peek(), true);
        script.step(4, |step| {
            step.response(|raw| {
                raw["messages"][0]["last_read"] = cursor;
                raw["messages"][0]["text"] = json!("Invalid receipt content");
            })
        });
        run(state, &work, &script).await;
        assert!(failed(state, &work));
        let s = state.peek();
        assert_eq!(
            s.core
                .thread_subscriptions
                .active(s.core.account_epoch, &key()),
            None
        );
        assert_eq!(s.core.thread_cursor(&key()), Some(LATEST));
        assert!(
            !s.core.threads[&key()]
                .messages
                .iter()
                .any(|m| m.text.as_deref() == Some("Invalid receipt content"))
        );
    }
}

#[tokio::test(flavor = "current_thread")]
async fn subscription_aba_or_pending_change_during_read_keeps_both_fields() {
    for pending in [false, true] {
        let (_dom, mut state) = harness();
        {
            let mut s = state.write();
            let epoch = s.core.account_epoch;
            s.core.thread_subscriptions.apply(epoch, &key(), true);
        }
        let work = clear(state);
        let script = Script::new(&state.peek(), true);
        script.step(4, |step| {
            step.during(move || {
                let mut s = state.write();
                let epoch = s.core.account_epoch;
                if pending {
                    assert!(
                        s.core
                            .thread_subscriptions
                            .begin(epoch, &key(), false)
                            .is_some()
                    );
                } else {
                    s.core.thread_subscriptions.apply(epoch, &key(), false);
                    s.core.thread_subscriptions.apply(epoch, &key(), true);
                }
            })
        });
        run(state, &work, &script).await;
        script.assert_methods(&METHODS);
        assert!(failed(state, &work));
        let s = state.peek();
        let epoch = s.core.account_epoch;
        assert_eq!(
            s.core.thread_subscriptions.active(epoch, &key()),
            Some(true)
        );
        assert_eq!(s.core.thread_subscriptions.pending(epoch, &key()), pending);
        assert_eq!(
            s.core.thread_subscriptions.revision(epoch, &key()),
            if pending { 1 } else { 3 }
        );
        assert_eq!(s.core.thread_cursor(&key()), Some(LATEST));
    }
}

#[tokio::test(flavor = "current_thread")]
async fn already_pending_subscription_prevents_replies_dispatch() {
    let (_dom, mut state) = harness();
    {
        let mut s = state.write();
        let epoch = s.core.account_epoch;
        assert!(
            s.core
                .thread_subscriptions
                .begin(epoch, &key(), false)
                .is_some()
        );
    }
    let work = clear(state);
    let script = Script::new(&state.peek(), true);
    run(state, &work, &script).await;
    script.assert_methods(&METHODS[..4]);
    assert!(failed(state, &work));
}

#[tokio::test(flavor = "current_thread")]
async fn explicit_zero_thread_baselines_are_adopted_without_synthesizing_missing_cursor() {
    for cursor in ["0", "0.000000"] {
        let (_dom, state) = harness();
        let work = clear(state);
        let script = Script::new(&state.peek(), true);
        script.step(4, |step| {
            step.response(|raw| {
                raw["messages"][0]["last_read"] = json!(cursor);
            })
        });
        run(state, &work, &script).await;
        script.assert_methods(&METHODS);
        assert!(!failed(state, &work));
        let s = state.peek();
        assert_eq!(s.core.thread_cursor(&key()), Some(cursor));
        assert_eq!(
            s.core
                .thread_subscriptions
                .active(s.core.account_epoch, &key()),
            Some(true)
        );
    }
}

#[tokio::test(flavor = "current_thread")]
async fn absent_cursor_and_duplicate_roots_expose_incomplete_refresh_then_allow_retry() {
    // Slack preprocessing permits an absent cursor. Full reconciliation still
    // needs a proof; equal and conflicting duplicate roots are also unsupported.
    for case in 0..3 {
        let (_dom, state) = harness();
        let work = clear(state);
        let script = Script::new(&state.peek(), true);
        script.step(4, |step| {
            step.response(|raw| {
                raw["messages"][0]["text"] = json!("Unsupported root receipt");
                if case == 0 {
                    raw["messages"][0]
                        .as_object_mut()
                        .unwrap()
                        .remove("last_read");
                } else {
                    let mut duplicate = raw["messages"][0].clone();
                    if case == 2 {
                        duplicate["subscribed"] = json!(false);
                        duplicate["last_read"] = json!(ROOT);
                    }
                    raw["messages"].as_array_mut().unwrap().push(duplicate);
                }
            })
        });
        run(state, &work, &script).await;
        script.assert_methods(&METHODS);
        assert!(failed(state, &work));
        {
            let s = state.peek();
            let entry = &s.bulk_read.entries[&work.context.owner];
            assert!(
                entry
                    .message
                    .as_deref()
                    .unwrap()
                    .contains("current read state could not be refreshed")
            );
            assert_eq!(s.core.thread_cursor(&key()), Some(LATEST));
            assert_eq!(
                s.core
                    .thread_subscriptions
                    .active(s.core.account_epoch, &key()),
                None
            );
            assert!(
                !s.core.threads[&key()]
                    .messages
                    .iter()
                    .any(|m| m.text.as_deref() == Some("Unsupported root receipt"))
            );
        }
        let retry = retry(state);
        let script = Script::new(&state.peek(), true);
        run(state, &retry, &script).await;
        script.assert_methods(&METHODS);
        assert!(!failed(state, &retry));
    }
}
