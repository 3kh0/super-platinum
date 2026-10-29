use super::*;
use crate::{
    bootstrap::test_support::{harness, pending},
    state::{ChannelTab, MainView},
};
use std::cell::Cell;
use super_platinum_core::links::ChannelLink;
use super_platinum_core::slack::models::SearchPagination;
fn page(name: &str, number: u32) -> LinksPage {
    LinksPage {
        items: vec![ChannelLink {
            url: format!("https://example.test/{name}"),
            title: Some(name.into()),
            timestamp: "1.0".into(),
            ..Default::default()
        }],
        pagination: Some(SearchPagination {
            page: Some(number),
            page_count: Some(5),
            total_count: Some(83),
            ..Default::default()
        }),
    }
}
fn setup(mut state: Signal<ShellState>) -> String {
    let mut shell = state.write();
    shell.main_view = MainView::Home;
    shell.overlay = None;
    shell.channel_tab = ChannelTab::Links;
    shell.close_thread();
    let channel = shell.core.active_channel.clone().unwrap();
    let epoch = shell.core.account_epoch;
    shell.core.channel_links.ensure_scope(epoch, "T1", &channel);
    let generation = shell.core.channel_links.begin("docs".into(), 1);
    assert!(shell.core.channel_links.apply(generation, page("one", 1)));
    channel
}
fn source(state: Signal<ShellState>, channel: &str) -> Source {
    Source::capture(&state.peek(), channel).unwrap()
}
#[tokio::test(flavor = "current_thread")]
async fn actual_links_driver_retains_failed_page_and_retries_exact_normalized_target() {
    let h = harness();
    let state = h.state;
    let channel = setup(state);
    let origin = source(state, &channel);
    let (release, held) = tokio::sync::oneshot::channel();
    let expected = channel.clone();
    let mut work =
        Box::pin(
            origin.run_with(state, " docs ".into(), 2, move |request| async move {
                assert_eq!(request.channel, expected);
                assert_eq!(request.query, "docs");
                assert_eq!(request.page, 2);
                held.await.unwrap();
                Err(Error::Offline("synthetic offline".into()))
            }),
        );
    pending(work.as_mut()).await;
    assert!(state.peek().core.channel_links.loading);
    assert_eq!(
        state.peek().core.channel_links.items[0].title.as_deref(),
        Some("one")
    );
    assert_eq!(
        (
            state.peek().core.channel_links.page,
            state.peek().core.channel_links.page_count,
            state.peek().core.channel_links.total
        ),
        (1, 5, 83)
    );
    release.send(()).unwrap();
    work.await;
    assert!(!state.peek().core.channel_links.loading);
    assert_eq!(state.peek().core.channel_links.requested_page, Some(2));
    let retry = source(state, &channel);
    let target = retry.resume_page();
    assert_eq!(target, 2);
    let query = retry.query.clone();
    let (release, held) = tokio::sync::oneshot::channel();
    let mut work = Box::pin(
        retry.run_with(state, query, target, move |request| async move {
            assert_eq!(request.page, 2);
            held.await.unwrap();
            Ok(page("two", 2))
        }),
    );
    pending(work.as_mut()).await;
    let busy = source(state, &channel);
    let generation = state.peek().core.channel_links.generation;
    release.send(()).unwrap();
    work.await;
    assert_eq!(state.peek().core.channel_links.generation, generation);
    assert!(!busy.current(&state.peek()));
    assert_eq!(state.peek().core.channel_links.page, 2);
    assert_eq!(
        state.peek().core.channel_links.items[0].title.as_deref(),
        Some("two")
    );
    assert_eq!(state.peek().core.channel_links.requested_page, None);
    assert!(state.peek().core.channel_links.error.is_none());
}
#[tokio::test(flavor = "current_thread")]
async fn links_retire_tab_round_trips_and_token_changes_release_only_the_owned_links_slot() {
    let h = harness();
    let mut state = h.state;
    let channel = setup(state);
    let old = source(state, &channel);
    let query = old.query.clone();
    {
        let mut shell = state.write();
        shell.channel_tab = ChannelTab::Messages;
        shell.channel_generation += 1;
        shell.channel_tab = ChannelTab::Links;
        shell.channel_generation += 1;
    }
    let calls = Rc::new(Cell::new(0));
    let observed = calls.clone();
    old.run_with(state, query, 2, move |_| {
        observed.set(observed.get() + 1);
        async { Ok(page("wrong-tab", 2)) }
    })
    .await;
    assert_eq!(calls.get(), 0);
    let fresh = source(state, &channel);
    let query = fresh.query.clone();
    let (release, held) = tokio::sync::oneshot::channel();
    let mut work = Box::pin(fresh.run_with(state, query, 2, move |_| async move {
        held.await.unwrap();
        Ok(page("wrong-token", 2))
    }));
    pending(work.as_mut()).await;
    let files_generation = state.peek().core.files.generation;
    state
        .write()
        .core
        .session
        .as_mut()
        .unwrap()
        .workspaces
        .get_mut("T1")
        .unwrap()
        .token = "rotated-links-synthetic".into();
    release.send(()).unwrap();
    work.await;
    let shell = state.peek();
    assert!(!shell.core.channel_links.loading);
    assert_eq!(shell.core.channel_links.resume_page(), 2);
    assert_eq!(
        shell.core.channel_links.items[0].title.as_deref(),
        Some("one")
    );
    assert_eq!(shell.core.files.generation, files_generation);
}
