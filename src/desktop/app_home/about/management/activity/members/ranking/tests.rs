use super::*;
use serde_json::json;
use super_platinum_core::slack::models::Channel;

#[test]
fn native_rank_facts_use_actual_dm_identity_and_retain_changed_native_values() {
    let (mut s, visit) = super::super::tests::setup();
    let q = Query::capture(&mut s, visit).unwrap();
    let team = q.owner.ctx.session.team_id.clone();
    let peer = "U12345678";
    let rows = vec![json!({"id":peer,"profile":{"real_name":"Source name"}})];
    let before = native(&s, &q, &rows).unwrap();
    assert!(before["ims"][peer].is_null());
    assert!(before["metadata"][peer].is_null());

    let channel: Channel = serde_json::from_value(json!({
        "id":"D12345678","is_im":true,"user":peer,"unread_count":2
    }))
    .unwrap();
    s.core
        .workspaces
        .get_mut(&team)
        .unwrap()
        .channels
        .insert(channel.id.clone(), channel);
    let observed = native(&s, &q, &rows).unwrap();
    assert_eq!(observed["ims"][peer]["id"], "D12345678");
    assert_eq!(observed["metadata"]["D12345678"]["unread"], true);
    assert!(observed["metadata"][peer].is_null());
    let messages = s
        .core
        .workspaces
        .get_mut(&team)
        .unwrap()
        .messages
        .entry("D12345678".into())
        .or_default();
    messages.latest = Some("2.000001".into());
    messages.last_read = Some("1.000001".into());
    messages.unread_count = 0;
    let changed = native(&s, &q, &rows).unwrap();
    assert_ne!(changed, observed);
    assert_eq!(changed["metadata"]["D12345678"]["latest"], "2.000001");
    assert_eq!(changed["metadata"]["D12345678"]["unread"], false);

    let duplicate: Channel =
        serde_json::from_value(json!({"id":"D87654321","is_im":true,"user":peer})).unwrap();
    s.core
        .workspaces
        .get_mut(&team)
        .unwrap()
        .channels
        .insert(duplicate.id.clone(), duplicate);
    assert!(native(&s, &q, &rows).is_err());
}

#[test]
fn native_users_are_comparison_witnesses_and_assignment_errors_are_not_defaults() {
    let (mut s, visit) = super::super::tests::setup();
    let q = Query::capture(&mut s, visit).unwrap();
    let rows =
        vec![json!({"id":q.owner.ctx.session.user_id,"profile":{"real_name":"Actual source"}})];
    let before = native(&s, &q, &rows).unwrap();
    let workspace = s
        .core
        .workspaces
        .get_mut(&q.owner.ctx.session.team_id)
        .unwrap();
    workspace
        .users
        .get_mut(&q.owner.ctx.session.user_id)
        .unwrap()
        .extra
        .insert("source_test_change".into(), json!(1));
    assert_ne!(native(&s, &q, &rows).unwrap(), before);
    assert_eq!(rows[0]["profile"]["real_name"], "Actual source");
    assert!(assignment(&json!({"ok":false,"assignments":{}}), "missing").is_err());
    assert!(assignment(&json!({"ok":true}), "missing").is_err());
    assert!(
        assignment(
            &json!({"ok":true,"assignments":{"key":{"group":false}}}),
            "key"
        )
        .is_err()
    );
    assert_eq!(
        assignment(&json!({"ok":true,"assignments":{}}), "missing").unwrap(),
        None
    );
}

#[test]
fn lazy_draft_refresh_cannot_replace_local_draft_or_other_native_seed_facts() {
    let before = json!({"metadata":{"D12345678":{"draft":false,"localDraft":false,"latest":"1.0"}},"users":{"U12345678":{"name":"Original"}}});
    let mut shared = before.clone();
    shared["metadata"]["D12345678"]["draft"] = json!(true);
    assert_eq!(without_drafts(&before), without_drafts(&shared));
    shared["metadata"]["D12345678"]["localDraft"] = json!(true);
    assert_ne!(without_drafts(&before), without_drafts(&shared));
    let mut newer = before.clone();
    newer["metadata"]["D12345678"]["latest"] = json!("2.0");
    assert_ne!(without_drafts(&before), without_drafts(&newer));
    newer = before.clone();
    newer["users"]["U12345678"]["name"] = json!("New name");
    assert_ne!(without_drafts(&before), without_drafts(&newer));
}
