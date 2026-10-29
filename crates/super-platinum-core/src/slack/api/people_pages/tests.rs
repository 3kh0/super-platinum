use super::*;
use crate::slack::client::RequestBody;
use serde_json::json;

fn workspace() -> WorkspaceSession {
    WorkspaceSession {
        team_id: "TONE".into(),
        user_id: "UONE".into(),
        name: "Synthetic".into(),
        url: "https://fixture.slack.com".into(),
        token: "synthetic-people-token".into(),
        enterprise_id: None,
    }
}
fn input(sort: Sort) -> Input<'static> {
    Input {
        query: "Elodie",
        page: 2,
        query_session: Uuid::from_u128(1),
        search_session: Uuid::from_u128(2),
        sort,
        hide_deactivated: false,
        primary_channel: "CONE",
    }
}

#[test]
fn literal_people_request_keeps_native_identity_and_source_text_values_without_generic_form_changes()
 {
    let client = SlackClient::default();
    let session = workspace();
    for (sort, wire, direction) in [
        (Sort::Relevant, "relevant", "desc"),
        (Sort::Name, "name", "asc"),
        (Sort::NameDescending, "name", "desc"),
    ] {
        let request = request(&client, &session, input(sort)).unwrap();
        let url = url::Url::parse(&request.inner.url).unwrap();
        assert_eq!(url.host_str(), Some("fixture.slack.com"));
        assert_eq!(url.path(), "/api/search.modules.people");
        assert!(
            !request
                .inner
                .headers
                .iter()
                .any(|(k, _)| k.eq_ignore_ascii_case("content-type"))
        );
        let RequestBody::Form(fields) = &request.inner.body else {
            panic!("private text fields")
        };
        assert_eq!(
            fields.first(),
            Some(&("token".into(), "synthetic-people-token".into()))
        );
        let map: BTreeMap<_, _> = fields
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        assert_eq!(map.len(), fields.len());
        assert_eq!(map["module"], "people");
        assert_eq!(map["page"], "2");
        assert_eq!(map["count"], "20");
        assert_eq!(map["sort"], wire);
        assert_eq!(map["sort_dir"], direction);
        assert_eq!(map["hide_deactivated_users"], "0");
        assert_eq!(map["file_title_only"], "false");
        assert_eq!(map["query_rewrite_disabled"], "false");
        assert_eq!(map["custom_fields"], "{}");
        assert_eq!(map["tz"], "");
        assert_eq!(
            serde_json::from_str::<Value>(map["request_context"]).unwrap(),
            json!({"active_cid":"CONE","recent_filter_in":[],"recent_filter_from":[]})
        );
        assert!(
            !map.contains_key("cursor")
                && !map.contains_key("account_type")
                && !map.contains_key("enterprise_token")
        );
        let ordinary = client.rest_form(&session, "users.info", vec![("user", "UOTHER".into())]);
        assert!(
            ordinary
                .headers
                .iter()
                .any(|(k, v)| k.eq_ignore_ascii_case("content-type")
                    && v == "application/x-www-form-urlencoded")
        );
    }
    let mut hostile = workspace();
    hostile.url = "https://fixture.slack.com.attacker.test".into();
    assert!(request(&client, &hostile, input(Sort::Relevant)).is_err());
    for query in ["", "\n", "two\r\nlines"] {
        let mut value = input(Sort::Relevant);
        value.query = query;
        assert!(request(&client, &session, value).is_err());
    }
    assert!(Sort::parse("score").is_none() && Sort::parse("timestamp").is_none());
}

#[test]
fn partial_people_pages_preserve_server_order_duplicates_and_opaque_metadata_without_users() {
    let raw = json!({"ok":true,"items":[{"id":"UZ","iid":"z","profile":{"display_name":"<b>Zed</b>","title":"not canonical"}},{"id":"UA","iid":"a1"},{"id":"UA","iid":"a2"}],"pagination":{"page":1,"page_count":2,"total_count":40,"last":3},"request_id":"original-one","query":"rewritten","extra":{"preserve":true}});
    let page = Page::decode(raw.clone(), 1).unwrap();
    assert_eq!(
        page.hits().iter().map(Hit::id).collect::<Vec<_>>(),
        ["UZ", "UA", "UA"]
    );
    assert_eq!(page.hits()[0].highlight(), Some("<b>Zed</b>"));
    assert_eq!(page.hits()[1].iid(), Some(&json!("a1")));
    assert_eq!(page.hits()[2].iid(), Some(&json!("a2")));
    assert_eq!(page.request_id(), Some("original-one"));
    assert_eq!(page.raw(), &raw);
    assert_eq!((page.number(), page.page_count(), page.total()), (1, 2, 40));
    let empty = Page::decode(
        json!({"ok":true,"items":[],"pagination":{"page":1,"page_count":0,"total_count":0}}),
        1,
    )
    .unwrap();
    assert!(empty.hits().is_empty());
}

#[test]
fn malformed_or_foreign_page_is_not_a_successful_empty_page_or_partial_publication() {
    let base = json!({"ok":true,"items":[{"id":"UONE"}],"pagination":{"page":2,"page_count":2,"total_count":40}});
    assert!(Page::decode(base.clone(), 1).is_err());
    for (pointer, value) in [
        ("/ok", json!(false)),
        ("/items", json!({})),
        ("/items/0/id", json!("")),
        ("/items/0/id", json!("U/bad")),
        ("/items/0/profile", json!(false)),
        ("/items/0/iid", json!({"unsafe":true})),
        ("/pagination/page", json!("2")),
        ("/pagination/page_count", json!(0)),
        ("/pagination/total_count", json!(-1)),
    ] {
        let mut raw = base.clone();
        if let Some(slot) = raw.pointer_mut(pointer) {
            *slot = value;
        } else {
            let key = pointer.rsplit('/').next().unwrap();
            raw["items"][0][key] = value;
        }
        assert!(Page::decode(raw, 2).is_err(), "{pointer}");
    }
}
