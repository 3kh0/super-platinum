use super::*;
use crate::{
    config::WorkspaceSession,
    slack::{SlackClient, api},
    slack_lists::{
        Document, Record,
        comments::{ReplyQuery, Target, Tombstones},
    },
};
use serde_json::json;
fn workspace() -> WorkspaceSession {
    WorkspaceSession {
        team_id: "TTEST".into(),
        user_id: "UTEST".into(),
        enterprise_id: None,
        name: "Fixture".into(),
        url: "https://fixture.slack.com".into(),
        token: "fixture".into(),
    }
}
fn request() -> PreparedRequest {
    let d = Document::parse(
        json!({"id":"F01234567","mode":"list","list_metadata":{"schema":[],"views":[]}}),
    )
    .unwrap();
    let r = Record::parse(json!({"id":"Rec1","fields":[]})).unwrap();
    api::list_record_comments_replies(
        &SlackClient::default(),
        &workspace(),
        &Target::capture(&d, &r).unwrap(),
        &ReplyQuery::default(),
        Tombstones::Include,
    )
    .unwrap()
}
fn set(r: &mut PreparedRequest, key: &str, value: &str) {
    let RequestBody::Form(f) = &mut r.body else {
        panic!()
    };
    if let Some((_, v)) = f.iter_mut().find(|(k, _)| k == key) {
        *v = value.into()
    } else {
        f.push((key.into(), value.into()));
    }
}
#[test]
fn factories_admit_only_exact_read_acquisition_shapes() {
    let c = SlackClient::default();
    let w = workspace();
    assert!(read_allowed(&request()));
    assert!(!acquire_allowed(&request()));
    for r in [
        api::file_info(&c, &w, "F01234567"),
        api::list_record_subtasks(&c, &w, "F01234567", "Rec1"),
        api::list_experiments(&c, &w),
        api::conversations_generic_info(&c, &w, &BTreeMap::from([("C01234567".into(), 0)])),
    ] {
        assert!(acquire_allowed(&r), "{}", r.url);
        assert!(!read_allowed(&r));
    }
    for endpoint in [
        "chat.postMessage",
        "chat.update",
        "chat.delete",
        "lists.records.open",
        "lists.getThreadsForRecord",
    ] {
        let mut r = request();
        r.url = format!("https://fixture.slack.com/api/{endpoint}");
        assert!(!read_allowed(&r) && !acquire_allowed(&r));
    }
    let mut r = request();
    r.method = "GET";
    assert!(!read_allowed(&r));
    for url in [
        "https://slack.com.evil.test/api/conversations.replies",
        "https://fixture.slack.com:444/api/conversations.replies",
        "https://u@fixture.slack.com/api/conversations.replies",
        "https://fixture.slack.com/api/conversations.replies?unknown=1",
    ] {
        let mut r = request();
        r.url = url.into();
        assert!(!read_allowed(&r));
    }
}

