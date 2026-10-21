use super::*;

fn channel() -> Value {
    json!({"id":"CCHANNEL1","properties":{"channel_workflows":[
    {"workflow_trigger_id":"TR2","title":"Featured denied title","opaque":[1]},
    {"workflow_trigger_id":"TR1","title":"Featured stale title"}]}})
}
fn bookmarks() -> Value {
    json!({"ok":true,"opaque":"retained","bookmarks":[
        {"id":"Bk2","channel_id":"CCHANNEL1","rank":"b","type":"shortcut","shortcut_id":"TR1","title":"Never metadata"},
        {"id":"Bk1","rank":"a","type":"auto_gen_shortcut","shortcut_id":"TR3","parent_id":"BkFolder","title":"Also never metadata"},
        {"id":"BkDupe","rank":"c","type":"shortcut","shortcut_id":"TR3"},
        {"id":"BkLink","rank":"0","type":"link","shortcut_id":"NOT_A_WORKFLOW"},
        {"id":"BkLegacy","rank":"0","type":"legacy_workflow","shortcut_id":"LEGACY"}
    ]})
}
fn capture() -> Collections {
    Collections::capture(&channel(), &bookmarks()).unwrap()
}
fn previews() -> Value {
    json!({"ok":true,"opaque":{"response":true},"triggers":[
        {"id":"TR3","name":"","description":"","workflow":{"workflow_id":"WF3","title":"Actual fallback","description":"Actual fallback description","app":{"id":"A3","icons":{"image_36":"https://example.test/36.png"}}},"opaque":"preview"},
        {"id":"TR1","name":"  Actual &amp; name  ","description":"Actual description","workflow":{"title":"Not preferred"}},
        {"id":"TR2","error":"access_denied"}
    ]})
}

