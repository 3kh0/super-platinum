use super::*;
use crate::config::WorkspaceSession;
use crate::slack::{
    api,
    client::{RequestBody, SlackClient},
};
use crate::slack_lists::{
    CellEdit, CellEditValue, ColumnCreate, ColumnCreateAttempt, Document, PersonalTaskLists,
    RecordsPage,
};
use serde_json::json;

fn ordinary() -> PersonalTaskLists {
    PersonalTaskLists::from_response(
        &json!({"ok":true,"prefs":{"task_lists":"[]","assigned_to_list":""}}),
    )
    .unwrap()
}
fn doc(options: Value) -> Document {
    Document::parse(json!({"id":"F1","mode":"list","user":"U1","access":"write","list_limits":{"over_column_maximum":false},"list_metadata":{"schema":[
        {"id":"C1","key":"NAME","name":"Title","type":"text","is_primary_column":true},
        {"id":"C2","key":"support","name":"Support","type":"vote","options":options,"opaque":{"keep":7}}
    ],"views":[{"id":"V1","name":"Custom","type":"table","columns":[{"id":"C1","key":"NAME","visible":true},{"id":"C2","key":"support","visible":true}],"filters":[],"sorts":[]}],"opaque":{"keep":8}}})).unwrap()
}
fn page(fields: Value) -> RecordsPage {
    RecordsPage::parse(json!({"ok":true,"records":[{"id":"R1","fields":fields,"created_by":"U9","opaque":{"keep":9}}],"response_metadata":{"next_cursor":"NEXT"}})).unwrap()
}
fn cell(voters: Value) -> RecordsPage {
    page(json!([{"key":"support","column_id":"C2","row_id":"R1","user":voters}]))
}
fn edit(
    d: &Document,
    p: &RecordsPage,
    user: &str,
    intent: VoteIntent,
) -> Result<CellEdit, ListError> {
    CellEdit::capture_vote(
        d,
        p,
        "R1",
        "C2",
        Some("full"),
        Some(&ordinary()),
        user,
        intent,
    )
}
fn session() -> WorkspaceSession {
    WorkspaceSession {
        team_id: "T1".into(),
        enterprise_id: None,
        user_id: "U1".into(),
        name: "fixture".into(),
        url: "https://fixture.slack.com".into(),
        token: "fixture-token".into(),
    }
}

#[test]
fn default_standard_custom_have_exact_seven_field_once_create_forms() {
    let d = doc(VoteOptions::default().wire());
    let p = page(json!([]));
    assert_eq!(
        VoteOptions::default().wire(),
        json!({"emoji":":sparkling_heart:"})
    );
    for (emoji, team) in [
        (":sparkling_heart:", None),
        (":heart:", Some("T1")),
        (":fixture_custom:", Some("T1")),
    ] {
        let options = VoteOptions::capture(emoji, team).unwrap();
        let intent = ColumnCreate::capture_vote(
            &d,
            &p,
            "V1",
            " Support ",
            options.clone(),
            "U1",
            Some("full"),
            Some(&ordinary()),
        )
        .unwrap();
        let mut attempt = ColumnCreateAttempt::new(intent);
        let permit = attempt
            .admit(&d, &p, "U1", Some("full"), Some(&ordinary()))
            .unwrap();
        assert!(
            attempt
                .admit(&d, &p, "U1", Some("full"), Some(&ordinary()))
                .is_err()
        );
        let request = api::list_column_create(&SlackClient::default(), &session(), permit);
        assert!(!request.retry_safe());
        let RequestBody::Form(form) = request.body else {
            panic!("form")
        };
        assert_eq!(
            form.iter().map(|x| x.0.as_str()).collect::<Vec<_>>(),
            [
                "token",
                "list_id",
                "view_id",
                "name",
                "type",
                "options",
                "_x_reason"
            ]
        );
        assert_eq!(form[3].1, "Support");
        assert_eq!(form[4].1, "vote");
        assert_eq!(
            serde_json::from_str::<Value>(&form[5].1).unwrap(),
            options.wire()
        );
        assert!(options.wire().get("max").is_none());
        assert!(options.wire().get("emoji_url").is_none());
        assert_eq!(
            options.wire().get("emoji_team_id").is_some(),
            team.is_some()
        );
        assert!(attempt.finish(false));
        assert!(!attempt.finish(false));
    }
}

