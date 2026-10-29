use super::*;
use crate::app_home::channel_fetches::calendar_member_test_support as renderer;
use crate::bootstrap::test_support::{Harness, pending};
use serde_json::{Value, json};
use std::cell::Cell;
use super_platinum_core::slack::{
    api::file_sort_preference as wire, transport::FileSortPreferenceFailure as Failure,
};
pub(crate) struct Fixture {
    pub(crate) h: Harness,
    team: String,
}
pub(crate) async fn setup(sort: api::Sort) -> Fixture {
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
    let init = json!({"ok":true,"self":{"id":user,"name":"fixture-self","team_id":team,"profile":{}},"team":{"id":team,"name":"Saved Files"},"workspaces":[{"id":team}],"onboarding":{"user_data":{"onboarding_complete":true}},"user_date_created":123,"prefs":{"result_count_toggle":false,"stick_user_sort":true,"search_file_sort":sort.raw(),"search_exclude_bots":false}});
    let channels = json!({"ok":true,"channels":{"channels":[{"id":"CINIT0001","name":"warm-harbor","is_channel":true,"is_member":true}],"ims":[],"mpims":[],"is_open":[],"has_more_mpdms":false,"visitor_channels":[],"temporary_channel_expirations":{}}});
    renderer::run_pumped(
        &mut h._dom,
        initial.complete_channels(state, init, channels),
    )
    .await
    .unwrap();
    {
        let mut s = state.write();
        s.overlay = Some(Overlay::Search);
        s.set_search_kind(SearchKind::Files);
        s.set_search_query("design notes".into());
    }
    assert!(
        file_search_parameters::capture(&state.peek(), &team).is_none(),
        "local-only proof is not widened"
    );
    assert_eq!(
        observation::capture(&state.peek(), &team).unwrap().sort(),
        sort
    );
    Fixture { h, team }
}
pub(crate) fn begin_read(f: &Fixture) -> load::Job {
    let mut state = f.h.state;
    let mut s = state.write();
    let request = super::super::super::begin(&mut s, &f.team, "design notes", false).unwrap();
    let transport = s.core.transport.clone().unwrap();
    let client = s.core.client.clone();
    let session = s.core.session.as_ref().unwrap().workspaces[&f.team].clone();
    begin(&mut s, &request, &transport, &client, &session).expect("actual saved Files entry")
}
pub(crate) async fn finish(
    f: &mut Fixture,
    job: load::Job,
    page: u32,
    empty: bool,
    calls: &Cell<usize>,
) -> bool {
    renderer::run_pumped(
        &mut f.h._dom,
        load::run_with(f.h.state, job, |request, current, observe| {
            assert!(current());
            assert_eq!(request.requested_page(), page);
            calls.set(calls.get() + 1);
            let result = Ok(super::super::tests::response(page, empty));
            observe(&result);
            std::future::ready(result)
        }),
    )
    .await
}
pub(crate) async fn loaded() -> Fixture {
    let mut f = setup(api::Sort::Newest).await;
    let job = begin_read(&f);
    assert!(finish(&mut f, job, 1, false, &Cell::new(0)).await);
    f
}
fn choice(f: &Fixture, sort: api::Sort) -> (load::Job, write::Work) {
    let mut state = f.h.state;
    let source = Source::capture(&state.peek()).unwrap();
    write::plan_choice(&mut state.write(), source, sort).unwrap()
}
fn original_event(f: &Fixture, name: &str, value: Value) {
    let mut state = f.h.state;
    let mut s = state.write();
    let generation = s.core.workspaces[&f.team].rt_generation;
    let raw: Arc<str> =
        json!({"type":"pref_change","name":name,"value":value,"fixture_original":23})
            .to_string()
            .into();
    let carrier = super_platinum_core::slack::realtime::PreferenceCarrier::parse(raw).unwrap();
    file_search_parameters::preference_event(
        &mut s,
        &f.team,
        generation,
        carrier.event(),
        Some(&carrier),
    );
    observation::preference_event(&mut s, &f.team, generation, carrier.event(), Some(&carrier));
}
#[tokio::test]
async fn actual_saved_five_orders_restore_without_saving_and_page_cache_retains_empty_occurrences()
{
    for sort in api::Sort::ALL {
        let mut f = setup(sort).await;
        let calls = Cell::new(0);
        let job = begin_read(&f);
        assert_eq!(job.frame.promoted.sort, sort);
        assert!(
            !f.h.state
                .peek()
                .search_file_save_registry
                .busy(&f.h.state.peek())
        );
        assert!(finish(&mut f, job, 1, false, &calls).await);
        let mut state = f.h.state;
        let source = Control::capture(&state.peek()).unwrap();
        let next = plan(&mut state.write(), source, 2, false).unwrap();
        assert!(finish(&mut f, next, 2, true, &calls).await);
        assert!(rows(&state.peek()).is_empty());
        let source = Control::capture(&state.peek()).unwrap();
        assert!(plan(&mut state.write(), source, 1, false).is_none());
        assert_eq!(
            rows(&state.peek())
                .iter()
                .map(|h| h.id())
                .collect::<Vec<_>>(),
            ["FZ", "FA", "FZ"]
        );
        assert_eq!(calls.get(), 2);
        assert_eq!(
            observation::capture(&state.peek(), &f.team).unwrap().sort(),
            sort
        );
        assert!(
            state
                .peek()
                .search_file_module
                .as_ref()
                .unwrap()
                .saved
                .pending
                .is_none()
        );
        assert!(state.peek().viewer.is_none());
    }
    let f = loaded().await;
    for raw in [
        json!("timestamp"),
        json!("last_read"),
        json!("last_engaged"),
        json!("unknown"),
        json!(null),
    ] {
        original_event(&f, "search_file_sort", raw);
        assert!(observation::capture(&f.h.state.peek(), &f.team).is_none());
    }
    assert!(
        super::super::refresh(f.h.state).await,
        "retired saved ownership is handled even when restart cannot admit current parameters"
    );
    let mut state = f.h.state;
    let (request, transport, client, session) = {
        let mut s = state.write();
        let epoch = s.core.account_epoch;
        let request = s
            .search_pager
            .begin(epoch, &f.team, "design notes", false)
            .unwrap();
        (
            request,
            s.core.transport.clone().unwrap(),
            s.core.client.clone(),
            s.core.session.as_ref().unwrap().workspaces[&f.team].clone(),
        )
    };
    super::super::super::files::load_scoped_with(
        state,
        request,
        &transport,
        &client,
        &session,
        None,
        |_| async {
            panic!("retired owned saved scope cannot enter legacy HTTP");
            #[allow(unreachable_code)]
            Ok(json!({"ok":true}))
        },
    )
    .await;
}
#[tokio::test]
async fn failed_save_retains_loaded_choice_and_explicit_retry_never_refetches_or_accepts_response_echo()
 {
    let mut f = loaded().await;
    let mut state = f.h.state;
    let calls = Cell::new(0);
    let (read, work) = choice(&f, api::Sort::Name);
    assert!(finish(&mut f, read, 1, false, &calls).await);
    let frame = scene(&state.peek()).unwrap().frame.clone();
    write::run_with(state, work, |request, current, _| async move {
        assert_eq!(request.submitted_sort(), api::Sort::Name);
        assert!(current());
        Err(Failure::NotRequested(Error::Offline("synthetic".into())))
    })
    .await;
    assert!(status(&state.peek()).unwrap().contains("not sent"));
    assert_eq!(rows(&state.peek()).len(), 3);
    let retry = RetrySource::capture(&state.peek()).unwrap();
    let old = retry.clone();
    let work = write::plan_retry(&mut state.write(), retry).unwrap();
    assert!(write::plan_retry(&mut state.write(), old).is_none());
    write::run_with(state, work, |_, current, observe| async move {
        assert!(current());
        let e = Error::HttpStatus {
            status: 503,
            retry_after_secs: None,
        };
        observe(&Err(e.clone()));
        Err(Failure::Unconfirmed(e))
    })
    .await;
    assert!(status(&state.peek()).unwrap().contains("confirm"));
    let retry = RetrySource::capture(&state.peek()).unwrap();
    let work = write::plan_retry(&mut state.write(), retry).unwrap();
    write::run_with(state, work, |_, current, observe| async move {
        assert!(current());
        let result =
            Ok(json!({"ok":true,"value":"name_desc","prefs":{"search_file_sort":"created"}}));
        observe(&result);
        result.map_err(Failure::Unconfirmed)
    })
    .await;
    assert_eq!(
        status(&state.peek()),
        Some("File sort preference submitted.")
    );
    let p = observation::capture(&state.peek(), &f.team).unwrap();
    assert_eq!(p.sort(), api::Sort::Name);
    assert!(p.local_choice());
    assert!(Arc::ptr_eq(&scene(&state.peek()).unwrap().frame, &frame));
    assert_eq!(
        calls.get(),
        1,
        "save outcomes do not launch acknowledgment reads"
    );
    assert!(RetrySource::capture(&state.peek()).is_none());
}
#[tokio::test]
async fn physical_reservation_survives_unpolled_drop_socket_retirement_and_actual_fresh_shell_replacement()
 {
    fn require_send<T: Send>() {}
    require_send::<ShellState>();
    let mut f = loaded().await;
    let mut state = f.h.state;
    let (read, work) = choice(&f, api::Sort::Name);
    assert!(state.peek().search_file_save_registry.busy(&state.peek()));
    let unpolled = write::run_with(state, work, |_: wire::Request, _, _| async {
        panic!("unpolled driver never dispatches");
        #[allow(unreachable_code)]
        Ok(json!({"ok":true}))
    });
    drop(unpolled);
    assert!(!state.peek().search_file_save_registry.busy(&state.peek()));
    assert!(finish(&mut f, read, 1, false, &Cell::new(0)).await);
    let (read, work) = choice(&f, api::Sort::NameDescending);
    assert!(finish(&mut f, read, 1, false, &Cell::new(0)).await);
    let sent = Cell::new(false);
    let (observe_reply, wait) = tokio::sync::oneshot::channel();
    let dispatched = &sent;
    let mut driver = Box::pin(write::run_with(
        state,
        work,
        |request, current, observe| async move {
            assert!(current());
            assert_eq!(request.submitted_sort(), api::Sort::NameDescending);
            dispatched.set(true);
            let result = wait.await.unwrap();
            observe(&result);
            result.map_err(Failure::Unconfirmed)
        },
    ));
    pending(driver.as_mut()).await;
    assert!(sent.get());
    original_event(&f, "search_file_sort", json!("name_desc"));
    assert!(
        Source::capture(&state.peek()).is_none(),
        "equal socket observation does not release physical occupancy"
    );
    let (mut loaded, account, team, user, epoch) = {
        let s = state.peek();
        (
            ShellState::fixture(crate::media::MediaRegistry::default()),
            s.core.active_account.clone(),
            s.core.active_team.clone(),
            s.core.workspaces[&f.team].self_user_id.clone(),
            s.core.account_epoch,
        )
    };
    crate::app_home::channel_fetches::fixture::install_people(&mut loaded);
    loaded.core.active_account = account;
    loaded.core.active_team = team;
    loaded.core.account_epoch = epoch + 1;
    loaded
        .core
        .workspaces
        .get_mut(&f.team)
        .unwrap()
        .self_user_id = user;
    crate::bootstrap::session::install_loaded(state, loaded);
    assert!(
        state.peek().search_file_save_registry.busy(&state.peek()),
        "actual fresh-shell path retains old physical driver across epoch changes"
    );
    assert!(
        state
            .peek()
            .search_file_save_registry
            .claim(&state.peek())
            .is_none()
    );
    observe_reply.send(Ok(json!({"ok":true}))).unwrap();
    driver.await;
    assert!(!state.peek().search_file_save_registry.busy(&state.peek()));
    assert!(
        state.peek().search_file_module.is_none(),
        "old completion cannot install saved UI or parameters in fresh shell"
    );
}
