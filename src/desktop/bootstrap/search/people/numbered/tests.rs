use super::*;
use crate::app_home::channel_fetches::calendar_member_test_support as renderer;
use serde_json::json;
struct Fixture {
    h: crate::bootstrap::test_support::Harness,
    team: String,
    transport: Arc<Transport>,
    client: SlackClient,
    session: WorkspaceSession,
}
async fn setup() -> Fixture {
    setup_with_carrier(true).await
}
async fn setup_with_carrier(has_prefs: bool) -> Fixture {
    let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
    crate::app_home::channel_fetches::fixture::install_people(&mut s);
    assert!(s.core.cache.is_none());
    let team = s.core.active_team.clone().unwrap();
    renderer::connect(&mut s).unwrap();
    let initial=renderer::assignments(&mut s,&json!({"ok":true,"assignments":{
        "enterprise_search":{"group":"off"},"search_normalize_visible_external_users":{"group":"off"},"preserve_omitted_user_change_member_fields":{"group":"on"}}})).unwrap();
    let mut h = crate::message_buttons::inputs::user::directory::read_scope::test_support::mount(s);
    renderer::provide(&h._dom, Rc::new(renderer::Renderer::default()));
    let mut state = h.state;
    renderer::run_pumped(&mut h._dom, initial.drain(state))
        .await
        .unwrap();
    let user = state.peek().core.workspaces[&team].self_user_id.clone();
    let mut init = json!({"ok":true,"self":{"id":user,"name":"fixture-self","team_id":team,"profile":{}},"team":{"id":team,"name":"People pages"},"workspaces":[{"id":team}],"onboarding":{"user_data":{"onboarding_complete":true}},"user_date_created":123,
        "prefs":{"result_count_toggle":false,"stick_user_sort":true,"search_people_sort":"name_desc","search_hide_deactivated_users":false}});
    if !has_prefs {
        init.as_object_mut().unwrap().remove("prefs");
    }
    let channels = json!({"ok":true,"channels":{"channels":[{"id":"CINIT0001","name":"warm-harbor","is_channel":true,"is_member":true}],"ims":[],"mpims":[],"is_open":[],"has_more_mpdms":false,"visitor_channels":[],"temporary_channel_expirations":{}}});
    renderer::run_pumped(
        &mut h._dom,
        initial.complete_channels(state, init, channels),
    )
    .await
    .unwrap_or_else(|error| panic!("{error:?}; {}", initial.diagnostic(&state.peek())));
    let (transport, client, session) = {
        let mut s = state.write();
        s.overlay = Some(Overlay::Search);
        s.set_search_kind(SearchKind::People);
        s.set_search_query("alex".into());
        if has_prefs {
            assert!(matches!(
                people_parameters::capture(&s, &team).unwrap().sort(),
                api::Sort::NameDescending
            ));
        } else {
            assert!(people_parameters::capture(&s, &team).is_none());
        }
        (
            s.core.transport.clone().unwrap(),
            s.core.client.clone(),
            s.core.session.as_ref().unwrap().workspaces[&team].clone(),
        )
    };
    Fixture {
        h,
        team,
        transport,
        client,
        session,
    }
}
fn begin(f: &Fixture) -> load::Job {
    let mut state = f.h.state;
    let request =
        crate::bootstrap::search::begin(&mut state.write(), &f.team, "alex", false).unwrap();
    load::begin(state, &request, &f.transport, &f.client, &f.session)
        .expect("actual accepted parameters and paired owner")
}
fn response(number: u32) -> serde_json::Value {
    json!({"ok":true,"items":[{"id":"UREADONLY","iid":format!("{number}-a"),"profile":{"display_name":"<b>Never a User</b>","title":"untrusted"}},{"id":"UREADONLY","iid":format!("{number}-b")}],"pagination":{"page":number,"page_count":2,"total_count":4,"last":number*2},"request_id":format!("response-{number}")})
}
async fn complete(
    f: &mut Fixture,
    job: load::Job,
    raw: serde_json::Value,
    calls: &Cell<usize>,
) -> bool {
    renderer::run_pumped(
        &mut f.h._dom,
        load::run_with(f.h.state, job, |_, current, observe| async move {
            assert!(current());
            calls.set(calls.get() + 1);
            let result = Ok(raw);
            observe(&result);
            result
        }),
    )
    .await
}
fn canonical(f: &Fixture) -> serde_json::Value {
    serde_json::to_value(&f.h.state.peek().core.workspaces[&f.team].users).unwrap()
}

