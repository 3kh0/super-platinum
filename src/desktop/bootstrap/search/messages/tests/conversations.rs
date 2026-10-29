use super::*;

#[tokio::test]
async fn cold_explicit_dm_peer_resolves_without_hash_or_guessed_membership_and_preserves_routes() {
    let (_dom, mut state, transport, session) = setup("U2", false);
    {
        let mut shell = state.write();
        let workspace = shell.core.workspaces.get_mut("T1").unwrap();
        for value in [
            json!({"id":"D_CACHED","is_im":true,"user":"U1"}),
            json!({"id":"G_CONFLICT","is_im":true,"user":"U_MUST_NOT_LOOKUP"}),
        ] {
            let channel: super_platinum_core::slack::models::Channel =
                serde_json::from_value(value).unwrap();
            workspace.channels.insert(channel.id.clone(), channel);
        }
    }
    let before = serde_json::to_value(&state.peek().core.workspaces["T1"].channels).unwrap();
    let client = SlackClient::default();
    let request = begin(state);
    let payload = json!({"ok":true,"items":[
        {"channel":{"id":"D_COLD","is_im":true,"user":"U_DM_PEER"},"messages":[{"user":"U2","ts":"200.000001","thread_ts":"200.000000","text":"Cold direct hit"}]},
        {"channel":{"id":"C2","is_channel":true,"name":"ship"},"messages":[{"user":"U2","ts":"200.000002","text":"Channel hit"}]},
        {"channel":{"id":"G_GROUP","is_mpim":true,"name":"mpdm-Maya.Chen--Jules-1"},"messages":[{"user":"U2","ts":"200.000003","text":"Group hit"}]},
        {"channel":{"id":"D_CACHED","name":"opaque"},"messages":[{"user":"U2","ts":"200.000004","text":"Cached classification"}]},
        {"channel":{"id":"G_CONFLICT","is_mpim":true,"name":"mpdm-group--members-1"},"messages":[{"user":"U2","ts":"200.000005","text":"Explicit group wins"}]},
        {"channel":{"id":"D_AMBIGUOUS","is_im":true,"name":"U_NAME_ONLY"},"messages":[{"user":"U2","ts":"200.000006","text":"No guessed peer"}]},
        {"channel":{"id":"D_EMPTY","is_im":true,"user":"U_EMPTY_ITEM"},"messages":[]}
    ],"pagination":{"page":1,"page_count":1,"total_count":6}});
    let calls = Rc::new(Cell::new(0));
    let observed = calls.clone();
    load_with_profiles(
        state,
        request,
        &transport,
        &client,
        &session,
        None,
        |_| async move { Ok(payload) },
        move |ids| {
            assert_eq!(ids, ["U_DM_PEER"]);
            observed.set(observed.get() + 1);
            std::future::ready(Ok(vec![user("U_DM_PEER", "Readable peer")]))
        },
    )
    .await;
    assert_eq!(calls.get(), 1);
    let shell = state.peek();
    assert_eq!(shell.search_results.len(), 6);
    let headings = shell
        .search_results
        .iter()
        .map(crate::view::search::result_heading)
        .collect::<Vec<_>>();
    assert_eq!(
        headings,
        [
            "Readable peer · Jules",
            "# ship · Jules",
            "Maya Chen, Jules · Jules",
            "Maya Chen · Jules",
            "group, members · Jules",
            "U_NAME_ONLY · Jules"
        ]
    );
    assert_eq!(
        shell
            .search_results
            .iter()
            .map(|r| r.direct_conversation)
            .collect::<Vec<_>>(),
        [true, false, true, true, true, true]
    );
    assert_eq!(
        shell
            .search_results
            .iter()
            .map(|r| r.channel_id.as_str())
            .collect::<Vec<_>>(),
        [
            "D_COLD",
            "C2",
            "G_GROUP",
            "D_CACHED",
            "G_CONFLICT",
            "D_AMBIGUOUS"
        ]
    );
    assert_eq!(
        shell.search_results[0].thread_ts.as_deref(),
        Some("200.000000")
    );
    assert_eq!(shell.search_results[0].ts, "200.000001");
    assert_eq!(shell.search_results[0].text, "Cold direct hit");
    assert_eq!(
        serde_json::to_value(&shell.core.workspaces["T1"].channels).unwrap(),
        before
    );
    assert_eq!(
        shell.core.workspaces["T1"].display_name("U_DM_PEER"),
        "Readable peer"
    );
    assert!(
        !shell.core.workspaces["T1"]
            .users
            .contains_key("U_MUST_NOT_LOOKUP")
    );
    assert!(
        !shell.core.workspaces["T1"]
            .users
            .contains_key("U_NAME_ONLY")
    );
    assert_eq!((shell.search_pager.page, shell.search_pager.total), (1, 6));
    assert!(
        !shell.search_pager.loading && !shell.search_pager.failed && !shell.search_pager.has_more()
    );
    assert_eq!(shell.core.composer.text, "Keep draft");
}
