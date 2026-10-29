use super::*;
use crate::bootstrap::test_support::{self, Harness};
use serde_json::{Value, json};
use std::cell::Cell;
use super_platinum_core::slack::{Error, PreparedRequest, SlackClient, client::RequestBody};

fn setup() -> Harness {
    let h = test_support::harness();
    let mut state = h.state;
    state.write().set_search_kind(SearchKind::Messages);
    state.write().set_search_query("release".into());
    h
}
fn first(h: &Harness) -> (SearchRequest, Read) {
    let mut state = h.state;
    let query = state.peek().search_query.clone();
    let request = super::super::begin(&mut state.write(), "T1", &query, false).unwrap();
    let read = initial(
        &state.peek(),
        &request,
        &h.transport,
        &SlackClient::default(),
        &h.session,
    )
    .unwrap();
    (request, read)
}
fn response(state: Signal<ShellState>, page: u32, bound: u32, total: u64, empty: bool) -> Value {
    json!({"ok":true,"items":if empty {json!([])} else {json!([{
        "channel":{"id":state.peek().core.active_channel,"name":"ship"},
        "messages":[{"user":"U_PAGE_AUTHOR","ts":format!("120.{page:06}"),"text":format!("Page {page} release")}]
    }])},"pagination":{"page":page,"page_count":bound,"total_count":total}})
}
fn wire(request: PreparedRequest, page: u32) {
    assert!(
        request
            .url
            .split('?')
            .next()
            .unwrap()
            .ends_with("/api/search.modules.messages")
    );
    let RequestBody::Form(fields) = request.body else {
        panic!("existing form required")
    };
    let fields = fields.into_iter().collect::<BTreeMap<_, _>>();
    assert_eq!(fields["page"], page.to_string());
    assert_eq!(fields["query"], "release");
    assert_eq!(fields["count"], "20");
    assert_eq!(
        (&*fields["sort"], &*fields["sort_dir"]),
        ("timestamp", "desc")
    );
}
async fn load(
    h: &Harness,
    request: SearchRequest,
    read: Read,
    result: Result<Value, Error>,
    calls: &Cell<usize>,
    lookups: &Cell<usize>,
) {
    let page = request.page;
    super::super::messages::load_owned_with_profiles(
        h.state,
        request,
        &h.transport,
        &SlackClient::default(),
        &h.session,
        None,
        Some(read),
        |request| {
            wire(request, page);
            calls.set(calls.get() + 1);
            std::future::ready(result)
        },
        |ids| {
            assert_eq!(ids, ["U_PAGE_AUTHOR"]);
            lookups.set(lookups.get() + 1);
            std::future::ready(Ok(vec![
                serde_json::from_value(
                    json!({"id":"U_PAGE_AUTHOR","profile":{"display_name":"Page author"}}),
                )
                .unwrap(),
            ]))
        },
    )
    .await;
}
fn planned(mut state: Signal<ShellState>, page: u32, retry: bool) -> Plan {
    let source = Control::capture(&state.peek()).unwrap();
    plan(&mut state.write(), &source, page, retry, None).unwrap()
}
async fn finish(
    h: &Harness,
    work: read::Work,
    result: Result<Value, Error>,
    calls: &Cell<usize>,
    lookups: &Cell<usize>,
) {
    let request = work.ticket.request.clone();
    load(h, request, Read::Page(work.ticket), result, calls, lookups).await;
}
async fn seed(h: &Harness, calls: &Cell<usize>, lookups: &Cell<usize>) {
    let (request, read) = first(h);
    load(
        h,
        request,
        read,
        Ok(response(h.state, 1, 7, 121, false)),
        calls,
        lookups,
    )
    .await;
    assert!(active(&h.state.peek()));
}

pub(crate) async fn loaded_harness() -> Harness {
    let h = setup();
    seed(&h, &Cell::new(0), &Cell::new(0)).await;
    h
}