#[test]
fn config_and_observed_addition_are_exact_without_url_normalization() {
    for raw in [
        json!(null),
        json!({}),
        json!({"emoji":null}),
        json!({"emoji":"heart"}),
        json!({"emoji":":bad name:"}),
        json!({"emoji":":heart:","max":5}),
        json!({"emoji":":heart:","emoji_team_id":null}),
        json!({"emoji":":heart:","emoji_url":"https://fixture.invalid/e"}),
    ] {
        assert!(VoteOptions::from_wire(&raw).is_err(), "{raw}");
    }
    let options = VoteOptions::capture(":custom:", Some("T1")).unwrap();
    let d = doc(VoteOptions::default().wire());
    let p = page(json!([]));
    let intent = ColumnCreate::capture_vote(
        &d,
        &p,
        "V1",
        "Support",
        options.clone(),
        "U1",
        Some("full"),
        Some(&ordinary()),
    )
    .unwrap();
    let mut raw = d.raw.clone();
    raw["list_metadata"]["schema"].as_array_mut().unwrap().push(json!({"id":"C_NEW","key":"new_support","name":"Support","type":"vote","options":options.wire(),"opaque":true}));
    raw["list_metadata"]["views"][0]["columns"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"C_NEW","key":"new_support","visible":true}));
    assert_eq!(
        intent
            .matching_addition(&Document::parse(raw.clone()).unwrap(), &p)
            .unwrap()
            .id,
        "C_NEW"
    );
    raw["list_metadata"]["schema"][2]["options"]["emoji_url"] = json!("https://fixture.invalid/e");
    assert!(
        intent
            .matching_addition(&Document::parse(raw).unwrap(), &p)
            .is_none()
    );
    assert_eq!(intent.original(), &d);
    assert_eq!(intent.original_records(), &p);
}

#[test]
fn own_vote_preserves_other_order_and_uses_user_wire_never_arbitrary_assignment() {
    let d = doc(VoteOptions::default().wire());
    for (before, intent, wanted) in [
        (vec![], VoteIntent::AddMyVote, vec!["U1"]),
        (
            vec!["U9", "U8"],
            VoteIntent::AddMyVote,
            vec!["U9", "U8", "U1"],
        ),
        (
            vec!["U9", "U1", "U8"],
            VoteIntent::RemoveMyVote,
            vec!["U9", "U8"],
        ),
        (vec!["U1"], VoteIntent::RemoveMyVote, vec![]),
    ] {
        let p = cell(json!(before));
        let old = p.clone();
        let e = edit(&d, &p, "U1", intent).unwrap();
        assert_eq!(
            e.cells(),
            json!([{"row_id":"R1","column_id":"C2","user":wanted}])
        );
        let request = api::list_cell_update(&SlackClient::default(), &session(), &e);
        assert!(!request.retry_safe());
        let RequestBody::Form(form) = request.body else {
            panic!("form")
        };
        assert_eq!(
            form.iter().map(|x| x.0.as_str()).collect::<Vec<_>>(),
            ["token", "list_id", "cells", "_x_reason"]
        );
        assert_eq!(form[2].1, e.cells().to_string());
        assert_eq!(form[3].1, "bulk-update-cells");
        assert!(e.readback_matches(&d, &cell(json!(wanted))));
        assert_eq!(p, old);
        assert!(
            CellEdit::capture(
                &d,
                &p,
                "R1",
                "C2",
                Some("full"),
                Some(&ordinary()),
                CellEditValue::Votes(vec!["U_OTHER".into()])
            )
            .is_err()
        );
        assert!(
            CellEdit::capture(
                &d,
                &p,
                "R1",
                "C2",
                Some("full"),
                Some(&ordinary()),
                CellEditValue::Users(vec!["U_OTHER".into()])
            )
            .is_err()
        );
    }
}

