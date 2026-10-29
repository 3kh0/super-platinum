use super::*;
use crate::bootstrap::test_support::{self, Harness};
use serde_json::json;
use std::cell::Cell;
use super_platinum_core::slack::{
    PreparedRequest,
    client::{RequestBody, SlackClientConfig},
};

fn setup(kind: SearchKind) -> Harness {
    let h = test_support::harness();
    let mut state = h.state;
    {
        let mut s = state.write();
        s.set_search_kind(kind);
        s.set_search_query("release".into());
        s.search_pager.change_sort(SearchSort::Newest);
    }
    h
}
fn control(state: Signal<ShellState>) -> Control {
    Control::capture(&state.peek()).unwrap()
}
fn sort(mut state: Signal<ShellState>, value: SearchSort) -> Visit {
    let source = control(state);
    plan_sort(&mut state.write(), source, value).unwrap()
}
fn kind(mut state: Signal<ShellState>, value: SearchKind) -> Visit {
    let source = control(state);
    plan_kind(&mut state.write(), source, value).unwrap()
}
fn wire(request: &PreparedRequest, expected: SearchKind, sort: &str, direction: &str) {
    let endpoint = match expected {
        SearchKind::Messages => "/api/search.modules.messages",
        SearchKind::Files => "/api/search.files",
        SearchKind::People => panic!("separate People request path"),
    };
    assert!(request.url.split('?').next().unwrap().ends_with(endpoint));
    let RequestBody::Form(fields) = &request.body else {
        panic!("unchanged native legacy form")
    };
    let get = |key| fields.iter().find(|(k, _)| k == key).unwrap().1.as_str();
    assert_eq!(get("sort"), sort);
    assert_eq!(get("sort_dir"), direction);
    assert_eq!(get("page"), "1");
    assert!(
        !fields.iter().any(|(k, v)| k == "sort" && v == "created"),
        "no private File-Newest mapping smuggled into legacy endpoint"
    );
}
async fn read(
    h: &Harness,
    visit: Visit,
    expected: SearchKind,
    sort_value: SearchSort,
    expected_wire: (&str, &str),
) {
    let mut state = h.state;
    let client = state.peek().core.client.clone();
    let calls = Cell::new(0);
    assert!(run_with(state,visit,||async {
        let q=state.peek().search_query.clone();let request=super::super::begin(&mut state.write(),"T1",&q,false).unwrap();
        assert_eq!((request.kind,request.sort,request.page),(expected,sort_value,1));
        let execute=|r:PreparedRequest|{calls.set(calls.get()+1);wire(&r,expected,expected_wire.0,expected_wire.1);async move {
                Ok(json!({"ok":true,"items":[],"pagination":{"page":1,"page_count":1,"total_count":0},"files":{"matches":[],"pagination":{"page":1,"page_count":1,"total_count":0}}}))
        }};
        match expected {
            SearchKind::Messages=>super::super::messages::load_with_profiles(state,request,&h.transport,&client,&h.session,None,execute,|_|async{Ok(Vec::new())}).await,
            SearchKind::Files=>super::super::files::load_with_profiles(state,request,&h.transport,&client,&h.session,None,execute,|_|async{Ok(Vec::new())}).await,
            SearchKind::People=>unreachable!(),
        }
    }).await);
    assert_eq!(calls.get(), 1);
    assert!(!state.peek().search_pager.loading);
    assert!(
        !state.peek().search_pager.failed,
        "actual response shape settles the native decoder"
    );
}

