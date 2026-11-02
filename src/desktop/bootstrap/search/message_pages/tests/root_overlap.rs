//! Genuine original Search page/Owner for the shared-bank workflow.
use super::*;
use crate::bootstrap::search_message_pages::reaction_actions as actions;
pub(crate) async fn original_search_control(
    dom: &mut VirtualDom,
    mut state: Signal<ShellState>,
    original: Value,
    blocked: bool,
) {
    {
        let mut s = state.write();
        s.overlay = Some(crate::state::Overlay::Search);
        s.clear_search_results();
        s.set_search_kind(SearchKind::Messages);
        s.set_search_query("release".into());
    }
    let (transport, client, session) = {
        let s = state.peek();
        (
            s.core.transport.clone().unwrap(),
            s.core.client.clone(),
            s.core.session.as_ref().unwrap().workspaces["T1"].clone(),
        )
    };
    let request = super::super::super::begin(&mut state.write(), "T1", "release", false).unwrap();
    let owned = initial(&state.peek(), &request, &transport, &client, &session).unwrap();
    let raw = json!({"ok":true,"items":[{"channel":{"id":"C1","name":"ship"},"messages":[original]}],"pagination":{"page":1,"page_count":1,"total_count":1}});
    super::super::super::messages::load_owned_with_profiles(
        state,
        request,
        &transport,
        &client,
        &session,
        None,
        Some(owned),
        |_| std::future::ready(Ok(raw)),
        |ids| {
            std::future::ready(Ok(ids
                .into_iter()
                .map(|id| {
                    serde_json::from_value(
                        json!({"id":id,"profile":{"display_name":"Original author"}}),
                    )
                    .unwrap()
                })
                .collect()))
        },
    )
    .await;
    assert_eq!(rows(&state.peek()).len(), 1);
    assert!(
        actions::available(&state.peek(), 0),
        "positive Search candidate and caught-up Owner; overlap is not defeated by truthy thread_ts"
    );
    let io = Arc::new(actions::test_io::Io::new(&session.token));
    state.write().search_reaction_actions.io = Some(io.clone());
    let response = (!blocked).then(|| io.expect("conversations.history"));
    dom.in_scope(ScopeId::ROOT, || {
        let source = RowSource::capture(&state.peek(), 0).unwrap();
        actions::open(state, source);
    });
    if let Some(response) = response {
        crate::resource_reads::render_test::settle(dom, || io.calls.lock().unwrap().len() == 1)
            .await;
        response
            .send(Err(Error::Offline(
                "bounded overlap control policy read".into(),
            )))
            .unwrap();
        crate::resource_reads::render_test::settle(dom, || {
            actions::presentation(&state.peek()).is_some_and(|p| p.terminal)
        })
        .await;
        assert_eq!(
            io.calls.lock().unwrap().len(),
            1,
            "unblocked actual Source control reaches original policy IO"
        );
    } else {
        assert!(actions::presentation(&state.peek()).unwrap().terminal);
        assert!(
            io.calls.lock().unwrap().is_empty(),
            "shared pending/Unknown prevents new policy or mutation IO"
        );
    }
    let id = actions::presentation(&state.peek()).unwrap().id;
    actions::close(state, id);
    state.write().overlay = None;
}