#[tokio::test]
async fn actual_message_decoder_hydration_and_empty_page_cache_preserve_latest_metadata_without_more_io()
 {
    let h = setup();
    let state = h.state;
    let calls = Cell::new(0);
    let lookups = Cell::new(0);
    seed(&h, &calls, &lookups).await;
    assert_eq!((calls.get(), lookups.get()), (1, 1));
    assert_eq!(rows(&state.peek())[0].author, "Page author");
    let Plan::Fetch(work) = planned(state, 2, false) else {
        panic!("uncached page")
    };
    finish(
        &h,
        work,
        Ok(response(state, 2, 2, 88, true)),
        &calls,
        &lookups,
    )
    .await;
    assert_eq!(status(&state.peek()), Some((2, 2, 7, 88, false, false)));
    assert!(rows(&state.peek()).is_empty());
    assert!(matches!(planned(state, 1, false), Plan::Cached));
    assert_eq!(rows(&state.peek())[0].text, "Page 1 release");
    assert_eq!(status(&state.peek()), Some((1, 1, 7, 88, false, false)));
    let Plan::Fetch(work) = planned(state, 3, false) else {
        panic!("third page")
    };
    finish(
        &h,
        work,
        Ok(response(state, 3, 1, 0, false)),
        &calls,
        &lookups,
    )
    .await;
    assert_eq!(status(&state.peek()), Some((3, 3, 7, 0, false, false)));
    assert!(matches!(planned(state, 1, false), Plan::Cached));
    assert!(matches!(planned(state, 2, false), Plan::Cached));
    assert!(rows(&state.peek()).is_empty());
    assert_eq!(status(&state.peek()), Some((2, 2, 7, 0, false, false)));
    assert_eq!((calls.get(), lookups.get()), (3, 1));
    assert_eq!(state.peek().core.composer.text, "Keep draft");
}

#[tokio::test]
async fn failed_fourth_page_retains_first_rows_and_retries_exact_fourth_under_admitted_reconnect() {
    let h = setup();
    let mut state = h.state;
    let calls = Cell::new(0);
    let lookups = Cell::new(0);
    seed(&h, &calls, &lookups).await;
    let Plan::Fetch(work) = planned(state, 4, false) else {
        panic!()
    };
    let before = Control::capture(&state.peek()).unwrap();
    assert!(plan(&mut state.write(), &before, 2, false, None).is_none());
    finish(
        &h,
        work,
        Err(Error::Offline("synthetic outage".into())),
        &calls,
        &lookups,
    )
    .await;
    assert_eq!(status(&state.peek()), Some((1, 4, 7, 121, false, true)));
    assert_eq!(rows(&state.peek())[0].text, "Page 1 release");
    let Plan::Fetch(work) = planned(state, 4, true) else {
        panic!()
    };
    assert_eq!(work.ticket.request.page, 4);
    finish(
        &h,
        work,
        Err(Error::Offline("synthetic outage".into())),
        &calls,
        &lookups,
    )
    .await;
    let old = crate::bootstrap::reconnect::resource_test_visit(&mut state.write());
    let visit = crate::bootstrap::reconnect::resource_test_visit(&mut state.write());
    let source = Control::capture(&state.peek()).unwrap();
    let revision = state.peek().search_pager.revision();
    super::super::recovery::schedule(state, visit.clone());
    assert_eq!(
        state.peek().search_pager.revision(),
        revision,
        "generic recovery must refuse owned books"
    );
    assert!(plan(&mut state.write(), &source, 4, true, Some(old)).is_none());
    let Some(Plan::Fetch(work)) = plan(&mut state.write(), &source, 4, true, Some(visit)) else {
        panic!()
    };
    crate::bootstrap::reconnect::resource_test_visit(&mut state.write());
    finish(
        &h,
        work,
        Ok(response(state, 4, 1, 3, false)),
        &calls,
        &lookups,
    )
    .await;
    assert_eq!(status(&state.peek()), Some((4, 4, 7, 3, false, false)));
    assert_eq!(rows(&state.peek())[0].text, "Page 4 release");
    assert_eq!(calls.get(), 4);
}

#[tokio::test]
async fn manual_cache_read_can_finish_closed_but_recovery_keeps_dialog_custody() {
    let h = setup();
    let mut state = h.state;
    let calls = Cell::new(0);
    let lookups = Cell::new(0);
    let (request, read) = first(&h);
    state.write().dismiss_overlay();
    load(
        &h,
        request,
        read,
        Ok(response(state, 1, 7, 121, false)),
        &calls,
        &lookups,
    )
    .await;
    assert!(active(&state.peek()));
    assert!(state.peek().overlay.is_none());
    assert_eq!(rows(&state.peek()).len(), 1);
    state.write().open_search(false);
    let Plan::Fetch(work) = planned(state, 4, false) else {
        panic!()
    };
    finish(
        &h,
        work,
        Err(Error::Offline("synthetic outage".into())),
        &calls,
        &lookups,
    )
    .await;
    let visit = crate::bootstrap::reconnect::resource_test_visit(&mut state.write());
    let source = Control::capture(&state.peek()).unwrap();
    let Some(Plan::Fetch(work)) = plan(&mut state.write(), &source, 4, true, Some(visit)) else {
        panic!()
    };
    state.write().dismiss_overlay();
    let before = calls.get();
    finish(
        &h,
        work,
        Ok(response(state, 4, 7, 99, false)),
        &calls,
        &lookups,
    )
    .await;
    assert_eq!(
        calls.get(),
        before,
        "closed queued recovery must decline before IO"
    );
    assert!(state.peek().overlay.is_none());
    assert_eq!(state.peek().search_pager.page, 1);
    assert_eq!(rows(&state.peek())[0].text, "Page 1 release");
    assert_eq!(
        lookups.get(),
        0,
        "closed manual cache retained fallback rather than author publication"
    );
}

