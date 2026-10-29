use super::*;
use saved_sort::{RetrySource, SavedSortSource};
use super_platinum_core::slack::{
    events::RtEvent, transport::PeopleSortPreferenceFailure as Failure,
};

async fn loaded() -> Fixture {
    let mut f = setup().await;
    let job = begin(&f);
    assert!(complete(&mut f, job, response(1), &Cell::new(0)).await);
    f
}
fn choose(f: &Fixture, sort: api::Sort) -> saved_sort::Choice {
    let mut state = f.h.state;
    let source = SavedSortSource::capture(&state.peek()).unwrap();
    saved_sort::plan_choice(&mut state.write(), source, sort).unwrap()
}
fn socket(
    s: &mut ShellState,
    team: &str,
    name: &str,
    value: serde_json::Value,
) -> Option<sort_refresh::RefreshWork> {
    let source = (name == "search_people_sort")
        .then(|| RefreshSource::capture(s))
        .flatten();
    let generation = s.core.workspaces[team].rt_generation;
    let change = people_parameters::preference_event_with_change(
        s,
        team,
        generation,
        &RtEvent::PreferencesChanged {
            name: name.into(),
            value,
        },
    );
    source
        .zip(change)
        .and_then(|(source, change)| after_sort_change(s, source, &change))
}

#[tokio::test]
async fn saved_choice_reads_immediately_and_current_success_echo_starts_new_read_without_canonical_value_rebinding()
 {
    let mut f = loaded().await;
    let mut state = f.h.state;
    let users = canonical(&f);
    let old = scene(&state.peek()).unwrap().frame.clone();
    let old_row = RowSource::capture(&state.peek(), 0).unwrap();
    let old_selection = Selection::capture(&state.peek(), 2).unwrap();
    let choice = choose(&f, api::Sort::Name);
    let read = choice.read.unwrap().0;
    assert!(!old.current(&state.peek()));
    assert!(read.frame.effective_sort() == api::Sort::Name);
    assert_eq!(read.ticket.page, 1);
    assert!(old_row.prepare(&mut state.write()).is_none());
    assert!(plan_selection(&mut state.write(), old_selection).is_none());
    assert!(SavedSortSource::capture(&state.peek()).is_none());
    assert!(complete(&mut f, read, response(1), &Cell::new(0)).await);
    let first = scene(&state.peek()).unwrap().frame.clone();
    assert!(
        saved_sort::status(&state.peek())
            .unwrap()
            .starts_with("Saving")
    );
    // Write reservation survives replacement of the entire read scene/query.
    state.write().set_search_query("blair".into());
    let request =
        crate::bootstrap::search::begin(&mut state.write(), &f.team, "blair", false).unwrap();
    let read = load::begin(state, &request, &f.transport, &f.client, &f.session).unwrap();
    assert!(complete(&mut f, read, response(1), &Cell::new(0)).await);
    let before_echo = scene(&state.peek()).unwrap().frame.clone();
    assert!(SavedSortSource::capture(&state.peek()).is_none());
    let read = saved_sort::run_with(state, choice.write, |_, current, observe| async move {
        assert!(current());
        let raw = Ok(json!({"ok":true,"prefs":{"search_people_sort":"name_desc"}}));
        observe(&raw);
        raw.map_err(Failure::Unconfirmed)
    })
    .await
    .unwrap()
    .0;
    assert!(!first.current(&state.peek()) && !before_echo.current(&state.peek()));
    assert_eq!(read.frame.query, "blair");
    assert!(
        read.frame.effective_sort() == api::Sort::Name,
        "echoes submitted value, never unproven response pref"
    );
    assert!(complete(&mut f, read, response(1), &Cell::new(0)).await);
    assert!(saved_sort::status(&state.peek()).is_none());
    assert!(SavedSortSource::capture(&state.peek()).is_some());
    assert_eq!(
        people_parameters::capture(&state.peek(), &f.team)
            .unwrap()
            .sort_producer(),
        people_parameters::PreferenceProducer::SubmittedEcho
    );
    assert_eq!(canonical(&f), users);
}

#[tokio::test]
async fn saved_failure_keeps_optimistic_choice_distinguishes_not_sent_and_unknown_and_requires_fresh_explicit_retry()
 {
    let mut f = loaded().await;
    let mut state = f.h.state;
    let choice = choose(&f, api::Sort::Name);
    assert!(complete(&mut f, choice.read.unwrap().0, response(1), &Cell::new(0)).await);
    assert!(
        saved_sort::run_with(state, choice.write, |_, _, _| async {
            Err(Failure::NotRequested(Error::Offline("synthetic".into())))
        })
        .await
        .is_none()
    );
    assert!(
        saved_sort::status(&state.peek())
            .unwrap()
            .contains("not sent")
    );
    assert!(current_sort(&state.peek()) == Some(api::Sort::Name));
    let old_retry = RetrySource::capture(&state.peek()).unwrap();
    let retry = saved_sort::plan_retry(&mut state.write(), old_retry.clone()).unwrap();
    assert!(saved_sort::plan_retry(&mut state.write(), old_retry.clone()).is_none());
    assert!(
        saved_sort::run_with(state, retry, |_, current, observe| async move {
            assert!(current());
            let result = Err(Error::Transport("lost reply".into()));
            observe(&result);
            result.map_err(Failure::Unconfirmed)
        })
        .await
        .is_none()
    );
    assert!(
        saved_sort::status(&state.peek())
            .unwrap()
            .contains("confirm save")
    );
    assert!(saved_sort::plan_retry(&mut state.write(), old_retry).is_none());
    let retry = RetrySource::capture(&state.peek()).unwrap();
    let retry = saved_sort::plan_retry(&mut state.write(), retry).unwrap();
    // The same actual API validator applies to injected and production replies.
    assert!(
        saved_sort::run_with(state, retry, |_, _, _| async {
            Ok(json!({"ok":false,"error":"invalid_auth"}))
        })
        .await
        .is_none()
    );
    assert!(
        saved_sort::status(&state.peek())
            .unwrap()
            .contains("confirm save")
    );
    assert!(current_sort(&state.peek()) == Some(api::Sort::Name));
    let stale = RetrySource::capture(&state.peek()).unwrap();
    state.write().cancel_search_open();
    assert!(saved_sort::plan_retry(&mut state.write(), stale).is_none());
}

