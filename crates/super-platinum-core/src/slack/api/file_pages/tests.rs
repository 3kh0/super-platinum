use super::*;
use crate::slack::client::RequestBody;
use serde_json::json;
fn workspace() -> WorkspaceSession {
    WorkspaceSession {
        team_id: "TONE".into(),
        user_id: "UONE".into(),
        name: "Synthetic".into(),
        url: "https://fixture.slack.com".into(),
        token: "synthetic-token".into(),
        enterprise_id: None,
    }
}
fn input(sort: Sort) -> Input<'static> {
    Input {
        query: "design notes",
        page: 2,
        query_session: Uuid::from_u128(1),
        search_session: Uuid::from_u128(2),
        sort,
        exclude_bots: Some(false),
        only_my_channels: None,
        primary_channel: "CONE",
    }
}
#[test]
fn five_server_sorts_keep_created_distinct_raw_selector_and_present_false_without_people_fields() {
    for (sort, wire, direction, raw) in [
        (Sort::Relevant, "score", "desc", "score"),
        (Sort::Newest, "created", "desc", "created"),
        (Sort::Oldest, "timestamp", "asc", "timestamp_asc"),
        (Sort::Name, "name", "asc", "name"),
        (Sort::NameDescending, "name", "desc", "name_desc"),
    ] {
        let r = request(&SlackClient::default(), &workspace(), input(sort)).unwrap();
        assert_eq!(r.inner.method, "POST");
        assert!(
            r.inner
                .url
                .split('?')
                .next()
                .unwrap()
                .ends_with("/api/search.modules.files")
        );
        assert!(
            !r.inner
                .headers
                .iter()
                .any(|(k, _)| k.eq_ignore_ascii_case("content-type"))
        );
        let RequestBody::Form(fields) = r.inner.body else {
            panic!("closed scalar fields")
        };
        let map = fields.iter().cloned().collect::<BTreeMap<_, _>>();
        assert_eq!(map.len(), fields.len());
        for (key, value) in [
            ("token", "synthetic-token"),
            ("module", "files"),
            ("query", "design notes"),
            ("page", "2"),
            ("count", "20"),
            ("sort", wire),
            ("sort_dir", direction),
            ("search_tab_sort", raw),
            ("search_tab_filter", "files"),
            ("search_context", "desktop_files_tab"),
            ("search_only_team", ""),
            ("facets_result_count", "5"),
            ("max_filter_suggestions", "10"),
            ("search_exclude_bots", "false"),
        ] {
            assert_eq!(map[key], value);
        }
        assert!(!map.contains_key("search_only_my_channels"));
        assert!(!map.contains_key("account_type"));
        assert!(!map.contains_key("hide_deactivated_users"));
        assert_eq!(
            serde_json::from_str::<Value>(&map["request_context"]).unwrap(),
            json!({"active_cid":"CONE","recent_filter_in":[],"recent_filter_from":[]})
        );
        assert_eq!(map["client_req_id"], Uuid::from_u128(1).to_string());
        assert_eq!(map["search_session_id"], Uuid::from_u128(2).to_string());
    }
    let mut w = workspace();
    w.url = "https://fixture.slack.com.evil.test".into();
    assert!(request(&SlackClient::default(), &w, input(Sort::Newest)).is_err());
    for page in [0, MAX_PAGE + 1] {
        let mut i = input(Sort::Newest);
        i.page = page;
        assert!(request(&SlackClient::default(), &workspace(), i).is_err());
    }
}
#[test]
fn partial_file_hits_keep_occurrences_escaped_display_and_exact_page_metadata_without_file_authority()
 {
    let raw = json!({"ok":true,"items":[{"id":"FONE","search_title":"<b>Design</b>","url_private":"https://untrusted.invalid","user":"UONE","size":42},{"id":"FONE","name":"second occurrence"}],"pagination":{"page":2,"page_count":4,"total_count":0,"last":0}});
    let page = Page::decode(raw.clone(), 2).unwrap();
    assert_eq!(page.hits().len(), 2);
    assert_eq!(page.hits()[0].id(), page.hits()[1].id());
    assert_eq!(page.hits()[0].title(), "<b>Design</b>");
    assert_eq!(page.hits()[1].title(), "second occurrence");
    assert_eq!(page.total(), 0);
    assert_eq!(page.last(), Some(&json!(0)));
    assert!(Page::decode(raw.clone(), 1).is_err());
    for field in ["page", "page_count", "total_count"] {
        let mut bad = raw.clone();
        bad["pagination"].as_object_mut().unwrap().remove(field);
        assert!(Page::decode(bad, 2).is_err());
    }
    let mut bad = raw;
    bad["items"][0]["size"] = json!(-1);
    assert!(Page::decode(bad, 2).is_err());
    assert!(Page::decode(json!({"ok":true,"files":{"matches":[]},"pagination":{"page":1,"page_count":1,"total_count":0}}),1).is_err());
    assert!(
        Page::decode(
            json!({"ok":true,"items":[],"pagination":{"page":1,"page_count":0,"total_count":0}}),
            1
        )
        .is_ok()
    );
}
