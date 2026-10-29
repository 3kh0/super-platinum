use super::*;
use crate::app_home::channel_fetches::calendar_member_test_support as renderer;
use crate::bootstrap::test_support::Harness;
use serde_json::{Value, json};
use std::{cell::Cell, future::Future, pin::Pin, task::Poll};
use super_platinum_core::slack::events::RtEvent;
pub(crate) struct Fixture {
    pub(crate) h: Harness,
    team: String,
}
pub(crate) async fn setup(has_prefs: bool) -> Fixture {
    let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
    crate::app_home::channel_fetches::fixture::install_people(&mut s);
    let team = s.core.active_team.clone().unwrap();
    renderer::connect(&mut s).unwrap();
    let initial=renderer::assignments(&mut s,&json!({"ok":true,"assignments":{"enterprise_search":{"group":"off"},"search_normalize_visible_external_users":{"group":"off"},"preserve_omitted_user_change_member_fields":{"group":"on"}}})).unwrap();
    let mut h = crate::message_buttons::inputs::user::directory::read_scope::test_support::mount(s);
    renderer::provide(&h._dom, Rc::new(renderer::Renderer::default()));
    let mut state = h.state;
    renderer::run_pumped(&mut h._dom, initial.drain(state))
        .await
        .unwrap();
    let user = state.peek().core.workspaces[&team].self_user_id.clone();
    let mut init = json!({"ok":true,"self":{"id":user,"name":"fixture-self","team_id":team,"profile":{}},"team":{"id":team,"name":"Files pages"},"workspaces":[{"id":team}],"onboarding":{"user_data":{"onboarding_complete":true}},"user_date_created":123,"prefs":{"result_count_toggle":false,"stick_user_sort":false,"search_exclude_bots":false}});
    if !has_prefs {
        init.as_object_mut().unwrap().remove("prefs");
    }
    let channels = json!({"ok":true,"channels":{"channels":[{"id":"CINIT0001","name":"warm-harbor","is_channel":true,"is_member":true}],"ims":[],"mpims":[],"is_open":[],"has_more_mpdms":false,"visitor_channels":[],"temporary_channel_expirations":{}}});
    renderer::run_pumped(
        &mut h._dom,
        initial.complete_channels(state, init, channels),
    )
    .await
    .unwrap_or_else(|e| panic!("{e:?}: {}", initial.diagnostic(&state.peek())));
    let (request, transport, client, session) = {
        let mut s = state.write();
        s.overlay = Some(Overlay::Search);
        s.set_search_kind(SearchKind::Files);
        s.set_search_query("design notes".into());
        let request = super::super::begin(&mut s, &team, "design notes", false).unwrap();
        (
            request,
            s.core.transport.clone().unwrap(),
            s.core.client.clone(),
            s.core.session.as_ref().unwrap().workspaces[&team].clone(),
        )
    };
    // Establish Files through its unchanged real legacy request/decoder path,
    // not through a forged private-ready state or fabricated preference handle.
    super::super::files::load_scoped_with(state,request,&transport,&client,&session,None,|r|{assert!(r.url.contains("/api/search.files?"));std::future::ready(Ok(json!({"ok":true,"files":{"matches":[{"id":"FLEGACY","title":"Legacy row"}],"pagination":{"page":1,"page_count":1,"total_count":1}}})))}).await;
    assert_eq!(
        file_search_parameters::capture(&state.peek(), &team).is_some(),
        has_prefs
    );
    Fixture { h, team }
}
fn begin(f: &Fixture, sort: api::Sort) -> load::Job {
    let mut state = f.h.state;
    let source = EntrySource::capture(&state.peek()).expect("actual Files entry");
    plan_entry(&mut state.write(), source, sort).unwrap()
}
pub(crate) fn response(page: u32, empty: bool) -> Value {
    json!({"ok":true,"items":if empty{json!([])}else{json!([{"id":"FZ","title":"<b>Zed</b>","url_private":"https://untrusted.invalid","user":"UREADONLY"},{"id":"FA","title":"Alpha"},{"id":"FZ","title":"Second Zed occurrence"}])},"pagination":{"page":page,"page_count":7,"total_count":129,"last":page*20}})
}
async fn finish(
    f: &mut Fixture,
    job: load::Job,
    result: Result<Value, Error>,
    calls: &Cell<usize>,
) -> bool {
    renderer::run_pumped(
        &mut f.h._dom,
        load::run_with(f.h.state, job, |request, current, observe| {
            assert!(current());
            calls.set(calls.get() + 1);
            assert!(request.requested_page() > 0);
            observe(&result);
            std::future::ready(result)
        }),
    )
    .await
}
pub(crate) async fn loaded() -> Fixture {
    let mut f = setup(true).await;
    let job = begin(&f, api::Sort::Newest);
    assert!(finish(&mut f, job, Ok(response(1, false)), &Cell::new(0)).await);
    f
}
fn selected(f: &Fixture, page: u32, retry: bool) -> Option<load::Job> {
    let mut state = f.h.state;
    let source = Control::capture(&state.peek()).unwrap();
    plan(&mut state.write(), source, page, retry)
}
fn event(f: &Fixture, name: &str, value: Value) {
    let mut state = f.h.state;
    let mut s = state.write();
    let generation = s.core.workspaces[&f.team].rt_generation;
    let raw: Arc<str> =
        json!({"type":"pref_change","name":name,"value":value,"fixture_original":17})
            .to_string()
            .into();
    let original =
        super_platinum_core::slack::realtime::PreferenceCarrier::parse(raw.clone()).unwrap();
    assert_eq!(&*original.original(), &*raw);
    file_search_parameters::preference_event(
        &mut s,
        &f.team,
        generation,
        original.event(),
        Some(&original),
    );
}
fn canonical(f: &Fixture) -> Value {
    let s = f.h.state.peek();
    json!({"files":s.core.files.items,"channel_files":s.core.channel_files.items})
}
#[tokio::test]
async fn actual_acquisition_sorted_pages_cached_empty_and_new_sort_keep_duplicates_and_never_grant_canonical_file_or_legacy_open()
 {
    let mut f = setup(true).await;
    let mut state = f.h.state;
    let old = super::super::files::Source::capture(&state.peek(), "FLEGACY").unwrap();
    let before = canonical(&f);
    let calls = Cell::new(0);
    let job = begin(&f, api::Sort::Newest);
    assert!(super::super::files::Source::capture(&state.peek(), "FLEGACY").is_none());
    super::super::files::open(state, old);
    assert!(state.peek().viewer.is_none());
    assert!(finish(&mut f, job, Ok(response(1, false)), &calls).await);
    assert_eq!(
        rows(&state.peek())
            .iter()
            .map(|r| r.id())
            .collect::<Vec<_>>(),
        ["FZ", "FA", "FZ"]
    );
    let job = selected(&f, 2, false).unwrap();
    assert!(
        rows(&state.peek()).is_empty(),
        "pending hides success rows, cache remains"
    );
    assert!(finish(&mut f, job, Ok(response(2, true)), &calls).await);
    assert!(selected(&f, 1, false).is_none());
    assert_eq!(status(&state.peek()).unwrap().0, 1);
    assert_eq!(calls.get(), 2);
    assert!(selected(&f, 2, false).is_none());
    assert!(rows(&state.peek()).is_empty());
    assert_eq!(calls.get(), 2);
    let stale = Control::capture(&state.peek()).unwrap();
    let job = begin(&f, api::Sort::NameDescending);
    assert!(plan(&mut state.write(), stale, 3, false).is_none());
    assert!(finish(&mut f, job, Ok(response(1, false)), &calls).await);
    assert_eq!(status(&state.peek()).unwrap().6, api::Sort::NameDescending);
    assert_eq!(canonical(&f), before);
    assert!(state.peek().viewer.is_none());
    assert!(
        !state.peek().core.workspaces[&f.team]
            .users
            .contains_key("UREADONLY")
    );
    state
        .write()
        .core
        .workspaces
        .get_mut(&f.team)
        .unwrap()
        .deleted_files
        .insert("FZ".into());
    assert_eq!(
        rows(&state.peek())
            .iter()
            .filter(|hit| visible(&state.peek(), hit))
            .count(),
        1
    );
    crate::file_events::invalidate(&mut state.write(), &f.team, "FA");
    assert!(
        !rows(&state.peek())
            .iter()
            .any(|hit| visible(&state.peek(), hit)),
        "a newer file event cannot expose stale search display metadata"
    );
}
#[tokio::test]
async fn loaded_file_page_failure_retries_exact_target_and_cancellation_or_equal_preference_cannot_revive_old_actions()
 {
    let mut f = loaded().await;
    let mut state = f.h.state;
    f.h._dom.in_scope(dioxus::core::ScopeId::ROOT, || {
        provide_context(Rc::new(test_io::Io::default()))
    });
    let calls = Cell::new(0);
    let job = selected(&f, 4, false).unwrap();
    assert!(
        !finish(
            &mut f,
            job,
            Err(Error::Offline("synthetic outage".into())),
            &calls
        )
        .await
    );
    let st = status(&state.peek()).unwrap();
    assert_eq!((st.0, st.1, st.5), (1, 4, true));
    assert_eq!(rows(&state.peek()).len(), 3);
    let old = Control::capture(&state.peek()).unwrap();
    let job = selected(&f, 4, true).unwrap();
    assert_eq!(job.ticket.page, 4);
    assert!(plan(&mut state.write(), old, 2, false).is_none());
    assert!(finish(&mut f, job, Ok(response(4, true)), &calls).await);
    assert_eq!(status(&state.peek()).unwrap().0, 4);
    let cancelled = selected(&f, 3, false).unwrap();
    let mut abandoned = Box::pin(load::run_with(state, cancelled, |_, _, _| {
        std::future::pending::<Result<Value, Error>>()
    }));
    std::future::poll_fn(|cx| {
        assert!(matches!(Pin::new(&mut abandoned).poll(cx), Poll::Pending));
        Poll::Ready(())
    })
    .await;
    drop(abandoned);
    assert_eq!(status(&state.peek()).unwrap().1, 3);
    assert!(status(&state.peek()).unwrap().5);
    let job = selected(&f, 3, true).unwrap();
    let frame = job.frame.clone();
    let (send, receive) = tokio::sync::oneshot::channel();
    let mut pending = Box::pin(load::run_with(state, job, |_, _, observe| async move {
        receive.await.unwrap();
        let r = Err(Error::RateLimited {
            retry_after_secs: Some(71),
        });
        observe(&r);
        r
    }));
    std::future::poll_fn(|cx| {
        assert!(matches!(Pin::new(&mut pending).poll(cx), Poll::Pending));
        Poll::Ready(())
    })
    .await;
    event(&f, "search_exclude_bots", json!(false));
    assert!(!frame.current(&state.peek()));
    assert!(file_search_parameters::capture(&state.peek(), &f.team).is_some());
    assert!(Control::capture(&state.peek()).is_none());
    send.send(()).unwrap();
    assert!(!renderer::run_pumped(&mut f.h._dom, pending).await);
    assert!(has_scope(&state.peek()) && !active(&state.peek()));
    assert!(
        state
            .peek()
            .search_file_module
            .as_ref()
            .unwrap()
            .rate
            .cooling()
    );
    assert!(
        renderer::run_pumped(&mut f.h._dom, refresh(state)).await,
        "retired private scope must never call legacy search"
    );
    assert!(state.peek().search_file_results.is_empty());
}
#[tokio::test]
async fn missing_or_unsupported_file_observations_and_queued_entry_fail_closed_while_original_people_keys_stay_separate()
 {
    let f = setup(false).await;
    assert!(EntrySource::capture(&f.h.state.peek()).is_none());
    let f = setup(true).await;
    let mut state = f.h.state;
    let old = EntrySource::capture(&state.peek()).unwrap();
    event(&f, "search_only_my_channels", json!(true));
    assert!(plan_entry(&mut state.write(), old, api::Sort::Name).is_none());
    let parameters = file_search_parameters::capture(&state.peek(), &f.team).unwrap();
    assert_eq!(parameters.only_my_channels(), Some(true));
    assert_eq!(parameters.exclude_bots(), Some(false));
    event(&f, "search_exclude_bots", json!(true));
    event(&f, "search_exclude_bots", json!(false));
    assert!(
        !parameters.current(&state.peek()),
        "equal final values cannot revive pre-ABA receipts"
    );
    event(&f, "search_only_my_channels", json!(null));
    assert!(EntrySource::capture(&state.peek()).is_none());
    event(&f, "search_only_my_channels", json!(false));
    event(&f, "stick_user_sort", json!(true));
    assert!(EntrySource::capture(&state.peek()).is_none());
    event(&f, "stick_user_sort", json!(false));
    state.write().set_search_query("notes in:ship".into());
    assert!(EntrySource::capture(&state.peek()).is_none());
    assert_eq!(
        state.peek().core.workspaces[&f.team]
            .cfm_payload
            .user_prefs
            .get("search_people_sort"),
        None
    );
}