#[tokio::test]
async fn ordinary_equal_socket_refresh_does_not_ack_pending_save_and_retired_rate_is_observed_before_nonce_release()
 {
    let mut f = loaded().await;
    let mut state = f.h.state;
    let choice = choose(&f, api::Sort::Name);
    assert!(complete(&mut f, choice.read.unwrap().0, response(1), &Cell::new(0)).await);
    let old = scene(&state.peek()).unwrap().frame.clone();
    let read = socket(
        &mut state.write(),
        &f.team,
        "search_people_sort",
        json!("name"),
    )
    .unwrap()
    .0;
    assert!(!old.current(&state.peek()));
    assert!(complete(&mut f, read, response(1), &Cell::new(0)).await);
    assert!(
        SavedSortSource::capture(&state.peek()).is_none(),
        "ordinary equality cannot release the save reservation"
    );
    assert!(
        saved_sort::status(&state.peek()).is_none(),
        "ordinary event is not acknowledgement"
    );
    // If currentness is gone before dispatch, the injected executor is not run.
    let calls = Cell::new(0);
    assert!(
        saved_sort::run_with(state, choice.write, |_, _, _| async {
            calls.set(calls.get() + 1);
            Ok(json!({"ok":true}))
        })
        .await
        .is_none()
    );
    assert_eq!(calls.get(), 0);
    assert!(SavedSortSource::capture(&state.peek()).is_some());
    let choice = choose(&f, api::Sort::Relevant);
    assert!(complete(&mut f, choice.read.unwrap().0, response(1), &Cell::new(0)).await);
    let rate = scene(&state.peek()).unwrap().frame.rate.clone();
    let team = f.team.clone();
    assert!(
        saved_sort::run_with(state, choice.write, |_, current, observe| async move {
            assert!(current());
            let read = socket(
                &mut state.write(),
                &team,
                "search_people_sort",
                json!("relevant"),
            )
            .unwrap();
            assert!(read.0.frame.effective_sort() == api::Sort::Relevant);
            assert!(!current());
            let result = Err(Error::RateLimited {
                retry_after_secs: Some(60),
            });
            observe(&result);
            result.map_err(Failure::Unconfirmed)
        })
        .await
        .is_none()
    );
    assert!(
        rate.cooling(),
        "actual attempted account rate survives publication retirement"
    );
    assert!(
        saved_sort::status(&state.peek()).is_none(),
        "late completion cannot publish an echo or save status"
    );
}

#[tokio::test]
async fn sort_refresh_requires_exact_preproducer_intent_and_unaffected_receipts_and_never_revives_aba()
 {
    let mut f = loaded().await;
    let mut state = f.h.state;
    let original = scene(&state.peek()).unwrap().frame.clone();
    let stale = RefreshSource::capture(&state.peek()).unwrap();
    // A new page-selection ticket invalidates the pre-producer capsule.
    let next = Selection::capture(&state.peek(), 2).unwrap();
    let job = plan_selection(&mut state.write(), next).unwrap();
    let generation = state.peek().core.workspaces[&f.team].rt_generation;
    let change = people_parameters::preference_event_with_change(
        &mut state.write(),
        &f.team,
        generation,
        &RtEvent::PreferencesChanged {
            name: "search_people_sort".into(),
            value: json!("name"),
        },
    )
    .unwrap();
    assert!(after_sort_change(&mut state.write(), stale, &change).is_none());
    assert!(!complete(&mut f, job, response(2), &Cell::new(0)).await);
    assert!(!original.current(&state.peek()));
    // A fresh explicit search can enter the new receipts; it is not an old read.
    state.write().search_pager.reset();
    let job = begin(&f);
    assert!(complete(&mut f, job, response(1), &Cell::new(0)).await);
    let fresh = scene(&state.peek()).unwrap().frame.clone();
    let read = socket(
        &mut state.write(),
        &f.team,
        "search_people_sort",
        json!("name_desc"),
    )
    .unwrap()
    .0;
    assert!(!original.current(&state.peek()) && !fresh.current(&state.peek()));
    assert!(complete(&mut f, read, response(1), &Cell::new(0)).await);
    let source = RefreshSource::capture(&state.peek()).unwrap();
    let change = people_parameters::preference_event_with_change(
        &mut state.write(),
        &f.team,
        generation,
        &RtEvent::PreferencesChanged {
            name: "search_people_sort".into(),
            value: json!("name"),
        },
    )
    .unwrap();
    assert!(
        socket(
            &mut state.write(),
            &f.team,
            "search_hide_deactivated_users",
            json!(false)
        )
        .is_none()
    );
    assert!(
        after_sort_change(&mut state.write(), source, &change).is_none(),
        "even equal unaffected receipt replacement refuses adoption"
    );
    assert!(SavedSortSource::capture(&state.peek()).is_none());
}
