use super::*;
const ROOT: &str = "1700000000.000001";
const REPLY: &str = "1700000001.000001";

fn root(target: &Target) -> Value {
    json!({"type":"message","ts":ROOT,"subtype":"list_record_comment",
        "slack_list":{"list_id":target.list_id(),"list_record_id":target.record_id()},
        "reply_users":["U2","U1","U2"],"opaque":{"retained":true},"text":"Root"})
}
fn reply() -> Value {
    json!({"ts":REPLY,"thread_ts":ROOT,"text":"Reply","user":"NOT_A_PARTICIPANT"})
}
fn loaded(target: &Target, rows: Value) -> network::Loaded {
    network::Loaded {
        page: Page::parse(
            target,
            json!({"ok":true,"messages":rows,"has_more":true,
            "response_metadata":{"next_cursor":"next"}}),
        )
        .unwrap(),
        projection: Projection::observed(json!({"ok":true,"assignments":{}})).unwrap(),
    }
}
fn read(shell: &mut ShellState, rows: Value, paging: bool) {
    let (owner, serial, target, cursor) = controller::begin(shell, paging).unwrap();
    controller::finish(shell, &owner, serial, cursor, Ok(loaded(&target, rows)));
}
fn setup() -> ShellState {
    let mut shell = super::super::tests::shell();
    shell.core.workspaces.get_mut("T1").unwrap().rt_generation = 7;
    let target = shell
        .slack_lists
        .comments
        .entry
        .as_ref()
        .unwrap()
        .target
        .clone();
    read(&mut shell, json!([root(&target), reply()]), false);
    shell
}
fn frame(raw: Value, kind: &str) -> RtEvent {
    super_platinum_core::slack::realtime::parse_event(
        &json!({"type":"message",
        "subtype":kind,"channel":"CCOMMENT01","message":raw})
        .to_string(),
    )
    .unwrap()
}

#[test]
fn accepted_root_survives_rootless_pagination_with_order_duplicates_and_opaque_fields() {
    let mut shell = setup();
    let original = snapshot(&shell).unwrap();
    assert_eq!(original["threadUsers"], json!(["U2", "U1", "U2"]));
    read(&mut shell, json!([reply()]), true);
    assert_eq!(snapshot(&shell).unwrap(), original);
    assert!(
        shell
            .slack_lists
            .comments
            .entry
            .as_ref()
            .unwrap()
            .page
            .as_ref()
            .unwrap()
            .root()
            .is_none()
    );
}

#[test]
fn no_page_is_unavailable_but_accepted_missing_root_is_observed_null_without_authors() {
    let mut shell = super::super::tests::shell();
    assert!(snapshot(&shell).is_err());
    read(&mut shell, json!([reply()]), false);
    let observed = snapshot(&shell).unwrap();
    assert_eq!(observed["root"], Value::Null);
    assert_eq!(observed["threadUsers"], json!([]));
    assert!(!apply(
        &mut shell,
        "T1",
        0,
        &frame(json!({"ts":ROOT,"reply_users":["U1"]}), "message_replied")
    ));
}

#[test]
fn partial_replied_and_changed_merge_only_present_fields_and_retire_older_read() {
    let mut shell = setup();
    let prior = snapshot(&shell).unwrap();
    let page_raw = shell
        .slack_lists
        .comments
        .entry
        .as_ref()
        .unwrap()
        .page
        .as_ref()
        .unwrap()
        .raw()
        .clone();
    let rows = shell
        .slack_lists
        .comments
        .entry
        .as_ref()
        .unwrap()
        .rows
        .clone();
    let (owner, serial, target, cursor) = controller::begin(&mut shell, false).unwrap();
    let late = loaded(&target, page_raw["messages"].clone());
    assert!(apply(
        &mut shell,
        "T1",
        7,
        &frame(
            json!({"ts":ROOT,"reply_users":["U3","U1"]}),
            "message_replied"
        )
    ));
    assert!(!owner.reading(&shell, serial));
    let next = snapshot(&shell).unwrap();
    assert_eq!(next["threadUsers"], json!(["U3", "U1"]));
    assert_eq!(next["root"]["opaque"], prior["root"]["opaque"]);
    controller::finish(&mut shell, &owner, serial, cursor, Ok(late));
    assert_eq!(snapshot(&shell).unwrap(), next);
    let entry = shell.slack_lists.comments.entry.as_ref().unwrap();
    assert_eq!(entry.page.as_ref().unwrap().raw(), &page_raw);
    assert!(Arc::ptr_eq(&rows, &entry.rows));
    assert!(!apply(
        &mut shell,
        "T1",
        7,
        &frame(json!({"ts":ROOT,"text":"Renamed"}), "message_changed")
    ));
    assert_eq!(
        snapshot(&shell).unwrap()["threadUsers"],
        next["threadUsers"]
    );
    assert!(apply(
        &mut shell,
        "T1",
        7,
        &frame(json!({"ts":ROOT,"reply_users":[]}), "message_replied")
    ));
    assert_eq!(snapshot(&shell).unwrap()["threadUsers"], json!([]));
}

