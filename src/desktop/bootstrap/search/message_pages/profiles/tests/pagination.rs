//! The existing fetched and cached page controls, not a second test fixture.
use super::*;

pub(super) async fn round_trip(
    dom: &mut VirtualDom,
    state: Signal<ShellState>,
    captured: &Captured,
    io: &Rc<test_io::Io>,
) {
    let original = pages::rows(&state.peek()).to_vec();
    let before = count(io);
    let page_io = Rc::new(pages::test_io::Io::default());
    dom.in_scope(ScopeId::ROOT, || provide_context(page_io.clone()));
    let next_page = page_io.expect(2);
    let prefs = io.expect("users.prefs.get");
    let humans = io.expect("users.info");
    let next = button(captured.borrow().as_ref().unwrap(), "message-search-next").unwrap();
    assert!(!next.disabled);
    click(dom, next);
    settle(dom, || page_io.pages.borrow().as_slice() == [2]).await;
    let channel = state.peek().core.active_channel.clone().unwrap();
    let mut raw = raw_page(&channel);
    raw["pagination"]["page"] = json!(2);
    raw["items"][0]["messages"][0]["ts"] = json!("1700000201.000001");
    raw["items"][1]["messages"][0]["ts"] = json!("1700000202.000001");
    reply(next_page, raw);
    settle(dom, || count(io) == before + 1).await;
    assert_eq!(state.peek().search_pager.page, 2);
    reply(prefs, json!({"ok":true,"prefs":{"hidden_users":""}}));
    settle(dom, || count(io) == before + 2).await;
    reply(
        humans,
        json!({"ok":true,"results":[member("U1","Page two Maya"),member("U2","Page two Jules")]}),
    );
    settle(dom, || control(&state.peek(), 1).is_some()).await;
    let stale = button(
        captured.borrow().as_ref().unwrap(),
        "message-search-author-2-1",
    )
    .unwrap();

    let prefs = io.expect("users.prefs.get");
    let humans = io.expect("users.info");
    let previous = button(
        captured.borrow().as_ref().unwrap(),
        "message-search-previous",
    )
    .unwrap();
    assert!(!previous.disabled);
    click(dom, previous);
    settle(dom, || count(io) == before + 3).await;
    assert_eq!(state.peek().search_pager.page, 1);
    assert_eq!(pages::rows(&state.peek()), original);
    assert_eq!(
        page_io.pages.borrow().as_slice(),
        [2],
        "Previous reuses its original cached Search page"
    );
    reply(prefs, json!({"ok":true,"prefs":{"hidden_users":""}}));
    settle(dom, || count(io) == before + 4).await;
    reply(
        humans,
        json!({"ok":true,"results":[member("U1","Fresh Maya"),member("U2","Fresh Jules")]}),
    );
    settle(dom, || {
        control(&state.peek(), 0).is_some() && control(&state.peek(), 1).is_some()
    })
    .await;
    click(dom, stale);
    assert!(
        state.peek().profile_user.is_none(),
        "old page author cannot borrow the cached successor's scope"
    );
    assert_eq!(pages::rows(&state.peek()), original);
    assert_eq!(count(io), before + 4);
}