#[tokio::test]
async fn actual_reader_numbered_pages_cached_return_duplicates_and_separate_readonly_profile_preserve_membership()
 {
    let mut f = setup().await;
    let mut state = f.h.state;
    let calls = Cell::new(0);
    let original = canonical(&f);
    let job = begin(&f);
    let frame = job.frame.clone();
    assert!(complete(&mut f, job, response(1), &calls).await);
    assert_eq!(
        (
            page_count(&state.peek()),
            total(&state.peek()),
            rows(&state.peek()).len()
        ),
        (2, 4, 2)
    );
    let old = RowSource::capture(&state.peek(), 0).unwrap();
    let duplicate = RowSource::capture(&state.peek(), 1).unwrap();
    assert_eq!(old.id(), duplicate.id());
    assert_ne!(rows(&state.peek())[0].iid(), rows(&state.peek())[1].iid());
    renderer::run_pumped(&mut f.h._dom,profiles::hydrate_with(state,frame.clone(),|ids|async move {
        assert_eq!(ids,["UREADONLY"]);Ok(serde_json::from_value(json!([{"id":"UREADONLY","profile":{"display_name":"Authoritative Alex","title":"Engineer"}}])).unwrap())
    })).await;
    assert_eq!(
        row_user(&state.peek(), 0)
            .unwrap()
            .profile
            .as_ref()
            .unwrap()
            .display_name
            .as_deref(),
        Some("Authoritative Alex")
    );
    assert_eq!(canonical(&f), original);
    let next = Selection::capture(&state.peek(), 2).unwrap();
    let job = plan_selection(&mut state.write(), next).unwrap();
    assert_eq!(
        page(&state.peek()),
        1,
        "loaded page retained while next pending"
    );
    assert!(RowSource::capture(&state.peek(), 0).is_none());
    assert!(old.prepare(&mut state.write()).is_none());
    assert!(complete(&mut f, job, response(2), &calls).await);
    let back = Selection::capture(&state.peek(), 1).unwrap();
    assert!(plan_selection(&mut state.write(), back).is_none());
    assert_eq!(calls.get(), 2, "cached page1 issues zero People requests");
    assert_eq!(page(&state.peek()), 1);
    assert!(
        old.prepare(&mut state.write()).is_none(),
        "cached return cannot revive queued old row"
    );
    let current = RowSource::capture(&state.peek(), 1).unwrap();
    let Some(profiles::Open::ReadOnly(ticket)) = current.prepare(&mut state.write()) else {
        panic!("partial hit opens independent read-only pane")
    };
    {
        let s = state.peek();
        let p = crate::bootstrap::profile_search::presentation(&s, "UREADONLY").unwrap();
        assert!(p.loading);
        assert!(
            p.user.is_none(),
            "no User synthesized from hit or cached card metadata"
        );
    }
    crate::bootstrap::profile_search::run_with(state, ticket, || async {
        Ok(serde_json::from_value(
            json!([{"id":"UREADONLY","profile":{"display_name":"Fresh Profile"}}]),
        )
        .unwrap())
    })
    .await;
    {
        let s = state.peek();
        let p = crate::bootstrap::profile_search::presentation(&s, "UREADONLY").unwrap();
        assert!(!p.loading);
        assert_eq!(
            p.user
                .unwrap()
                .profile
                .as_ref()
                .unwrap()
                .display_name
                .as_deref(),
            Some("Fresh Profile")
        );
    }
    assert_eq!(canonical(&f), original);
}

