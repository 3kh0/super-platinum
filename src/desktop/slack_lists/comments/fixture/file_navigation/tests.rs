use super::*;

#[test]
fn received_whole_tokens_retain_original_identity_style_and_opaque_fields() {
    let original = received_message();
    let state = Some(State {
        received: original.clone(),
        ..Default::default()
    });
    let mut page = super::super::reply(ROOT, false);
    let root = page["messages"][0].clone();
    decorate(&state, &mut page);
    assert_eq!(page["messages"][0], root);
    assert_eq!(page["messages"][1], original);
    assert_eq!(original["ts"], RECEIVED_TS);
    assert_eq!(original["thread_ts"], ROOT);
    let elements = original["blocks"][0]["elements"][0]["elements"]
        .as_array()
        .unwrap();
    assert_eq!(elements[0]["type"], "canvas");
    assert_eq!(elements[0]["file_id"], CANVAS);
    assert_eq!(elements[0]["style"]["bold"], true);
    assert_eq!(elements[0]["navigation_opaque"]["occurrence"], 1);
    assert_eq!(elements[2]["type"], "file");
    assert_eq!(elements[2]["file_id"], TARGET_LIST);
    assert_eq!(elements[2]["style"]["italic"], true);
    assert_eq!(elements[2]["navigation_opaque"]["occurrence"], 2);
    let mut later = super::super::reply(ROOT, true);
    let before = later.clone();
    decorate(&state, &mut later);
    assert_eq!(later, before);
}

#[test]
fn target_list_is_independent_readonly_canonical_document_and_records() {
    let search = source_file(TARGET_LIST).unwrap();
    let raw = list_document().unwrap();
    assert_eq!(raw["fixture_opaque"], search["fixture_opaque"]);
    assert_eq!(raw["title"], completion::files::LIST_LABEL);
    assert_eq!(raw["access"], "read");
    assert_eq!(raw["is_locked"], true);
    let doc = super_platinum_core::slack_lists::Document::parse(raw).unwrap();
    assert_eq!(doc.id, TARGET_LIST);
    let rows = api::parse_list_records(list_records(), TARGET_LIST).unwrap();
    assert_eq!(rows.records.len(), 1);
    assert_eq!(rows.records[0].raw["list_id"], TARGET_LIST);
    assert_eq!(rows.records[0].raw["fields"][0]["text"], LIST_CONTENT);
    assert!(api::parse_list_records(list_records(), LIST).is_err());
    assert_eq!(source_file(TARGET_LIST).unwrap(), search);
}

#[test]
fn target_read_factories_keep_exact_destination_and_read_only_methods() {
    let owner = WorkspaceSession {
        team_id: "T1".into(),
        user_id: completion::SELF.into(),
        url: "https://list-navigation-fixture.slack.com".into(),
        ..super::super::session()
    };
    let client = super_platinum_core::slack::SlackClient::default();
    let info = api::file_info(&client, &owner, TARGET_LIST);
    let rows = api::lists_records_list(&client, &owner, TARGET_LIST);
    assert_eq!(
        url::Url::parse(&info.url).unwrap().path(),
        "/api/files.info"
    );
    assert_eq!(
        url::Url::parse(&rows.url).unwrap().path(),
        "/api/lists.records.list"
    );
    let RequestBody::Form(body) = rows.body else {
        panic!("form")
    };
    assert!(body.contains(&("list_id".into(), TARGET_LIST.into())));
    let load = api::canvas_collab_load_request(&client, &owner, "navigation-thread").unwrap();
    assert_eq!(
        url::Url::parse(&load.url).unwrap().path(),
        "/canvas/-/load-data"
    );
    assert!(source_file("FFOREIGN01").is_err());
}