#[tokio::test]
async fn late_page_success_error_and_owned_release_cannot_replace_a_fresh_same_query_book() {
    for late_error in [false, true] {
        let h = setup();
        let mut state = h.state;
        let calls = Cell::new(0);
        let lookups = Cell::new(0);
        seed(&h, &calls, &lookups).await;
        let old_control = Control::capture(&state.peek()).unwrap();
        let Plan::Fetch(work) = planned(state, 4, false) else {
            panic!()
        };
        let request = work.ticket.request.clone();
        let (send, receive) = tokio::sync::oneshot::channel::<Result<Value, Error>>();
        let client = SlackClient::default();
        let pending = super::super::messages::load_owned_with_profiles(
            state,
            request,
            &h.transport,
            &client,
            &h.session,
            None,
            Some(Read::Page(work.ticket)),
            |request| {
                wire(request, 4);
                calls.set(calls.get() + 1);
                async { receive.await.unwrap() }
            },
            |_| async { panic!("retired result cannot hydrate") },
        );
        tokio::pin!(pending);
        test_support::pending(pending.as_mut()).await;
        state.write().set_search_query("other".into());
        state.write().set_search_query("release".into());
        seed(&h, &calls, &lookups).await;
        let revision = state.peek().search_pager.revision();
        assert!(plan(&mut state.write(), &old_control, 2, false, None).is_none());
        let result = if late_error {
            Err(Error::Offline("synthetic outage".into()))
        } else {
            Ok(response(state, 4, 7, 66, false))
        };
        send.send(result).unwrap();
        pending.await;
        assert_eq!(state.peek().search_pager.revision(), revision);
        assert_eq!(status(&state.peek()), Some((1, 1, 7, 121, false, false)));
        assert_eq!(rows(&state.peek())[0].text, "Page 1 release");
        assert!(state.peek().toast.is_none());
        assert_eq!((calls.get(), lookups.get()), (3, 1));
    }
}

#[tokio::test]
async fn owned_entry_guards_metadata_legacy_append_and_actual_queued_row_opener_across_page_aba() {
    let h = setup();
    let mut state = h.state;
    let calls = Cell::new(0);
    let lookups = Cell::new(0);
    seed(&h, &calls, &lookups).await;
    let old_row = RowSource::capture(&state.peek(), 0).unwrap();
    let old_hit = rows(&state.peek())[0].clone();
    let Plan::Fetch(work) = planned(state, 2, false) else {
        panic!()
    };
    let revision = state.peek().search_pager.revision();
    super::super::search_more(state).await;
    assert_eq!(
        state.peek().search_pager.revision(),
        revision,
        "legacy append cannot overlap owned read"
    );
    let mut malformed = response(state, 2, 7, 99, false);
    malformed["pagination"]
        .as_object_mut()
        .unwrap()
        .remove("page");
    finish(&h, work, Ok(malformed), &calls, &lookups).await;
    assert_eq!(status(&state.peek()), Some((1, 2, 7, 121, false, true)));
    assert!(has_scope(&state.peek()));
    let Plan::Fetch(work) = planned(state, 2, true) else {
        panic!()
    };
    finish(
        &h,
        work,
        Ok(response(state, 2, 7, 99, false)),
        &calls,
        &lookups,
    )
    .await;
    assert!(matches!(planned(state, 1, false), Plan::Cached));
    let generation = state.peek().search_open_generation;
    open(state, old_row);
    crate::bootstrap::open_search_hit(state, old_hit);
    assert_eq!(state.peek().search_open_generation, generation);
    assert!(state.peek().search_opening.is_none());
    let current = RowSource::capture(&state.peek(), 0).unwrap();
    h._dom
        .runtime()
        .in_scope(dioxus::core::ScopeId::ROOT, || open(state, current));
    assert_eq!(
        state.peek().search_open_generation,
        generation.wrapping_add(1)
    );
    assert!(
        state.peek().search_opening.is_some(),
        "actual opener admitted only the fresh retained row"
    );
    // Retire synchronously before polling the actual task. No conversation IO,
    // native input, history load or live Slack operation is performed here.
    state
        .write()
        .set_search_query("retire before task IO".into());
    assert!(state.peek().search_opening.is_none());
    assert_eq!(calls.get(), 3);
}