#[tokio::test]
async fn failed_second_page_exact_retry_and_first_page_retry_keep_loaded_data_and_scope() {
    let mut f = setup().await;
    let mut state = f.h.state;
    let calls = Cell::new(0);
    let job = begin(&f);
    assert!(complete(&mut f, job, response(1), &calls).await);
    let first = scene(&state.peek())
        .unwrap()
        .book
        .active()
        .unwrap()
        .page
        .clone();
    let next = Selection::capture(&state.peek(), 2).unwrap();
    let job = plan_selection(&mut state.write(), next).unwrap();
    assert!(!complete(&mut f, job, json!({"ok":true,"items":false}), &calls).await);
    assert_eq!(page(&state.peek()), 1);
    assert!(Arc::ptr_eq(
        &scene(&state.peek()).unwrap().book.active().unwrap().page,
        &first
    ));
    assert_eq!(total(&state.peek()), 4);
    let retry = Retry::capture(&state.peek()).unwrap();
    assert_eq!(retry.ticket.page, 2);
    let old_retry = retry.clone();
    let job = plan_retry(&mut state.write(), retry).unwrap();
    assert!(plan_retry(&mut state.write(), old_retry).is_none());
    assert!(complete(&mut f, job, response(2), &calls).await);
    assert_eq!(page(&state.peek()), 2);
    assert_eq!(calls.get(), 3);
    let job = begin(&f);
    assert!(!complete(&mut f, job, json!({"ok":true,"items":[]}), &calls).await);
    assert_eq!(page(&state.peek()), 0);
    assert!(state.peek().search_pager.failed);
    let retry = Retry::capture(&state.peek()).unwrap();
    let job = plan_retry(&mut state.write(), retry).unwrap();
    assert!(job.initial.is_some());
    assert_eq!(job.ticket.page, 1);
    assert!(complete(&mut f, job, response(1), &calls).await);
    assert!(!state.peek().search_pager.failed);
    assert_eq!(state.peek().search_pager.page, 1);
}