#[tokio::test]
async fn first_failure_exact_retry_and_late_initial_release_preserve_new_sort_successor_while_reconnect_requires_original_visit()
 {
    let mut f = setup(true).await;
    let mut state = f.h.state;
    let calls = Cell::new(0);
    let first = begin(&f, api::Sort::Newest);
    assert!(
        !finish(
            &mut f,
            first,
            Err(Error::Offline("synthetic outage".into())),
            &calls
        )
        .await
    );
    assert_eq!(state.peek().search_pager.page, 0);
    assert_eq!(status(&state.peek()).unwrap().1, 1);
    let retry = selected(&f, 1, true).unwrap();
    assert_eq!(retry.ticket.page, 1);
    assert!(finish(&mut f, retry, Ok(response(1, false)), &calls).await);
    assert!(!state.peek().search_pager.failed && state.peek().search_pager.page == 1);
    for late_error in [false, true] {
        let old = begin(&f, api::Sort::Oldest);
        let (send, receive) = tokio::sync::oneshot::channel::<Result<Value, Error>>();
        let mut pending = Box::pin(load::run_with(state, old, |_, _, _| async move {
            receive.await.unwrap()
        }));
        std::future::poll_fn(|cx| {
            assert!(matches!(Pin::new(&mut pending).poll(cx), Poll::Pending));
            Poll::Ready(())
        })
        .await;
        // Real query mutation retires the old coarse request and private scene.
        // Re-establish the new native Files entry through its original decoder.
        state.write().set_search_query("other".into());
        state.write().set_search_query("design notes".into());
        let (request, transport, client, session) = {
            let mut s = state.write();
            let r = super::super::begin(&mut s, &f.team, "design notes", false).unwrap();
            (
                r,
                s.core.transport.clone().unwrap(),
                s.core.client.clone(),
                s.core.session.as_ref().unwrap().workspaces[&f.team].clone(),
            )
        };
        super::super::files::load_scoped_with(state,request,&transport,&client,&session,None,|_|std::future::ready(Ok(json!({"ok":true,"files":{"matches":[],"pagination":{"page":1,"page_count":1,"total_count":0}}})))).await;
        let successor = begin(&f, api::Sort::Name);
        let revision = state.peek().search_pager.revision();
        let frame = successor.frame.clone();
        send.send(if late_error {
            Err(Error::Offline("late synthetic failure".into()))
        } else {
            Ok(response(1, false))
        })
        .unwrap();
        assert!(!renderer::run_pumped(&mut f.h._dom, pending).await);
        assert_eq!(state.peek().search_pager.revision(), revision);
        assert!(state.peek().search_pager.loading);
        assert!(owned(&state.peek(), &frame).is_some());
        assert!(finish(&mut f, successor, Ok(response(1, false)), &calls).await);
        assert_eq!(status(&state.peek()).unwrap().6, api::Sort::Name);
    }
    let job = selected(&f, 4, false).unwrap();
    assert!(
        !finish(
            &mut f,
            job,
            Err(Error::Offline("synthetic outage".into())),
            &calls
        )
        .await
    );
    let source = Control::capture(&state.peek()).unwrap();
    let stale = crate::bootstrap::reconnect::resource_test_visit(&mut state.write());
    let current_visit = crate::bootstrap::reconnect::resource_test_visit(&mut state.write());
    assert!(plan_recovery(&mut state.write(), source, stale).is_none());
    let source = Control::capture(&state.peek()).unwrap();
    let job = plan_recovery(&mut state.write(), source, current_visit).unwrap();
    assert_eq!(job.ticket.page, 4);
    state.write().dismiss_overlay();
    let before = calls.get();
    assert!(!finish(&mut f, job, Ok(response(4, false)), &calls).await);
    assert_eq!(calls.get(), before);
    assert!(state.peek().overlay.is_none());
}
