use super::*;
const C: &str = "C00000001";
const T: &str = "T00000001";
const ROOT: &str = "1710000000.000001";
const R1: &str = "1710000001.000001";
const R2: &str = "1710000002.000001";
fn session() -> WorkspaceSession {
    WorkspaceSession {
        team_id: T.into(),
        enterprise_id: None,
        name: "offline".into(),
        url: "https://example.slack.com".into(),
        user_id: "U00000001".into(),
        token: "test-only".into(),
    }
}
fn core() -> CoreAppState {
    let mut c = CoreAppState::new(Default::default());
    c.active_team = Some(T.into());
    let mut w = crate::state::Workspace::from_session(&session());
    w.channels.insert(
        C.into(),
        serde_json::from_value(json!({"id":C,"is_channel":true,"is_member":true})).unwrap(),
    );
    c.workspaces.insert(T.into(), w);
    c
}
fn raw() -> Value {
    json!({"ok":true,"threads":[{"root_msg":{"channel":C,"ts":ROOT,"thread_ts":ROOT,"subscribed":true,"reply_count":147,"text":"root","opaque":"kept"},"unread_replies":[{"ts":R2,"text":"unread first"}],"latest_replies":[{"ts":R1,"text":"earlier"},{"ts":R2,"text":"later duplicate"}]}],"has_more":true})
}
fn key() -> ThreadKey {
    (T.into(), C.into(), ROOT.into())
}
#[test]
fn exact_initial_wire_and_strict_positive_original_assignment() {
    for v in [
        json!(null),
        json!({"ok":true}),
        json!({"ok":true,"assignments":{}}),
        json!({"ok":true,"assignments":{"channel_tabs_threads":{"group":"off"}}}),
        json!({"ok":true,"assignments":{"channel_tabs_threads":{"group":true}}}),
        json!({"ok":false,"assignments":{"channel_tabs_threads":{"group":"on"}}}),
    ] {
        assert!(!enabled(&v));
    }
    assert!(enabled(
        &json!({"ok":true,"assignments":{"channel_tabs_threads":{"group":"on"}}})
    ));
    let req = request(&SlackClient::default(), &session(), C).unwrap();
    assert_eq!(req.method, "POST");
    assert!(req.url.contains("/api/subscriptions.thread.getView?"));
    let crate::slack::RequestBody::Form(body) = req.body else {
        panic!("form")
    };
    assert_eq!(
        body,
        vec![
            ("token".into(), "test-only".into()),
            ("limit".into(), "5".into()),
            ("fetch_threads_state".into(), "true".into()),
            ("priority_mode".into(), "all".into()),
            ("channel_id".into(), C.into())
        ]
    );
    assert!(request(&SlackClient::default(), &session(), "D00000001").is_err());
}
#[test]
fn unread_before_latest_dedup_and_partial_context_keep_actual_count() {
    let mut c = core();
    let page = Page::decode(&raw(), C).unwrap();
    let accepted = Read::capture(&c, T, C)
        .unwrap()
        .publish(&mut c, page)
        .unwrap();
    assert!(accepted.has_more());
    let d = &accepted.discussions()[0];
    assert_eq!(d.timestamps(), &[R1.to_owned(), R2.to_owned()]);
    assert!(!d.reached_start());
    let bag = &c.threads[&key()];
    assert!(bag.loaded && bag.has_more_older);
    assert_eq!(bag.messages[0].reply_count, Some(147));
    assert_eq!(bag.messages[2].text.as_deref(), Some("unread first"));
    assert_eq!(bag.messages[0].extra["opaque"], "kept");
    assert!(c.workspaces[T].messages.get(C).is_none());
    assert!(c.threads_view.items.is_empty());
    assert_eq!(c.thread_subscriptions.active(c.account_epoch, &key()), None);
}
#[test]
fn malformed_or_foreign_row_refuses_entire_batch_before_any_canonical_mutation() {
    let base = raw();
    for mutate in 0..10 {
        let mut v = base.clone();
        match mutate {
            0 => v["ok"] = false.into(),
            1 => v["has_more"] = Value::Null,
            2 => v["threads"][0]["root_msg"]["channel"] = "C00000002".into(),
            3 => v["threads"][0]["root_msg"]["thread_ts"] = R1.into(),
            4 => v["threads"][0]["root_msg"]["subscribed"] = false.into(),
            5 => v["threads"][0]["root_msg"]["reply_count"] = Value::Null,
            6 => v["threads"][0]["latest_replies"][0]["ts"] = "bad".into(),
            7 => v["threads"][0]["latest_replies"][0]["channel"] = "C00000002".into(),
            8 => v["threads"][0]["unread_replies"] = json!({}),
            _ => {
                v["threads"][0]["latest_replies"] = json!([]);
                v["threads"][0]["unread_replies"] = json!([]);
            }
        }
        assert!(Page::decode(&v, C).is_err(), "case {mutate}");
    }
    let mut v = base;
    let duplicate = v["threads"][0].clone();
    v["threads"].as_array_mut().unwrap().push(duplicate);
    assert!(Page::decode(&v, C).is_err());
    let mut c = core();
    assert!(c.threads.is_empty());
    let good = Page::decode(&raw(), C).unwrap();
    assert!(
        Read::capture(&c, T, C)
            .unwrap()
            .publish(&mut c, good)
            .is_ok()
    );
}
#[test]
fn refresh_merges_partials_preserves_windows_cursors_and_all_existing_replies() {
    let mut c = core();
    Read::capture(&c, T, C)
        .unwrap()
        .publish(&mut c, Page::decode(&raw(), C).unwrap())
        .unwrap();
    let b = c.threads.get_mut(&key()).unwrap();
    b.last_read = Some(R1.into());
    b.read_revision = 8;
    b.has_more_older = false;
    b.history_loading_older = true;
    b.messages[0].reactions = vec![
        serde_json::from_value(json!({"name":"thumbsup","count":1,"users":["U00000001"]})).unwrap(),
    ];
    b.merge_update(
        serde_json::from_value(
            json!({"channel":C,"ts":"1710000003.000001","thread_ts":ROOT,"text":"retained later"}),
        )
        .unwrap(),
    );
    let timeline = c
        .workspaces
        .get_mut(T)
        .unwrap()
        .messages
        .entry(C.into())
        .or_default();
    timeline.upsert(b.messages[0].clone());
    timeline.loaded = true;
    timeline.has_more_older = true;
    let mut v = raw();
    v["threads"][0]["root_msg"]["text"] = "refreshed".into();
    let read = Read::capture(&c, T, C).unwrap();
    read.publish(&mut c, Page::decode(&v, C).unwrap()).unwrap();
    let b = &c.threads[&key()];
    assert_eq!(b.messages.len(), 4);
    assert_eq!(b.messages[0].reactions[0].name, "thumbsup");
    assert_eq!(b.messages[0].text.as_deref(), Some("refreshed"));
    assert_eq!(b.last_read.as_deref(), Some(R1));
    assert_eq!(b.read_revision, 8);
    assert!(!b.has_more_older && b.history_loading_older);
    let t = &c.workspaces[T].messages[C];
    assert_eq!(t.messages.len(), 1);
    assert_eq!(t.messages[0].text.as_deref(), Some("refreshed"));
    assert!(t.loaded && t.has_more_older);
}
#[test]
fn concurrent_edits_deletions_marks_new_bags_and_subscription_changes_refuse_atomically() {
    for case in 0..7 {
        let mut c = core();
        Read::capture(&c, T, C)
            .unwrap()
            .publish(&mut c, Page::decode(&raw(), C).unwrap())
            .unwrap();
        let read = Read::capture(&c, T, C).unwrap();
        match case {
            0 => {
                c.threads.get_mut(&key()).unwrap().messages[1].text =
                    Some("newer local edit".into());
            }
            1 => c
                .workspaces
                .get_mut(T)
                .unwrap()
                .messages
                .entry(C.into())
                .or_default()
                .record_remote_deletion(R2),
            2 => c.apply_thread_mark(&key(), R2),
            3 => {
                c.threads
                    .entry((T.into(), C.into(), "1710000040.000001".into()))
                    .or_default();
            }
            4 => c.thread_subscriptions.apply(c.account_epoch, &key(), false),
            5 => c.account_epoch += 1,
            _ => c.active_team = Some("TOTHER".into()),
        };
        assert!(!read.current(&c), "case{case}");
        let before = serde_json::to_value(&c.threads[&key()].messages).unwrap();
        assert!(
            read.publish(&mut c, Page::decode(&raw(), C).unwrap())
                .is_err()
        );
        assert_eq!(
            serde_json::to_value(&c.threads[&key()].messages).unwrap(),
            before
        );
    }
}
#[test]
fn prior_tombstones_and_channel_snapshot_never_remove_global_subscriptions_or_rows() {
    let mut c = core();
    c.workspaces
        .get_mut(T)
        .unwrap()
        .messages
        .entry(C.into())
        .or_default()
        .record_remote_deletion(R1);
    c.thread_subscriptions.apply(c.account_epoch, &key(), true);
    c.threads_view.ensure_scope(c.account_epoch, T);
    c.threads_view.loaded = true;
    c.threads_view.has_more = true;
    c.threads_view.next_cursor = Some("opaque-global-coordinate".into());
    let a = Read::capture(&c, T, C)
        .unwrap()
        .publish(&mut c, Page::decode(&raw(), C).unwrap())
        .unwrap();
    assert_eq!(a.discussions()[0].timestamps(), &[R2.to_owned()]);
    assert_eq!(c.threads[&key()].messages.len(), 2);
    let empty = Page::decode(&json!({"ok":true,"threads":[],"has_more":false}), C).unwrap();
    let accepted = Read::capture(&c, T, C)
        .unwrap()
        .publish(&mut c, empty)
        .unwrap();
    assert!(accepted.discussions().is_empty());
    assert_eq!(c.threads[&key()].messages.len(), 2);
    assert_eq!(
        c.thread_subscriptions.active(c.account_epoch, &key()),
        Some(true)
    );
    assert!(c.threads_view.loaded && c.threads_view.has_more);
    assert_eq!(
        c.threads_view.next_cursor.as_deref(),
        Some("opaque-global-coordinate")
    );
}
#[test]
fn original_capture_limits_refuse_without_truncating_known_preimages() {
    let mut c = core();
    let bag = c
        .workspaces
        .get_mut(T)
        .unwrap()
        .messages
        .entry(C.into())
        .or_default();
    bag.pending = (0..20_001).map(|i| i.to_string()).collect();
    assert!(Read::capture(&c, T, C).is_err());
    assert_eq!(c.workspaces[T].messages[C].pending.len(), 20_001);
}
#[test]
fn hidden_reply_is_not_hidden_in_thread_and_count_cutoff_is_observed_not_loaded_length() {
    for field in ["isNonExistent", "no_display", "_hidden_stub"] {
        let m: Message = serde_json::from_value(json!({field:true})).unwrap();
        assert!(hidden(&m));
    }
    let m: Message =
        serde_json::from_value(json!({"_hidden_reply":true,"reply_count":147})).unwrap();
    assert!(!hidden(&m));
    let keys = vec![R1.into(), R2.into()];
    assert_eq!(
        minimum_visible(&m, &keys, Some(R2), false).as_deref(),
        Some(R1)
    );
    let short: Message = serde_json::from_value(json!({"reply_count":4})).unwrap();
    assert_eq!(minimum_visible(&short, &keys, Some(R2), false), None);
}