#[tokio::test]
async fn accepted_equal_preference_retires_held_page_and_metadata_and_keeps_account_rate_observation()
 {
    let mut f = setup().await;
    let mut state = f.h.state;
    let calls = Cell::new(0);
    let job = begin(&f);
    let frame = job.frame.clone();
    assert!(complete(&mut f, job, response(1), &calls).await);
    let old = RowSource::capture(&state.peek(), 0).unwrap();
    let (metadata_send, metadata_receive) = tokio::sync::oneshot::channel();
    let mut metadata = Box::pin(profiles::hydrate_with(
        state,
        frame.clone(),
        |_| async move { metadata_receive.await.unwrap() },
    ));
    renderer::pending(&f.h._dom, metadata.as_mut()).await;
    let next = Selection::capture(&state.peek(), 2).unwrap();
    let job = plan_selection(&mut state.write(), next).unwrap();
    metadata_send
        .send(Ok(serde_json::from_value(
            json!([{"id":"UREADONLY","profile":{"display_name":"Too late"}}]),
        )
        .unwrap()))
        .unwrap();
    renderer::run_pumped(&mut f.h._dom, metadata).await;
    assert!(
        row_user(&state.peek(), 0).is_none(),
        "navigation retires pending detached metadata before new page settles"
    );
    let (send, receive) = tokio::sync::oneshot::channel();
    let mut pending = Box::pin(load::run_with(
        state,
        job,
        |_, current, observe| async move {
            assert!(current());
            let raw = receive.await.unwrap();
            let result = Err(Error::RateLimited {
                retry_after_secs: Some(9),
            });
            observe(&result);
            assert!(!current());
            let _ = raw;
            result
        },
    ));
    renderer::pending(&f.h._dom, pending.as_mut()).await;
    {
        let mut s = state.write();
        let generation = s.core.workspaces[&f.team].rt_generation;
        people_parameters::preference_event(
            &mut s,
            &f.team,
            generation,
            &super_platinum_core::slack::events::RtEvent::PreferencesChanged {
                name: "search_hide_deactivated_users".into(),
                value: json!(false),
            },
        );
        assert!(people_parameters::capture(&s, &f.team).is_some());
        assert!(
            !frame.parameters.current(&s),
            "equal value replaces actual receipt"
        );
    }
    send.send(()).unwrap();
    assert!(!renderer::run_pumped(&mut f.h._dom, pending).await);
    assert!(frame.rate.cooling());
    assert!(retired(&state.peek()));
    assert!(old.prepare(&mut state.write()).is_none());
    // Fresh parameter capture cannot manufacture the old page/selection proof.
    assert!(Retry::capture(&state.peek()).is_none());
    let request =
        crate::bootstrap::search::begin(&mut state.write(), &f.team, "alex", false).unwrap();
    let mut foreign = f.session.clone();
    foreign.token = "foreign-token".into();
    assert!(load::begin(state, &request, &f.transport, &f.client, &foreign).is_none());
    let parameters = people_parameters::capture(&state.peek(), &f.team).unwrap();
    assert!(parameters.current(&state.peek()));
    {
        let mut s = state.write();
        let generation = s.core.workspaces[&f.team].rt_generation;
        people_parameters::preference_event(
            &mut s,
            &f.team,
            generation,
            &super_platinum_core::slack::events::RtEvent::PreferencesChanged {
                name: "result_count_toggle".into(),
                value: json!({"unsupported":true}),
            },
        );
    }
    assert!(people_parameters::capture(&state.peek(), &f.team).is_none());
    assert!(
        load::begin(state, &request, &f.transport, &f.client, &f.session).is_none(),
        "unknown required setting leaves bounded lookup available"
    );
    let missing = setup_with_carrier(false).await;
    {
        let mut missing_state = missing.h.state;
        let request = crate::bootstrap::search::begin(
            &mut missing_state.write(),
            &missing.team,
            "alex",
            false,
        )
        .unwrap();
        assert!(
            load::begin(
                missing_state,
                &request,
                &missing.transport,
                &missing.client,
                &missing.session
            )
            .is_none(),
            "successful init without prefs carrier grants no falsy prefs"
        );
    }
    let stale = setup().await;
    {
        let mut stale_state = stale.h.state;
        assert!(people_parameters::capture(&stale_state.peek(), &stale.team).is_some());
        let mut config = super_platinum_core::slack::SlackClientConfig::default();
        config.xparams.version_ts = "changed-client".into();
        stale_state.write().core.client = SlackClient::new(config);
        let request =
            crate::bootstrap::search::begin(&mut stale_state.write(), &stale.team, "alex", false)
                .unwrap();
        assert!(crate::bootstrap::search::current(
            &stale_state.peek(),
            &request,
            &stale.transport
        ));
        assert!(
            load::begin(
                stale_state,
                &request,
                &stale.transport,
                &stale.client,
                &stale.session
            )
            .is_none(),
            "stale slot and caller cannot agree past current shell client"
        );
    }
    // The new read also survives disposal of its originating render scope.
    let mut disposed = setup().await;
    let disposed_state = disposed.h.state;
    let job = begin(&disposed);
    let (send, receive) = tokio::sync::oneshot::channel();
    let mut read = Box::pin(load::run_with(
        disposed_state,
        job,
        |_, _, observe| async move {
            let result = Ok(receive.await.unwrap());
            observe(&result);
            result
        },
    ));
    renderer::pending(&disposed.h._dom, read.as_mut()).await;
    drop(disposed.h._dom);
    send.send(response(1)).unwrap();
    assert!(!read.await, "disposed source cannot publish or panic");
}

