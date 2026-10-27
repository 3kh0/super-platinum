use super::*;
use serde_json::json;
use super_platinum_core::slack_lists::{Document, Record};
fn target(extra: Value) -> Target {
    let mut raw = json!({"id":"F01234567","mode":"list","list_metadata":{"schema":[],"views":[]}});
    raw.as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    let doc = Document::parse(raw).unwrap();
    let record =
        Record::parse(json!({"id":"Rec1","thread_ts":"1700000000.000001","fields":[]})).unwrap();
    Target::capture(&doc, &record).unwrap()
}
fn channel() -> Value {
    json!({"id":"C01234567","is_file":true,"is_channel":true})
}
fn root_value() -> Value {
    json!({"channelId":"C01234567","rootTs":"1700000000.000001","root":{"ts":"1700000000.000001","subtype":"list_record_comment","slack_list":{"list_id":"F01234567","list_record_id":"Rec1"}}})
}
#[test]
fn only_exact_known_ordinary_file_channel_is_admitted() {
    assert_eq!(
        facts::channel(&json!({"ok":true,"channels":[channel()]}), "C01234567").unwrap(),
        channel()
    );
    for raw in [
        json!({"ok":false,"channels":[channel()]}),
        json!({"ok":true,"channels":[]}),
        json!({"ok":true,"channels":[channel(),channel()]}),
        json!({"ok":true,"channels":[{"id":"COTHER","is_file":true}]}),
        json!({"ok":true,"channels":[{"id":"C01234567"}]}),
    ] {
        assert!(facts::channel(&raw, "C01234567").is_err());
    }
    for field in ["isUnknown", "isNonExistent", "is_im", "is_mpim"] {
        let mut row = channel();
        row[field] = json!(true);
        assert!(facts::channel(&json!({"ok":true,"channels":[row]}), "C01234567").is_err());
    }
}
#[test]
fn acquired_list_file_branch_does_not_use_edit_mode_window_access_or_locked_as_grant() {
    for extra in [
        json!({}),
        json!({"access":"read","is_locked":true}),
        json!({"msg_edit_window_mins":0,"plain_text_mode":true}),
        json!({"filetype":"list"}),
    ] {
        assert!(!facts::read_only_file_thread(&channel(), &target(extra)).unwrap());
    }
    assert!(
        facts::read_only_file_thread(&channel(), &target(json!({"filetype":"thread_share"})))
            .unwrap()
    );
    assert!(
        facts::read_only_file_thread(
            &channel(),
            &target(json!({"mimetype":"application/vnd.slack-thread-share"}))
        )
        .unwrap()
    );
    // Polls are refused before a List comment Target can be acquired.
    let mut poll = target(json!({})).document_raw().clone();
    poll["subtype"] = json!("poll");
    assert!(Document::parse(poll).is_err());
    let mut foreign = channel();
    foreign["id"] = json!("COTHER");
    // Identity validation belongs to the channel response; no foreign response admits.
    assert!(facts::channel(&json!({"ok":true,"channels":[foreign]}), "C01234567").is_err());
}
#[test]
fn original_root_requires_exact_item_identity_and_no_locked_or_deleted_source() {
    let target = target(json!({}));
    let root = root_value();
    facts::root(&root, &target).unwrap();
    for field in [
        "is_locked",
        "is_deleted",
        "is_tombstoned",
        "isUnknown",
        "isNonExistent",
    ] {
        let mut raw = root.clone();
        raw["root"][field] = json!(true);
        assert!(facts::root(&raw, &target).is_err(), "{field}");
    }
    for path in ["list_id", "list_record_id"] {
        let mut raw = root.clone();
        raw["root"]["slack_list"][path] = json!("OTHER");
        assert!(facts::root(&raw, &target).is_err());
    }
    let mut raw = root.clone();
    raw["root"] = Value::Null;
    assert!(facts::root(&raw, &target).is_err());
    raw = root;
    raw["channelId"] = json!("COTHER");
    assert!(facts::root(&raw, &target).is_err());
}
#[test]
fn ordinary_client_permission_does_not_elevate_admin_or_owner_actor() {
    let mut shell = super::super::tests::shell();
    let team = shell.core.active_team.clone().unwrap();
    // Real actor role fields do not represent a Slack admin-site page context.
    let user = shell
        .core
        .workspaces
        .get_mut(&team)
        .unwrap()
        .users
        .values_mut()
        .next();
    if let Some(user) = user {
        user.extra.insert("is_admin".into(), json!(true));
        user.extra.insert("is_owner".into(), json!(true));
    }
    let session = WorkspaceSession {
        team_id: "T1".into(),
        user_id: "U1".into(),
        enterprise_id: Some("E1".into()),
        name: "Fixture".into(),
        url: "https://fixture.slack.com".into(),
        token: "fixture-only".into(),
    };
    let q = native_client_permission_query(&session, &target(json!({}))).unwrap();
    assert_eq!(
        q.body(),
        json!({"user_id":"U1","channel_id":"C01234567","permissions":["DELETE_OWN_MESSAGE"]})
    );
    assert_eq!(q.edge_scope(), "E1");
    assert_eq!(q.enterprise_scope(), Some("E1"));
}
#[test]
fn source_flags_do_not_create_facts_from_missing_objects_or_malformed_values() {
    assert!(!facts::flag(&json!({}), "isExternal").unwrap());
    assert!(facts::flag(&json!({"isExternal":true}), "isExternal").unwrap());
    for raw in [
        Value::Null,
        json!([]),
        json!({"isExternal":"false"}),
        json!({"isExternal":0}),
    ] {
        assert!(facts::flag(&raw, "isExternal").is_err());
    }
}
#[test]
fn original_owner_socket_metadata_revision_and_target_are_required() {
    let mut shell = super::super::tests::shell();
    let e = shell.slack_lists.comments.entry.as_ref().unwrap();
    let owner = e.owner.clone();
    let target = e.target.clone();
    let socket = shell.core.workspaces[&owner.scope.team].rt_generation;
    let revision = crate::app_home::autocomplete::preferences_revision(&shell);
    assert!(active(&shell, &owner, &target, socket, revision));
    assert!(!active(&shell, &owner, &target, socket + 1, revision));
    assert!(!active(&shell, &owner, &target, socket, revision + 1));
    controller::close_shell(&mut shell);
    assert!(!active(&shell, &owner, &target, socket, revision));
}
#[tokio::test]
async fn retired_owner_never_polls_policy_work_or_accepts_late_result() {
    use std::cell::Cell;
    let polled = Cell::new(false);
    assert!(
        guarded_read(|| false, async {
            polled.set(true);
            Ok(json!({"ok":true}))
        })
        .await
        .is_err()
    );
    assert!(!polled.get());
    let current = Cell::new(true);
    assert!(
        guarded_read(|| current.get(), async {
            current.set(false);
            Ok(json!({"ok":true,"allowed":["DELETE_OWN_MESSAGE"]}))
        })
        .await
        .is_err()
    );
}
#[tokio::test]
async fn pending_policy_work_is_cancelled_after_original_owner_retires() {
    use std::cell::Cell;
    let current = Cell::new(true);
    let dropped = Cell::new(false);
    struct Retire<'a>(&'a Cell<bool>);
    impl Drop for Retire<'_> {
        fn drop(&mut self) {
            self.0.set(true);
        }
    }
    let result = tokio::time::timeout(
        Duration::from_secs(1),
        guarded_read(|| current.get(), async {
            let _retire = Retire(&dropped);
            current.set(false);
            std::future::pending::<Result<Value, Error>>().await
        }),
    )
    .await
    .unwrap();
    assert!(result.is_err());
    assert!(dropped.get());
}