#[test]
fn foreign_or_unwitnessed_events_and_equal_root_do_not_retire_read() {
    let mut shell = setup();
    let before = snapshot(&shell).unwrap();
    let (owner, serial, _, _) = controller::begin(&mut shell, false).unwrap();
    let good = frame(json!({"ts":ROOT,"reply_users":["U3"]}), "message_replied");
    assert!(!apply(&mut shell, "T2", 7, &good));
    assert!(!apply(&mut shell, "T1", 8, &good));
    for raw in [
        json!({"ts":REPLY,"thread_ts":ROOT,"reply_users":["U3"]}),
        json!({"ts":ROOT,"channel":"COTHER","reply_users":["U3"]}),
        json!({"ts":ROOT,"thread_ts":REPLY,"reply_users":["U3"]}),
        json!({"ts":ROOT}),
        json!({"ts":ROOT,"text":"Ignored edit."}),
        json!({"ts":ROOT,"reply_users":["U3"],"subtype":"other"}),
        json!({"ts":ROOT,"reply_users":["U3"],"slack_list":{"list_id":"FOTHER"}}),
        json!({"ts":ROOT,"reply_users":["U3"],"slack_list":{"list_record_id":"OTHER"}}),
    ] {
        assert!(!apply(&mut shell, "T1", 7, &frame(raw, "message_changed")));
    }
    let RtEvent::MessageChanged {
        channel, message, ..
    } = good
    else {
        panic!()
    };
    assert!(!apply(
        &mut shell,
        "T1",
        7,
        &RtEvent::MessageChanged {
            channel,
            message,
            original_update: None
        }
    ));
    assert_eq!(snapshot(&shell).unwrap(), before);
    assert!(owner.reading(&shell, serial));
}

#[test]
fn malformed_original_participants_refuse_context_until_an_accepted_root_repairs_it() {
    let mut shell = setup();
    assert!(apply(
        &mut shell,
        "T1",
        7,
        &frame(json!({"ts":ROOT,"reply_users":[""]}), "message_changed")
    ));
    assert!(snapshot(&shell).is_err());
    read(&mut shell, json!([reply()]), true);
    assert!(snapshot(&shell).is_err());
    let target = shell
        .slack_lists
        .comments
        .entry
        .as_ref()
        .unwrap()
        .target
        .clone();
    read(&mut shell, json!([root(&target), reply()]), false);
    assert_eq!(
        snapshot(&shell).unwrap()["threadUsers"],
        json!(["U2", "U1", "U2"])
    );
    // Null follows raw-source ||[] semantics; the current typed parser rejects
    // null before producing a MessageUpdate, so this is a decoder-only check.
    assert!(
        thread_users(Some(&json!({"reply_users":null})))
            .unwrap()
            .is_empty()
    );
    assert!(thread_users(Some(&json!({"reply_users":false}))).is_err());
    assert!(thread_users(Some(&json!({"reply_users":[1]}))).is_err());
}

#[test]
fn socket_account_canonical_target_and_visit_replacement_refuse_old_source() {
    for mutation in 0..4 {
        let mut shell = setup();
        match mutation {
            0 => shell.core.workspaces.get_mut("T1").unwrap().rt_generation += 1,
            1 => shell.core.account_epoch += 1,
            2 => shell.slack_lists.view.as_mut().unwrap().revision += 1,
            _ => {
                controller::close_shell(&mut shell);
                let generation =
                    record_detail::presentation(shell.slack_lists.view.as_ref().unwrap())
                        .unwrap()
                        .generation;
                assert!(controller::open_shell(&mut shell, generation));
            }
        }
        assert!(snapshot(&shell).is_err());
    }
}
