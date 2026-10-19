use super::*;
use crate::{
    slack::client::RequestBody,
    slack_lists::{Document, Record},
};
use serde_json::json;
fn session() -> WorkspaceSession {
    WorkspaceSession {
        team_id: "TTEST".into(),
        user_id: "UTEST".into(),
        enterprise_id: None,
        name: "Fixture".into(),
        url: "https://fixture.slack.com".into(),
        token: "fixture".into(),
    }
}
#[test]
fn synthetic_and_acquired_root_use_exact_read_fields() {
    let doc = Document::parse(
        json!({"id":"F01234567","mode":"list","list_metadata":{"schema":[],"views":[]}}),
    )
    .unwrap();
    let mut row = Record::parse(json!({"id":"Rec1","fields":[]})).unwrap();
    for known in [false, true] {
        if known {
            row.raw["thread_ts"] = json!("1700000000.000001");
        }
        let target = Target::capture(&doc, &row).unwrap();
        let query = if known {
            ReplyQuery {
                cursor: Some("next+opaque".into()),
                ..Default::default()
            }
        } else {
            ReplyQuery::default()
        };
        let req = list_record_comments_replies(
            &SlackClient::default(),
            &session(),
            &target,
            &query,
            Tombstones::Include,
        )
        .unwrap();
        assert!(req.url.contains("/api/conversations.replies?"));
        let RequestBody::Form(fields) = req.body else {
            panic!()
        };
        let mut expected = vec![
            ("token".into(), "fixture".into()),
            ("channel".into(), "C01234567".into()),
            (
                "ts".into(),
                if known {
                    "1700000000.000001"
                } else {
                    "F01234567-Rec1"
                }
                .into(),
            ),
            ("inclusive".into(), "true".into()),
            ("limit".into(), "28".into()),
            ("cached_latest_updates".into(), "{}".into()),
            ("_x_reason".into(), "history-api/fetchReplies".into()),
        ];
        if known {
            expected.push(("cursor".into(), "next+opaque".into()));
        }
        expected.push(("include_tombstones".into(), "true".into()));
        assert_eq!(fields, expected);
    }
}
#[test]
fn exact_root_equal_bounds_remove_latest_and_tombstones_can_be_omitted() {
    let doc = Document::parse(
        json!({"id":"F01234567","mode":"list","list_metadata":{"schema":[],"views":[]}}),
    )
    .unwrap();
    let row =
        Record::parse(json!({"id":"Rec1","fields":[],"thread_ts":"1700000000.000001"})).unwrap();
    let target = Target::capture(&doc, &row).unwrap();
    let q = ReplyQuery {
        oldest: target.root_ts().map(str::to_owned),
        latest: target.root_ts().map(str::to_owned),
        cursor: None,
    };
    let req = list_record_comments_replies(
        &SlackClient::default(),
        &session(),
        &target,
        &q,
        Tombstones::Exclude,
    )
    .unwrap();
    let RequestBody::Form(fields) = req.body else {
        panic!()
    };
    assert!(
        fields
            .iter()
            .any(|(k, v)| k == "oldest" && v == "1700000000.000001")
    );
    assert!(
        !fields
            .iter()
            .any(|(k, _)| k == "latest" || k == "include_tombstones")
    );
}