#[tokio::test]
async fn explicit_message_file_choices_survive_user_type_and_people_cycle_with_actual_native_page_one_requests()
 {
    let h = setup(SearchKind::Messages);
    let mut state = h.state;
    let draft = state.peek().core.composer.text.clone();
    read(
        &h,
        sort(state, SearchSort::Oldest),
        SearchKind::Messages,
        SearchSort::Oldest,
        ("timestamp", "asc"),
    )
    .await;
    let visit = kind(state, SearchKind::Files);
    assert_eq!(
        state.peek().search_pager.sort,
        SearchSort::Oldest,
        "unknown target retains legacy carry"
    );
    read(
        &h,
        visit,
        SearchKind::Files,
        SearchSort::Oldest,
        ("timestamp", "asc"),
    )
    .await;
    read(
        &h,
        sort(state, SearchSort::Relevant),
        SearchKind::Files,
        SearchSort::Relevant,
        ("score", "desc"),
    )
    .await;
    kind(state, SearchKind::People);
    let source = control(state);
    assert!(
        plan_sort(&mut state.write(), source, SearchSort::Newest).is_none(),
        "generic People sort is never recorded"
    );
    read(
        &h,
        kind(state, SearchKind::Messages),
        SearchKind::Messages,
        SearchSort::Oldest,
        ("timestamp", "asc"),
    )
    .await;
    read(
        &h,
        kind(state, SearchKind::Files),
        SearchKind::Files,
        SearchSort::Relevant,
        ("score", "desc"),
    )
    .await;
    let s = state.peek();
    let record = s.search_control_memory.as_ref().unwrap();
    assert_eq!(
        (record.messages, record.files),
        (Some(SearchSort::Oldest), Some(SearchSort::Relevant))
    );
    assert_eq!(s.core.composer.text, draft);
}

#[test]
fn history_defaults_same_value_and_legacy_kind_primitive_do_not_populate_or_replace_explicit_choices()
 {
    let h = setup(SearchKind::Messages);
    let mut state = h.state;
    let source = control(state);
    assert!(plan_sort(&mut state.write(), source, SearchSort::Newest).is_none());
    assert!(state.peek().search_control_memory.is_none());
    sort(state, SearchSort::Oldest);
    kind(state, SearchKind::Files);
    sort(state, SearchSort::Relevant);
    let entry = {
        let mut s = state.write();
        s.core
            .workspaces
            .get_mut("T1")
            .unwrap()
            .recent_searches
            .record("from history", SearchKind::Messages, SearchSort::Newest);
        crate::view::search::history::toggle(&mut s);
        crate::view::search::history::entries(&s)[0].clone()
    };
    let selection =
        crate::view::search::history::Selection::capture(&state.peek(), &entry).unwrap();
    assert!(crate::view::search::history::prepare(
        &mut state.write(),
        selection
    ));
    assert_eq!(state.peek().search_pager.sort, SearchSort::Newest);
    assert_eq!(
        state
            .peek()
            .search_control_memory
            .as_ref()
            .unwrap()
            .messages,
        Some(SearchSort::Oldest)
    );
    kind(state, SearchKind::Files);
    assert_eq!(state.peek().search_pager.sort, SearchSort::Relevant);
    kind(state, SearchKind::Messages);
    assert_eq!(state.peek().search_pager.sort, SearchSort::Oldest);
    {
        let mut s = state.write();
        s.set_search_kind(SearchKind::People);
        s.search_pager.change_sort(SearchSort::Oldest);
        s.set_search_kind(SearchKind::Messages);
        assert_eq!(
            s.search_pager.sort,
            SearchSort::Oldest,
            "original Core primitive/test contract intact"
        );
    }
}

#[test]
fn rendered_memory_controls_require_exact_raw_page_flags_and_owner_without_erasing_successor_choices()
 {
    for change in 0..14 {
        let h = setup(SearchKind::Messages);
        let mut state = h.state;
        let source = control(state);
        {
            let mut s = state.write();
            match change {
                0 => s.search_query.push(' '),
                1 => s.search_pager.loading = true,
                2 => s.search_pager.failed = true,
                3 => s.search_pager.page += 1,
                4 => s.core.account_epoch += 1,
                5 => s
                    .core
                    .session
                    .as_mut()
                    .unwrap()
                    .workspaces
                    .get_mut("T1")
                    .unwrap()
                    .token
                    .push_str("-rotated"),
                6 => {
                    let mut config = SlackClientConfig::default();
                    config.identity.user_agent.push_str(" alternate");
                    s.core.client = SlackClient::new(config);
                }
                7 => s.core.transport = Some(test_support::transport("synthetic")),
                8 => s.overlay = None,
                9 => s.cancel_search_open(),
                10 => {
                    s.search_pager.change_kind(SearchKind::Files);
                }
                11 => s
                    .core
                    .session
                    .as_mut()
                    .unwrap()
                    .d_cookie
                    .push_str("-rotated"),
                12 => s
                    .core
                    .workspaces
                    .get_mut("T1")
                    .unwrap()
                    .self_user_id
                    .push('X'),
                13 => s.search_pager.query.push('X'),
                _ => unreachable!(),
            }
        }
        assert!(plan_sort(&mut state.write(), source, SearchSort::Oldest).is_none());
        assert!(state.peek().search_control_memory.is_none());
    }
    let h = setup(SearchKind::Messages);
    let mut state = h.state;
    sort(state, SearchSort::Oldest);
    let stale = control(state);
    state.write().core.account_epoch += 1;
    sort(state, SearchSort::Relevant);
    assert!(plan_sort(&mut state.write(), stale, SearchSort::Oldest).is_none());
    assert_eq!(
        state
            .peek()
            .search_control_memory
            .as_ref()
            .unwrap()
            .messages,
        Some(SearchSort::Relevant)
    );
    assert!(
        state
            .peek()
            .search_control_memory
            .as_ref()
            .unwrap()
            .owner
            .account_current(&state.peek())
    );
}

