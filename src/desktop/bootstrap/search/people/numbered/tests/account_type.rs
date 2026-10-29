use super::*;
use api::AccountType;
async fn loaded() -> Fixture {
    let mut f = setup().await;
    let job = begin(&f);
    assert!(complete(&mut f, job, response(1), &Cell::new(0)).await);
    f
}
fn choose(f: &Fixture, kind: AccountType) -> load::Job {
    let mut state = f.h.state;
    let source = account_filter::FilterSource::capture(&state.peek()).unwrap();
    account_filter::plan(&mut state.write(), source, kind).unwrap()
}
fn projection(job: &load::Job, kind: AccountType, first_all: bool) {
    assert!(job.frame.controls.account_types == kind);
    assert!(matches!(job.entry, load::EntryMode::FirstAll) == first_all);
    let entry = Entry {
        query: uuid::Uuid::from_u128(1),
        search: uuid::Uuid::from_u128(2),
        latest_api: uuid::Uuid::from_u128(3),
    };
    assert!(
        job.frame.request(job.ticket.page, &entry).is_ok(),
        "same actual closed runtime factory; private wire mapping verified in Core"
    );
    assert_eq!(
        job.frame.input(job.ticket.page, &entry).page,
        job.ticket.page
    );
    assert!(!job.frame.input(job.ticket.page, &entry).hide_deactivated);
    assert!(job.frame.controls.selected_team.is_empty());
}

#[tokio::test]
async fn account_type_server_pages_cache_return_and_aba_keep_order_controls_receipts_and_retire_old_rows()
 {
    let mut f = loaded().await;
    let mut state = f.h.state;
    let calls = Cell::new(0);
    let prefs = state.peek().core.workspaces[&f.team]
        .cfm_payload
        .user_prefs
        .clone();
    let users = canonical(&f);
    let parameters = scene(&state.peek()).unwrap().frame.parameters.clone();
    let old = scene(&state.peek()).unwrap().frame.clone();
    let row = RowSource::capture(&state.peek(), 0).unwrap();
    let stale = account_filter::FilterSource::capture(&state.peek()).unwrap();
    assert!(
        account_filter::plan(&mut state.write(), stale.clone(), AccountType::Any).is_none(),
        "native same-value refusal"
    );
    let job = choose(&f, AccountType::RegularMembers);
    projection(&job, AccountType::RegularMembers, true);
    assert_eq!(job.ticket.page, 1);
    assert!(!old.current(&state.peek()));
    assert!(row.prepare(&mut state.write()).is_none());
    assert!(account_filter::plan(&mut state.write(), stale, AccountType::Deactivated).is_none());
    assert!(complete(&mut f, job, response(1), &calls).await);
    assert_eq!(
        rows(&state.peek())
            .iter()
            .map(api::Hit::id)
            .collect::<Vec<_>>(),
        ["UREADONLY", "UREADONLY"]
    );
    let page1 = scene(&state.peek())
        .unwrap()
        .book
        .active()
        .unwrap()
        .page
        .clone();
    let stale_page1 = account_filter::FilterSource::capture(&state.peek()).unwrap();
    let next = Selection::capture(&state.peek(), 2).unwrap();
    let job = plan_selection(&mut state.write(), next).unwrap();
    projection(&job, AccountType::RegularMembers, false);
    assert!(complete(&mut f, job, response(2), &calls).await);
    let back = Selection::capture(&state.peek(), 1).unwrap();
    assert!(plan_selection(&mut state.write(), back).is_none());
    assert!(Arc::ptr_eq(
        &scene(&state.peek()).unwrap().book.active().unwrap().page,
        &page1
    ));
    assert!(
        account_filter::plan(&mut state.write(), stale_page1, AccountType::Deactivated).is_none(),
        "cached page ABA cannot revive queued filter callback"
    );
    for kind in [AccountType::Deactivated, AccountType::Any] {
        let job = choose(&f, kind);
        projection(&job, kind, true);
        assert!(complete(&mut f, job, response(1), &calls).await);
        assert!(account_filter::current(&state.peek()) == Some(kind));
        assert!(!old.current(&state.peek()));
    }
    assert_eq!(
        calls.get(),
        4,
        "page2 plus three new page1 reads, cached return has no IO"
    );
    assert!(
        parameters.current(&state.peek()),
        "local account filter does not replace any accepted preference receipt"
    );
    assert_eq!(
        state.peek().core.workspaces[&f.team].cfm_payload.user_prefs,
        prefs
    );
    assert_eq!(canonical(&f), users);
}