#[tokio::test]
async fn owned_server_sort_restarts_pages_isolates_cached_order_and_retires_prior_completion_without_pref_writes()
 {
    fn projection(job: &load::Job, expected: api::Sort) {
        let entry = Entry {
            query: uuid::Uuid::from_u128(11),
            search: uuid::Uuid::from_u128(12),
            latest_api: uuid::Uuid::from_u128(13),
        };
        let input = job.frame.input(job.ticket.page, &entry);
        assert!(
            input.sort == expected,
            "explicit expected sort through the production projection"
        );
        assert_eq!(input.query, "alex");
        assert_eq!(input.page, job.ticket.page);
        assert_eq!(input.primary_channel, "CINIT0001");
        assert_eq!(
            (input.query_session, input.search_session),
            (entry.query, entry.search)
        );
        assert!(!input.hide_deactivated);
        // The unchanged Core full gate verifies the actual private form's
        // relevant/desc, name/asc and name/desc mapping. This is the SAME
        // projection consumed by run_with, and this invokes the real factory.
        assert!(
            api::request(
                job.frame.parameters.client(),
                job.frame.parameters.session(),
                input
            )
            .is_ok()
        );
    }
    fn ordered(number: u32) -> serde_json::Value {
        // Deliberately asymmetric order, including duplicate IDs: a local sort
        // or ID dedupe must not replace the server's returned array.
        json!({"ok":true,"items":[{"id":"UZ","iid":format!("{number}-z")},{"id":"UA","iid":format!("{number}-a")},{"id":"UA","iid":format!("{number}-again")}],"pagination":{"page":number,"page_count":2,"total_count":6,"last":number*3}})
    }
    let mut f = setup().await;
    let mut state = f.h.state;
    let calls = Cell::new(0);
    let original_users = canonical(&f);
    let original_draft = state.peek().core.composer.text.clone();
    let job = begin(&f);
    projection(&job, api::Sort::NameDescending);
    assert!(complete(&mut f, job, ordered(1), &calls).await);
    assert!(
        SortSource::capture(&state.peek()).is_none(),
        "actual sticky true is never forced into a local branch"
    );
    {
        let mut s = state.write();
        let generation = s.core.workspaces[&f.team].rt_generation;
        people_parameters::preference_event(
            &mut s,
            &f.team,
            generation,
            &super_platinum_core::slack::events::RtEvent::PreferencesChanged {
                name: "stick_user_sort".into(),
                value: json!(false),
            },
        );
    }
    assert!(retired(&state.peek()));
    let parameters = people_parameters::capture(&state.peek(), &f.team).unwrap();
    assert!(parameters.local_sort_allowed());
    let prefs = state.peek().core.workspaces[&f.team]
        .cfm_payload
        .user_prefs
        .clone();
    let job = begin(&f);
    projection(&job, api::Sort::Relevant);
    let rate = job.frame.rate.clone();
    assert!(complete(&mut f, job, ordered(1), &calls).await);
    let old_row = RowSource::capture(&state.peek(), 0).unwrap();
    let old_choice = SortSource::capture(&state.peek()).unwrap();
    let next = Selection::capture(&state.peek(), 2).unwrap();
    let pending_job = plan_selection(&mut state.write(), next).unwrap();
    let (send, receive) = tokio::sync::oneshot::channel();
    let pending_calls = &calls;
    let mut pending = Box::pin(load::run_with(
        state,
        pending_job,
        |_, current, observe| async move {
            assert!(current());
            pending_calls.set(pending_calls.get() + 1);
            let result = Ok(receive.await.unwrap());
            observe(&result);
            assert!(
                !current(),
                "the old sort cannot publish after new-sort intent"
            );
            result
        },
    ));
    renderer::pending(&f.h._dom, pending.as_mut()).await;
    let job = plan_sort(&mut state.write(), old_choice.clone(), api::Sort::Name).unwrap();
    assert!(job.initial.is_some());
    assert_eq!(job.ticket.page, 1);
    assert_eq!(
        page(&state.peek()),
        0,
        "new sort starts with its own empty cache"
    );
    assert!(
        Arc::ptr_eq(&job.frame.rate, &rate),
        "account cooldown survives sort"
    );
    assert!(old_row.prepare(&mut state.write()).is_none());
    assert!(plan_sort(&mut state.write(), old_choice, api::Sort::NameDescending).is_none());
    projection(&job, api::Sort::Name);
    assert!(complete(&mut f, job, ordered(1), &calls).await);
    assert_eq!(
        rows(&state.peek())
            .iter()
            .map(api::Hit::id)
            .collect::<Vec<_>>(),
        ["UZ", "UA", "UA"]
    );
    send.send(ordered(2)).unwrap();
    assert!(!renderer::run_pumped(&mut f.h._dom, pending).await);
    assert_eq!(page(&state.peek()), 1);
    let next = Selection::capture(&state.peek(), 2).unwrap();
    let job = plan_selection(&mut state.write(), next).unwrap();
    projection(&job, api::Sort::Name);
    assert!(complete(&mut f, job, ordered(2), &calls).await);
    let before_cache = calls.get();
    let back = Selection::capture(&state.peek(), 1).unwrap();
    assert!(plan_selection(&mut state.write(), back).is_none());
    assert_eq!(page(&state.peek()), 1);
    assert_eq!(
        calls.get(),
        before_cache,
        "same-sort cached page1 has no People-page request"
    );
    let old_next = Selection::capture(&state.peek(), 2).unwrap();
    let choice = SortSource::capture(&state.peek()).unwrap();
    let job = plan_sort(&mut state.write(), choice, api::Sort::NameDescending).unwrap();
    projection(&job, api::Sort::NameDescending);
    assert!(complete(&mut f, job, ordered(1), &calls).await);
    assert!(plan_selection(&mut state.write(), old_next).is_none());
    let next = Selection::capture(&state.peek(), 2).unwrap();
    let job = plan_selection(&mut state.write(), next)
        .expect("previous sort's cached page2 is unavailable");
    projection(&job, api::Sort::NameDescending);
    assert!(complete(&mut f, job, ordered(2), &calls).await);
    assert_eq!(
        rows(&state.peek())
            .iter()
            .map(api::Hit::id)
            .collect::<Vec<_>>(),
        ["UZ", "UA", "UA"]
    );
    let stale = SortSource::capture(&state.peek()).unwrap();
    let retained = state
        .peek()
        .search_people_module
        .as_ref()
        .unwrap()
        .scene
        .as_ref()
        .unwrap()
        .book
        .active()
        .unwrap()
        .page
        .clone();
    state.write().set_search_query("alex ".into());
    assert!(
        retired(&state.peek()),
        "the existing dialog guard remains strict across raw input edits"
    );
    assert!(
        Arc::ptr_eq(
            &state
                .peek()
                .search_people_module
                .as_ref()
                .unwrap()
                .scene
                .as_ref()
                .unwrap()
                .book
                .active()
                .unwrap()
                .page,
            &retained
        ),
        "retirement does not destroy the loaded data"
    );
    assert!(
        plan_sort(&mut state.write(), stale, api::Sort::Relevant).is_none(),
        "queued sort retains exact rendered raw-query ownership"
    );
    assert!(
        parameters.current(&state.peek()),
        "local sort never replaces source preference receipts"
    );
    assert_eq!(
        state.peek().core.workspaces[&f.team].cfm_payload.user_prefs,
        prefs
    );
    assert_eq!(canonical(&f), original_users);
    assert_eq!(state.peek().core.composer.text, original_draft);
}

mod saved;

#[cfg(test)]
mod account_type;