#[tokio::test]
async fn deferred_memory_reads_refuse_successors_local_auth_promotion_disposal_and_late_old_loaded_results()
 {
    let h = setup(SearchKind::Messages);
    let mut state = h.state;
    let calls = Cell::new(0);
    let visit = sort(state, SearchSort::Oldest);
    state.write().set_search_query("new query".into());
    assert!(!run_with(state, visit, || async { calls.set(calls.get() + 1) }).await);
    assert_eq!(calls.get(), 0);
    let auth = state.peek().core.session.clone();
    state.write().core.session = None;
    assert!(
        Control::capture(&state.peek()).is_none(),
        "transport-only auth cannot become a local data owner"
    );
    {
        let mut s = state.write();
        s.core.session = auth.clone();
        s.core.transport = None;
    }
    assert!(
        Control::capture(&state.peek()).is_none(),
        "session-only auth cannot become a local data owner"
    );
    {
        let mut s = state.write();
        s.core.session = None;
        s.core.transport = None;
    }
    let visit = sort(state, SearchSort::Relevant);
    assert!(matches!(&visit.owner, Owner::Local(_)));
    assert!(crate::bootstrap::credentials(&state).is_none());
    assert!(
        run_with(state, visit.clone(), move || super::super::search(state)).await,
        "actual no-auth Search admission returns without a request"
    );
    {
        let mut s = state.write();
        s.core.transport = Some(h.transport.clone());
        s.core.session = auth;
    }
    assert!(!run_with(state, visit, || async { calls.set(calls.get() + 1) }).await);
    assert_eq!(calls.get(), 0);
    let visit = sort(state, SearchSort::Oldest);
    let client = state.peek().core.client.clone();
    let (tx, rx) = tokio::sync::oneshot::channel();
    let mut read_state = state;
    let transport = h.transport.clone();
    let session = h.session.clone();
    let observed = &calls;
    let work = run_with(state, visit, move || async move {
        let q = read_state.peek().search_query.clone();
        let request = super::super::begin(&mut read_state.write(), "T1", &q, false).unwrap();
        super::super::messages::load_with_profiles(
            read_state,
            request,
            &transport,
            &client,
            &session,
            None,
            |_| async {
                observed.set(observed.get() + 1);
                Ok(rx.await.unwrap())
            },
            |_| async { Ok(Vec::new()) },
        )
        .await;
    });
    let mut work = Box::pin(work);
    test_support::pending(work.as_mut()).await;
    let _new_visit = kind(state, SearchKind::Files);
    tx.send(json!({"ok":true,"items":[{"channel":{"id":state.peek().core.active_channel,"name":"ship"},"messages":[{"user":"U1","ts":"120.000001","text":"Late old result"}]}],"pagination":{"page":1,"page_count":1,"total_count":1}})).unwrap();
    assert!(work.await);
    assert_eq!(calls.get(), 1);
    assert!(state.peek().search_results.is_empty());
    assert_eq!(state.peek().search_pager.kind, SearchKind::Files);
    assert!(!state.peek().search_pager.failed);
    let visit = sort(state, SearchSort::Relevant);
    drop(h._dom);
    assert!(!run_with(state, visit, || async { calls.set(calls.get() + 1) }).await);
    assert_eq!(calls.get(), 1);
}