#[tokio::test]
async fn account_type_failed_first_and_second_pages_retry_only_current_filter_and_keep_loaded_page()
{
    let mut f = loaded().await;
    let mut state = f.h.state;
    let job = choose(&f, AccountType::RegularMembers);
    assert!(
        !load::run_with(state, job, |_, current, observe| async move {
            assert!(current());
            let r = Err(Error::Offline("synthetic".into()));
            observe(&r);
            r
        })
        .await
    );
    let retry = Retry::capture(&state.peek()).unwrap();
    let job = plan_retry(&mut state.write(), retry.clone()).unwrap();
    projection(&job, AccountType::RegularMembers, false);
    assert!(plan_retry(&mut state.write(), retry).is_none());
    assert!(complete(&mut f, job, response(1), &Cell::new(0)).await);
    let retained = scene(&state.peek())
        .unwrap()
        .book
        .active()
        .unwrap()
        .page
        .clone();
    let next = Selection::capture(&state.peek(), 2).unwrap();
    let job = plan_selection(&mut state.write(), next).unwrap();
    assert!(
        !load::run_with(state, job, |_, _, _| async {
            Err(Error::Transport("synthetic page failure".into()))
        })
        .await
    );
    assert!(Arc::ptr_eq(
        &scene(&state.peek()).unwrap().book.active().unwrap().page,
        &retained
    ));
    let stale = Retry::capture(&state.peek()).unwrap();
    let job = choose(&f, AccountType::Deactivated);
    projection(&job, AccountType::Deactivated, true);
    assert!(plan_retry(&mut state.write(), stale).is_none());
    assert!(complete(&mut f, job, response(1), &Cell::new(0)).await);
}

#[tokio::test]
async fn account_type_callbacks_require_rendered_raw_query_account_and_page_currentness_and_retain_account_cooldown()
 {
    let f = loaded().await;
    let mut state = f.h.state;
    let source = account_filter::FilterSource::capture(&state.peek()).unwrap();
    let controls = state
        .peek()
        .search_people_module
        .as_ref()
        .unwrap()
        .controls
        .clone();
    state.write().set_search_query("alex ".into());
    assert!(
        account_filter::plan(&mut state.write(), source, AccountType::RegularMembers).is_none()
    );
    assert!(Arc::ptr_eq(
        &state.peek().search_people_module.as_ref().unwrap().controls,
        &controls
    ));
    let f = loaded().await;
    let mut state = f.h.state;
    let source = account_filter::FilterSource::capture(&state.peek()).unwrap();
    state.write().core.account_epoch += 1;
    assert!(account_filter::plan(&mut state.write(), source, AccountType::Deactivated).is_none());
    let mut f = loaded().await;
    let mut state = f.h.state;
    let source = account_filter::FilterSource::capture(&state.peek()).unwrap();
    let next = Selection::capture(&state.peek(), 2).unwrap();
    let job = plan_selection(&mut state.write(), next).unwrap();
    assert!(
        account_filter::plan(&mut state.write(), source, AccountType::RegularMembers).is_none()
    );
    assert!(complete(&mut f, job, response(2), &Cell::new(0)).await);
    let rate = scene(&state.peek()).unwrap().frame.rate.clone();
    rate.observe(
        &state.peek(),
        &Err(Error::RateLimited {
            retry_after_secs: Some(60),
        }),
    );
    let job = choose(&f, AccountType::RegularMembers);
    assert!(Arc::ptr_eq(&job.frame.rate, &rate));
    let calls = Cell::new(0);
    assert!(
        !load::run_with(state, job, |_, _, _| async {
            calls.set(calls.get() + 1);
            Ok(response(1))
        })
        .await
    );
    assert_eq!(
        calls.get(),
        0,
        "changing filter never bypasses retained account cooling"
    );
    assert!(rate.cooling());
}

#[tokio::test]
async fn account_type_change_during_saved_sort_preserves_reservation_and_success_echo_refreshes_only_new_filter()
 {
    let mut f = loaded().await;
    let mut state = f.h.state;
    let source = saved_sort::SavedSortSource::capture(&state.peek()).unwrap();
    let choice = saved_sort::plan_choice(&mut state.write(), source, api::Sort::Name).unwrap();
    assert!(complete(&mut f, choice.read.unwrap().0, response(1), &Cell::new(0)).await);
    let old = scene(&state.peek()).unwrap().frame.clone();
    let job = choose(&f, AccountType::RegularMembers);
    projection(&job, AccountType::RegularMembers, true);
    assert!(complete(&mut f, job, response(1), &Cell::new(0)).await);
    assert!(
        saved_sort::SavedSortSource::capture(&state.peek()).is_none(),
        "read replacement must not unlock save reservation"
    );
    assert!(
        saved_sort::status(&state.peek())
            .unwrap()
            .starts_with("Saving")
    );
    let job = saved_sort::run_with(state, choice.write, |_, current, observe| async move {
        assert!(current());
        let r = Ok(json!({"ok":true}));
        observe(&r);
        r.map_err(super_platinum_core::slack::transport::PeopleSortPreferenceFailure::Unconfirmed)
    })
    .await
    .unwrap()
    .0;
    projection(&job, AccountType::RegularMembers, false);
    assert!(!old.current(&state.peek()));
    assert!(job.frame.effective_sort() == api::Sort::Name);
    assert!(complete(&mut f, job, response(1), &Cell::new(0)).await);
    assert!(account_filter::current(&state.peek()) == Some(AccountType::RegularMembers));
    assert!(saved_sort::status(&state.peek()).is_none());
    assert!(saved_sort::SavedSortSource::capture(&state.peek()).is_some());
}