#[test]
fn full_reaction_factory_is_a_closed_original_message_read() {
    let request = api::reactions_get(
        &SlackClient::default(),
        &workspace(),
        "C01234567".into(),
        "1700000001.000001".into(),
    );
    assert!(request.url.contains("/api/reactions.get"));
    let RequestBody::Form(fields) = &request.body else {
        panic!()
    };
    assert_eq!(
        fields
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect::<BTreeMap<_, _>>(),
        BTreeMap::from([
            ("token", "fixture"),
            ("channel", "C01234567"),
            ("timestamp", "1700000001.000001"),
            ("full", "true")
        ])
    );
    assert!(acquire_allowed(&request));
    assert!(!read_allowed(&request));
    for (key, value) in [
        ("full", "false"),
        ("timestamp", "bad"),
        ("channel", "C_OTHER"),
        ("file", "F01234567"),
        ("name", "wave"),
        ("ts", "1700000001.000001"),
    ] {
        let mut changed = request.clone();
        set(&mut changed, key, value);
        assert!(!acquire_allowed(&changed), "{key}");
    }
    let mut missing = request.clone();
    let RequestBody::Form(fields) = &mut missing.body else {
        panic!()
    };
    fields.retain(|(key, _)| key != "full");
    assert!(!acquire_allowed(&missing));
    let mut duplicate = request.clone();
    let RequestBody::Form(fields) = &mut duplicate.body else {
        panic!()
    };
    fields.push(("full".into(), "true".into()));
    assert!(!acquire_allowed(&duplicate));
    for url in [
        "https://fixture.slack.com.evil.test/api/reactions.get",
        "https://fixture.slack.com/api/reactions.get?full=true",
        "https://fixture.slack.com/api/reactions.remove",
    ] {
        let mut changed = request.clone();
        changed.url = url.into();
        assert!(!acquire_allowed(&changed));
    }
}
#[test]
fn malformed_foreign_or_ambiguous_read_forms_are_refused() {
    for (k, v) in [
        ("channel", "C76543210"),
        ("ts", "F76543210-Rec1"),
        ("limit", "100"),
        ("cached_latest_updates", "{\"x\":1}"),
        ("inclusive", "false"),
        ("include_tombstones", "false"),
        ("cursor", "next"),
        ("oldest", "1700000000.000001"),
        ("unexpected", "x"),
    ] {
        let mut r = request();
        set(&mut r, k, v);
        assert!(!read_allowed(&r), "{k}");
    }
    let mut duplicate = request();
    let RequestBody::Form(f) = &mut duplicate.body else {
        panic!()
    };
    f.push(("channel".into(), "C01234567".into()));
    assert!(!read_allowed(&duplicate));
    let mut known = request();
    set(&mut known, "ts", "1700000000.000001");
    set(&mut known, "cursor", "opaque+next");
    assert!(read_allowed(&known));
    set(&mut known, "oldest", "1700000000.000000");
    assert!(!read_allowed(&known));
    let mut both = request();
    set(&mut both, "ts", "1700000000.000001");
    set(&mut both, "oldest", "1700000000.000001");
    set(&mut both, "latest", "1700000000.000001");
    assert!(!read_allowed(&both));
}
#[tokio::test]
async fn initial_retirement_never_polls_work_and_ready_retirement_refuses_result() {
    let polled = std::cell::Cell::new(false);
    let result: Result<Value, Error> = guarded(|| false, async {
        polled.set(true);
        Ok(json!({"ok":true}))
    })
    .await;
    assert!(matches!(result, Err(Error::Api(_))));
    assert!(!polled.get());
    let active = std::cell::Cell::new(true);
    let result = guarded(|| active.get(), async {
        active.set(false);
        Ok(json!({"ok":true}))
    })
    .await;
    assert!(matches!(result, Err(Error::Api(_))));
}
#[tokio::test]
async fn pending_owner_retirement_drops_read_and_errors_remain_errors() {
    let active = std::cell::Cell::new(true);
    let dropped = std::cell::Cell::new(false);
    struct Dropped<'a>(&'a std::cell::Cell<bool>);
    impl Drop for Dropped<'_> {
        fn drop(&mut self) {
            self.0.set(true)
        }
    }
    let read = guarded(|| active.get(), async {
        let _drop = Dropped(&dropped);
        std::future::pending::<Result<Value, Error>>().await
    });
    let retire = async {
        tokio::time::sleep(Duration::from_millis(10)).await;
        active.set(false)
    };
    let (result, _) = tokio::join!(read, retire);
    assert!(matches!(result, Err(Error::Api(_))));
    assert!(dropped.get());
    let result: Result<Value, Error> = guarded(|| true, async {
        Err(Error::RateLimited {
            retry_after_secs: Some(17),
        })
    })
    .await;
    assert!(matches!(
        result,
        Err(Error::RateLimited {
            retry_after_secs: Some(17)
        })
    ));
}
#[tokio::test]
async fn public_read_refuses_retired_and_offline_without_dispatch() {
    let t = Transport::new("fixture-cookie").unwrap();
    assert!(matches!(
        t.execute_list_comments_read(request(), || false).await,
        Err(Error::Api(_))
    ));
    t.set_health(Health::Offline);
    assert!(matches!(
        t.execute_list_comments_read(request(), || true).await,
        Err(Error::Offline(_))
    ));
}
