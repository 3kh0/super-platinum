use super::*;

#[tokio::test]
async fn attachment_only_search_hits_publish_visible_text_and_keep_paged_navigation_identity() {
    let (_dom, mut state, transport, session) = setup("U_ATTACHMENT_AUTHOR", false);
    let client = SlackClient::default();
    let request = begin(state);
    let payload = json!({"ok":true,"items":[{"channel":{"id":"C2","name":"ship"},"messages":[
        {"user":"U_ATTACHMENT_AUTHOR","ts":"130.000001","thread_ts":"130.000000","attachments":[{
            "pretext":"Release notice","title":"Deploy report","title_link":"https://example.test/HIDDEN_TARGET",
            "text":"Service <https://example.test/HIDDEN_LINK|build console>","mrkdwn_in":["text","fields"],
            "fields":[{"title":"Checks","value":"*Passed*"}],
            "files":[{"id":"F_SEARCH_LOG","name":"build.log","title":"Build log"}],
            "fallback":"HIDDEN_FALLBACK"
        }]},
        {"user":"U_ATTACHMENT_AUTHOR","ts":"130.000002","text":"  Release \u{e000}caption\u{e001}","attachments":[{"text":"Do not append secondary text"}]},
        {"user":"U_ATTACHMENT_AUTHOR","ts":"130.000003","files":[{"id":"F_TOP","title":"Release archive","name":"release.zip"}]},
        {"user":"U_ATTACHMENT_AUTHOR","ts":"130.000004","files":[{"id":"F_CANVAS","filetype":"quip","title":"Release plan"}]},
        {"user":"U_ATTACHMENT_AUTHOR","ts":"130.000005","attachments":[{"fallback":"HIDDEN_PRIVATE_PLACEHOLDER"}]},
        {"user":"U_ATTACHMENT_AUTHOR","ts":"130.000006","attachments":[{
            "text":"HIDDEN_LEGACY_FALLBACK","blocks":[{"type":"section","text":{"type":"mrkdwn","text":"*Release* block body"}}]
        }]}
    ]}],"pagination":{"page":1,"page_count":2,"total_count":7}});
    load_with_profiles(
        state,
        request,
        &transport,
        &client,
        &session,
        None,
        |_| async move { Ok(payload) },
        |_| std::future::ready(Ok(Vec::new())),
    )
    .await;
    {
        let shell = state.peek();
        assert_eq!(shell.search_results.len(), 6);
        let texts = shell
            .search_results
            .iter()
            .map(|r| r.text.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            texts,
            [
                "Release notice\nDeploy report\nService build console\nChecks: Passed\nBuild log",
                "Release caption",
                "Release archive",
                "Release plan",
                "",
                "Release block body"
            ]
        );
        assert!(texts.iter().all(|text| !text.contains("HIDDEN_")));
        assert_eq!(
            shell.search_results[0].thread_ts.as_deref(),
            Some("130.000000")
        );
        assert!(shell.search_pager.has_more());
    }
    let request =
        crate::bootstrap::search::begin(&mut state.write(), "T1", "release", true).unwrap();
    let payload = json!({"ok":true,"items":[
        {"channel":{"id":"C2","name":"ship"},"messages":[{"user":"U_ATTACHMENT_AUTHOR","ts":"130.000001","text":"Overlapping hit must not replace its preview or thread target"}]},
        {"channel":{"id":"C_OTHER","name":"other"},"messages":[{"user":"U_ATTACHMENT_AUTHOR","ts":"130.000001","attachments":[{"text":"Other conversation result"}]}]}
    ],"pagination":{"page":2,"page_count":2,"total_count":7}});
    load_with_profiles(
        state,
        request,
        &transport,
        &client,
        &session,
        None,
        |request| async move {
            let RequestBody::Form(fields) = request.body else {
                panic!("form required")
            };
            assert!(
                fields
                    .iter()
                    .any(|(key, value)| key == "page" && value == "2")
            );
            Ok(payload)
        },
        |_| std::future::ready(Ok(Vec::new())),
    )
    .await;
    let shell = state.peek();
    assert_eq!(shell.search_results.len(), 7);
    assert_eq!(
        (
            shell.search_results[0].channel_id.as_str(),
            shell.search_results[0].ts.as_str(),
            shell.search_results[0].thread_ts.as_deref()
        ),
        ("C2", "130.000001", Some("130.000000"))
    );
    assert!(shell.search_results[0].text.starts_with("Release notice"));
    assert_eq!(
        (
            shell.search_results[6].channel_id.as_str(),
            shell.search_results[6].ts.as_str(),
            shell.search_results[6].text.as_str()
        ),
        ("C_OTHER", "130.000001", "Other conversation result")
    );
    assert_eq!((shell.search_pager.page, shell.search_pager.total), (2, 7));
    assert!(
        !shell.search_pager.failed && !shell.search_pager.loading && !shell.search_pager.has_more()
    );
    assert_eq!(shell.core.composer.text, "Keep draft");
}
