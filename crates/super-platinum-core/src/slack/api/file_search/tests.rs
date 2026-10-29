use super::*;
use crate::search::{SearchKind, SearchPager, SearchSort};
use serde_json::json;

fn session() -> WorkspaceSession {
    WorkspaceSession {
        team_id: "T1".into(),
        user_id: "U1".into(),
        name: "Fixture".into(),
        url: "https://example.test".into(),
        token: "synthetic-token".into(),
        enterprise_id: None,
    }
}

#[tokio::test]
async fn documented_file_response_preserves_files_and_normalizes_legacy_paging() {
    let query = "\"release notes\" in:ship from:me after:2026-10-01";
    let page = fetch_search_files_with(&SlackClient::default(), &session(), SearchArgs {
        query:query.into(),sort:SearchSort::Oldest,count:20,page:2,
    }, |request| async move {
        assert_eq!(url::Url::parse(&request.url).unwrap().path(),"/api/search.files");
        assert_eq!(request.method, "POST");
        let crate::slack::client::RequestBody::Form(fields) = request.body else {panic!("form required")};
        let fields=fields.into_iter().collect::<BTreeMap<_,_>>();
        assert_eq!(fields["query"],query);
        assert_eq!(fields["sort"],"timestamp");assert_eq!(fields["sort_dir"],"asc");
        assert_eq!(fields["page"],"2");assert_eq!(fields["count"],"20");
        assert_eq!(fields["highlight"],"false");
        Ok(json!({"ok":true,"files":{"matches":[{"id":"F1","title":"<Release> & notes","name":"release.txt","mimetype":"text/plain","filetype":"text","user":"U2","opaque":{"keep":true}}],
            "paging":{"page":2,"pages":3,"count":20,"total":42}}}))
    }).await.unwrap();
    assert_eq!(page.items[0].title.as_deref(), Some("<Release> & notes"));
    assert_eq!(page.items[0].extra["opaque"], json!({"keep":true}));
    let p = page.pagination.unwrap();
    assert_eq!(
        (p.page, p.page_count, p.per_page, p.total_count),
        (Some(2), Some(3), Some(20), Some(42))
    );
}

#[tokio::test]
async fn malformed_envelope_is_not_a_successful_empty_search_and_api_errors_survive() {
    for raw in [
        json!({"ok":true}),
        json!({"ok":true,"files":{}}),
        json!({"ok":true,"files":{"matches":null}}),
        json!({"ok":true,"files":{"matches":[false]}}),
    ] {
        assert!(
            fetch_search_files_with(
                &SlackClient::default(),
                &session(),
                SearchArgs::new("notes"),
                |_| async { Ok(raw) }
            )
            .await
            .is_err()
        );
    }
    let error = fetch_search_files_with(
        &SlackClient::default(),
        &session(),
        SearchArgs::new("notes"),
        |_| async { Ok(json!({"ok":false,"error":"missing_scope"})) },
    )
    .await
    .unwrap_err();
    assert!(matches!(error,Error::Api(ref code) if code=="missing_scope"));
    let error = fetch_search_files_with(
        &SlackClient::default(),
        &session(),
        SearchArgs::new("notes"),
        |_| async {
            Err(Error::RateLimited {
                retry_after_secs: Some(7),
            })
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(
        error,
        Error::RateLimited {
            retry_after_secs: Some(7)
        }
    ));
}

#[tokio::test]
async fn modern_pagination_and_empty_files_are_supported_without_message_shape() {
    let page=fetch_search_files_with(&SlackClient::default(),&session(),SearchArgs::new("notes"),|_|async {
        Ok(json!({"ok":true,"files":{"matches":[],"pagination":{"page":1,"page_count":1,"total_count":0}}}))
    }).await.unwrap();
    assert!(page.items.is_empty());
    assert_eq!(page.pagination.unwrap().total_count, Some(0));
}

#[test]
fn result_type_is_part_of_search_identity_and_ordinary_resets_keep_it() {
    let mut pager = SearchPager::default();
    let messages = pager.begin(1, "T1", "release", false).unwrap();
    assert!(pager.change_kind(SearchKind::Files));
    let files = pager.begin(1, "T1", "release", false).unwrap();
    assert!(!pager.fail(&messages));
    assert!(!pager.complete(&messages, None));
    assert!(pager.accepts(&files));
    assert!(pager.change_kind(SearchKind::Messages));
    pager.begin(1, "T1", "release", false).unwrap();
    assert!(!pager.accepts(&messages));
    assert!(!pager.accepts(&files));
    pager.change_kind(SearchKind::Files);
    pager.change_sort(SearchSort::Oldest);
    pager.reset();
    assert_eq!(pager.kind, SearchKind::Files);
    assert_eq!(pager.sort, SearchSort::Oldest);
}