#[test]
fn raw_collections_preserve_feature_order_rank_and_folder_children_without_legacy_links() {
    let c = capture();
    assert_eq!(c.channel_raw(), &channel());
    assert_eq!(c.bookmarks_raw(), &bookmarks());
    assert_eq!(
        c.featured()
            .iter()
            .map(Reference::trigger_id)
            .collect::<Vec<_>>(),
        vec!["TR2", "TR1"]
    );
    assert_eq!(
        c.bookmarked()
            .iter()
            .map(Reference::trigger_id)
            .collect::<Vec<_>>(),
        vec!["TR3", "TR1"]
    );
    assert_eq!(c.preview_ids(), &["TR2", "TR1", "TR3"]);
    assert_eq!(c.bookmarked()[0].raw()["id"], "Bk1");
    assert_eq!(c.bookmarked()[0].raw()["parent_id"], "BkFolder");
    assert_eq!(c.bookmarked()[0].title(), None);
}
#[test]
fn actual_preview_metadata_never_uses_bookmark_titles_and_denied_feature_uses_source_title_only() {
    let c = capture();
    let p = Previews::decode(c.preview_ids(), previews()).unwrap();
    assert_eq!(p.raw(), &previews());
    assert!(p.all_displayable());
    let rows = p.project(&c).unwrap();
    assert_eq!(rows.featured()[0].title(), Some("Featured denied title"));
    assert_eq!(
        rows.featured()[0].preview().kind(),
        &PreviewKind::AccessDenied
    );
    assert_eq!(rows.featured()[0].preview().description(), None);
    assert_eq!(rows.featured()[1].title(), Some("  Actual &amp; name  "));
    assert_eq!(rows.bookmarked()[0].title(), Some("Actual fallback"));
    assert_eq!(
        rows.bookmarked()[0].preview().description(),
        Some("Actual fallback description")
    );
    assert_eq!(
        rows.bookmarked()[0].preview().app().unwrap()["icons"]["image_36"],
        "https://example.test/36.png"
    );
    let denied=Previews::decode(c.preview_ids(),json!({"ok":true,"triggers":c.preview_ids().iter().map(|id|json!({"id":id,"error":"access_denied"})).collect::<Vec<_>>()})).unwrap().project(&c).unwrap();
    assert_eq!(denied.bookmarked()[0].title(), None);
    assert_eq!(denied.bookmarked()[1].title(), None);
}
#[test]
fn per_item_failure_unknown_nonexistent_and_missing_are_not_empty_success() {
    let ids = vec!["TR1".into(), "TR2".into(), "TR3".into(), "TR4".into()];
    let p = Previews::decode(
        &ids,
        json!({"ok":true,"triggers":[
            {"id":"TR1","error":"not_found","opaque":8},
            {"id":"TR2","isUnknown":true}, {"id":"TR3","isNonExistent":true}
        ]}),
    )
    .unwrap();
    assert!(!p.all_displayable());
    assert_eq!(p.items().len(), 4);
    assert_eq!(p.items()[0].kind(), &PreviewKind::Error("not_found".into()));
    assert_eq!(p.items()[0].raw().unwrap()["opaque"], 8);
    assert_eq!(p.items()[1].kind(), &PreviewKind::Unknown);
    assert_eq!(p.items()[2].kind(), &PreviewKind::NonExistent);
    assert_eq!(p.items()[3].kind(), &PreviewKind::Missing);
    assert_eq!(p.items()[3].raw(), None);
    assert!(p.items().iter().all(|p| !p.displayable()));
}
#[test]
fn request_identity_reason_comma_order_and_native_limits_are_explicit() {
    let list = Read::bookmarks("CCHANNEL1").unwrap();
    assert_eq!(list.method(), "bookmarks.list");
    assert_eq!(list.reason(), "bookmarks-store/conditional-fetching");
    assert_eq!(
        list.arguments(),
        &json!({"channel":"CCHANNEL1","include_folders":true,"include_legacy_workflows":true})
    );
    let ids = capture().preview_ids().to_vec();
    let p = Read::previews(&ids).unwrap();
    assert_eq!(p.method(), "workflows.triggers.preview");
    assert_eq!(p.reason(), "hermes_trigger_previews_ufm");
    assert_eq!(p.arguments(), &json!({"trigger_ids":"TR2,TR1,TR3"}));
    let owner = WorkspaceSession {
        team_id: "TWORKFLOW".into(),
        enterprise_id: None,
        user_id: "UWORKFLOW".into(),
        name: "Fixture".into(),
        url: "https://fixture.test".into(),
        token: "synthetic".into(),
    };
    for read in [&list, &p] {
        let request = read.request(&SlackClient::default(), &owner);
        // rest_form appends the existing desktop xparams query after the path.
        let url = url::Url::parse(&request.url).unwrap();
        assert_eq!(url.scheme(), "https");
        assert_eq!(url.host_str(), Some("fixture.test"));
        assert_eq!(url.path(), format!("/api/{}", read.method()));
        assert_eq!(request.method, "POST");
        assert!(
            url.query_pairs()
                .any(|(key, value)| key == "_x_num_retries" && value == "0")
        );
        let crate::slack::client::RequestBody::Form(fields) = request.body else {
            panic!("expected form")
        };
        assert_eq!(
            fields.iter().find(|(k, _)| k == "_x_reason").unwrap().1,
            read.reason()
        );
        for (key, value) in read.arguments().as_object().unwrap() {
            let expected = value
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| value.to_string());
            assert_eq!(fields.iter().find(|(k, _)| k == key).unwrap().1, expected);
        }
    }
    assert!(Read::previews(&[]).is_err());
    assert!(Read::previews(&["TR1".into(), "TR1".into()]).is_err());
    assert!(Read::previews(&["TR1,TR2".into()]).is_err());
    let max = (0..MAX_PREVIEW_IDS)
        .map(|n| format!("TR{n}"))
        .collect::<Vec<_>>();
    assert!(Read::previews(&max).is_ok());
    let mut over = max;
    over.push("EXTRA".into());
    assert!(Read::previews(&over).is_err());
}
#[test]
fn malformed_failed_paginated_and_wrong_owner_inputs_never_manufacture_empty_collections() {
    for raw in [
        json!({}),
        json!({"ok":false,"error":"offline"}),
        json!({"ok":true}),
        json!({"ok":true,"bookmarks":null}),
        json!({"ok":true,"bookmarks":[],"has_more":true}),
        json!({"ok":true,"bookmarks":[],"response_metadata":{"next_cursor":"next"}}),
        json!({"ok":true,"bookmarks":[{"channel_id":"OTHER"}]}),
        json!({"ok":true,"bookmarks":[{"rank":{}}]}),
        json!({"ok":true,"bookmarks":[{"type":"shortcut","shortcut_id":null}]}),
    ] {
        assert!(Collections::capture(&channel(), &raw).is_err(), "{raw}");
    }
    for raw in [
        json!({"id":"CCHANNEL1","properties":false}),
        json!({"id":"CCHANNEL1","properties":{"channel_workflows":{}}}),
        json!({"id":"CCHANNEL1","properties":{"channel_workflows":[{"title":"Not identity"}]}}),
    ] {
        assert!(Collections::capture(&raw, &bookmarks()).is_err());
    }
    let c = Collections::capture(
        &json!({"id":"CCHANNEL1"}),
        &json!({"ok":true,"bookmarks":[]}),
    )
    .unwrap();
    assert!(c.preview_ids().is_empty());
    assert!(
        Previews::decode(&[], json!({"ok":true,"triggers":[]}))
            .unwrap()
            .project(&c)
            .unwrap()
            .featured()
            .is_empty()
    );
}
#[test]
fn preview_envelope_identity_and_shape_cannot_borrow_unrequested_metadata() {
    let ids = vec!["TR1".into()];
    for raw in [
        json!({"ok":false,"triggers":[]}),
        json!({"ok":true}),
        json!({"ok":true,"triggers":null}),
        json!({"ok":true,"triggers":[{"id":"OTHER","name":"Wrong"}]}),
        json!({"ok":true,"triggers":[{"id":"TR1"},{"id":"TR1"}]}),
        json!({"ok":true,"triggers":[{"id":"TR1","name":false}]}),
        json!({"ok":true,"triggers":[{"id":"TR1","workflow":[]}]}),
        json!({"ok":true,"triggers":[{"id":"TR1","error":{}}]}),
        json!({"ok":true,"triggers":[{"id":"TR1","isUnknown":"true"}]}),
    ] {
        assert!(Previews::decode(&ids, raw.clone()).is_err(), "{raw}");
    }
    let p = Previews::decode(&ids, json!({"ok":true,"triggers":[]})).unwrap();
    assert!(p.project(&capture()).is_err());
}
#[test]
fn stable_rank_order_preserves_numeric_null_missing_and_equal_rank_source_order() {
    let input = json!({"ok":true,"bookmarks":[
        {"type":"shortcut","shortcut_id":"MISSING"},
        {"type":"shortcut","shortcut_id":"NULL","rank":null},
        {"type":"shortcut","shortcut_id":"TEN","rank":10},
        {"type":"shortcut","shortcut_id":"TWO","rank":2},
        {"type":"shortcut","shortcut_id":"TIE","rank":2}]});
    let c = Collections::capture(&json!({"id":"C1"}), &input).unwrap();
    assert_eq!(c.preview_ids(), &["TWO", "TIE", "TEN", "NULL", "MISSING"]);
    assert_eq!(c.bookmarks_raw(), &input);
    let input = json!({"ok":true,"bookmarks":[
        {"type":"shortcut","shortcut_id":"TEN","rank":"10"},
        {"type":"shortcut","shortcut_id":"TWO","rank":"2"}]});
    assert_eq!(
        Collections::capture(&json!({"id":"C1"}), &input)
            .unwrap()
            .preview_ids(),
        &["TEN", "TWO"]
    );
}