#[test]
fn missing_empty_noop_and_known_different_are_not_optimistic_confirmation() {
    let d = doc(VoteOptions::default().wire());
    for p in [page(json!([])), cell(json!([]))] {
        let old = p.clone();
        assert_eq!(
            read(d.column("C2").unwrap(), &p.records[0]).unwrap(),
            Vec::<String>::new()
        );
        assert!(edit(&d, &p, "U1", VoteIntent::RemoveMyVote).is_err());
        let e = edit(&d, &p, "U1", VoteIntent::AddMyVote).unwrap();
        assert_eq!(e.readback_value(&d, &p), Some(CellEditValue::Votes(vec![])));
        assert!(!e.readback_matches(&d, &p));
        assert!(!e.readback_matches(&d, &cell(json!(["U2"]))));
        assert!(e.readback_matches(&d, &cell(json!(["U1"]))));
        assert_eq!(e.readback_value(&d, &cell(json!(null))), None);
        assert_eq!(p, old);
    }
    assert!(edit(&d, &cell(json!(["U1"])), "U1", VoteIntent::AddMyVote).is_err());
}

#[test]
fn malformed_ambiguous_or_unknown_vote_refuses_without_raw_repair() {
    let d = doc(VoteOptions::default().wire());
    for raw in [
        json!(null),
        json!("U1"),
        json!([true]),
        json!([1]),
        json!([""]),
        json!(["U1|U2"]),
        json!([" U1"]),
        json!(["U1", "U1"]),
    ] {
        let p = cell(raw);
        assert!(edit(&d, &p, "U2", VoteIntent::AddMyVote).is_err());
    }
    for fields in [
        json!([{"key":"support"}]),
        json!([{"key":"support","user":[],"number":[1]}]),
        json!([{"key":"other","column_id":"C2","user":["U1"]}]),
        json!([{"key":"support","column_id":"WRONG","user":[]}]),
    ] {
        assert!(edit(&d, &page(fields), "U1", VoteIntent::AddMyVote).is_err());
    }
    assert!(
        edit(
            &doc(json!({"emoji":":heart:","anonymous":true})),
            &page(json!([])),
            "U1",
            VoteIntent::AddMyVote
        )
        .is_err()
    );
    for user in ["", "U1|U2", " U1", "U1\n"] {
        assert!(edit(&d, &page(json!([])), user, VoteIntent::AddMyVote).is_err());
    }
}

#[test]
fn exact_preflight_refuses_changed_voters_metadata_policy_and_archived_records() {
    let d = doc(VoteOptions::default().wire());
    let p = cell(json!(["U9"]));
    let e = edit(&d, &p, "U1", VoteIntent::AddMyVote).unwrap();
    e.revalidate(&d, &p, Some("full"), Some(&ordinary()))
        .unwrap();
    assert!(
        e.revalidate(
            &d,
            &cell(json!(["U9", "U8"])),
            Some("full"),
            Some(&ordinary())
        )
        .is_err()
    );
    let changed = doc(VoteOptions::capture(":heart:", None).unwrap().wire());
    assert!(
        e.revalidate(&changed, &p, Some("full"), Some(&ordinary()))
            .is_err()
    );
    assert!(
        e.readback_value(&changed, &cell(json!(["U9", "U1"])))
            .is_none()
    );
    assert!(e.revalidate(&d, &p, None, Some(&ordinary())).is_err());
    assert!(e.revalidate(&d, &p, Some("full"), None).is_err());
    let mut archived = p.raw.clone();
    archived["records"][0]["archived"] = json!(true);
    assert!(
        edit(
            &d,
            &RecordsPage::parse(archived).unwrap(),
            "U1",
            VoteIntent::AddMyVote
        )
        .is_err()
    );
    let mut changed = p.raw.clone();
    changed["records"][0]["opaque"]["keep"] = json!(10);
    assert!(
        e.revalidate(
            &d,
            &RecordsPage::parse(changed).unwrap(),
            Some("full"),
            Some(&ordinary())
        )
        .is_err()
    );
}

#[test]
fn local_bound_is_explicit_and_removal_remains_available_at_capacity() {
    let d = doc(VoteOptions::default().wire());
    let voters = (0..MAX_VOTERS).map(|n| format!("U{n}")).collect::<Vec<_>>();
    let p = cell(json!(voters));
    assert!(edit(&d, &p, "U_NEW", VoteIntent::AddMyVote).is_err());
    assert_eq!(
        edit(&d, &p, "U1", VoteIntent::RemoveMyVote)
            .unwrap()
            .cells()[0]["user"]
            .as_array()
            .unwrap()
            .len(),
        MAX_VOTERS - 1
    );
}
