//! One complete lookup keeps acquisition, fresh source-store receipt and
//! ranked observations together. No returned ID alone authorizes selection.
use super::*;
use std::collections::BTreeSet;
use super_platinum_core::error::NetworkFailure;

pub(in crate::app_home::about::management::activity) struct ResultSet {
    query: Arc<Query>,
    source: store::Snapshot,
    ranked: matching::Ranked,
    // Retain the actual page/marker/wire observations with the result.
    _acquisition: Acquired,
}
impl ResultSet {
    pub(super) fn current(&self, s: &ShellState) -> bool {
        self.source.current(s, &self.query)
            && self.ranked.current(s, &self.query, self.source.rows())
    }
    pub(super) fn rows<'a>(&'a self, s: &ShellState) -> Option<&'a [matching::Row]> {
        self.current(s).then_some(self.ranked.rows.as_slice())
    }
    pub(super) fn generation(&self) -> u64 {
        self.query.generation
    }
}

pub(super) async fn run(mut state: Signal<ShellState>, q: Arc<Query>) -> Result<ResultSet, Error> {
    // Name acquisition can refresh boot metadata, so it precedes the final
    // raw-member hydration and Ready snapshot rather than invalidating them.
    let names = names::capture(state, &q).await?;
    if store::initialize(&mut state.write(), &q)? {
        crate::bootstrap::persist_workspace_metadata(&state, &q.owner.ctx.session.team_id);
    }
    let acquisition = acquire(state, &q).await?;
    let remote = match &acquisition {
        Acquired::Local => BTreeSet::new(),
        Acquired::Remote(pages) => {
            let ids = store::enqueue_search(&mut state.write(), &q, pages);
            crate::bootstrap::persist_workspace_metadata(&state, &q.owner.ctx.session.team_id);
            // Overflow intentionally retires the source view even on error;
            // persist that refusal along with every accepted raw batch.
            ids?
        }
    };
    store::drain(state, &q).await?;
    store::recover_absent(state, &q).await?;
    let recovery_plan = { store::Recovery::capture(&state.peek(), &q)? };
    if let Some(plan) = recovery_plan {
        let received = Arc::new(recovery::acquire(state, &q, plan.requested().to_vec()).await?);
        q.recovery_receipts.lock().unwrap().push(received.clone());
        let enqueued = plan.enqueue(&mut state.write(), &q, &received);
        crate::bootstrap::persist_workspace_metadata(&state, &q.owner.ctx.session.team_id);
        enqueued?;
        store::drain(state, &q).await?;
    }
    store::repair_known(state, &q).await?;
    // Every source slot and native witness must now be complete/current. A
    // failed acquisition or conflicting repair cannot become an empty result.
    let source = store::snapshot(&state.peek(), &q)?;
    drafts::schedule(state, q.clone())?;
    let ranked = matching::capture(state, &q, source.rows(), &remote, &names, |s| {
        source.current(s, &q)
    })
    .await?;
    let result = ResultSet {
        query: q,
        source,
        ranked,
        _acquisition: acquisition,
    };
    if !result.current(&state.peek()) {
        return Err(unavailable());
    }
    Ok(result)
}

/// Called after each accepted draft page, without another network or renderer
/// acquisition. Taking the result out avoids aliasing the serial shell borrow.
pub(in crate::app_home::about::management::activity) fn refresh_drafts(
    s: &mut ShellState,
    q: &Query,
) {
    if !q.current(s) {
        return;
    }
    let Some(result) = document_mut(s, q.visit).and_then(|d| d.activity.member_results.take())
    else {
        return;
    };
    let result_handle = result;
    let mut result = result_handle.lock().unwrap();
    if result.generation() == q.generation && result.source.current(s, &result.query) {
        let ResultSet {
            query,
            source,
            ranked,
            ..
        } = &mut *result;
        let _ = ranked.refresh_drafts(s, query, source.rows());
    }
    if let Some(d) = document_mut(s, q.visit) {
        drop(result);
        d.activity.member_results = Some(result_handle);
    }
}

/// The event's exact presented generation and value must still name a current
/// ranked option. Empty means the independent "Anyone" filter, not a member.
pub(in crate::app_home::about::management::activity) fn choose(
    s: &mut ShellState,
    visit: Visit,
    generation: u64,
    filter_revision: u64,
    id: String,
) {
    if document(s, visit).is_none_or(|d| d.activity.filters.revision != filter_revision) {
        return;
    }
    if id.is_empty() {
        super::super::edit(s, visit, Field::User, id);
        return;
    }
    let chosen = document(s, visit)
        .and_then(|d| d.activity.member_results.as_ref())
        .and_then(|handle| {
            let r = handle.lock().unwrap();
            if r.generation() != generation {
                return None;
            }
            r.rows(s)?
                .iter()
                .find(|row| row.id == id)
                .map(|row| (row.id.clone(), row.label.clone()))
        });
    if let Some(chosen) = chosen {
        super::super::edit(s, visit, Field::User, id);
        if let Some(d) = document_mut(s, visit) {
            d.activity.member_selection = Some(chosen);
        }
    }
}

pub(in crate::app_home::about::management::activity) fn start(
    mut state: Signal<ShellState>,
    visit: Visit,
) {
    let expected = { native::Expected::capture(&state.peek(), visit) };
    let Some(expected) = expected else {
        return;
    };
    // The input event changes the controlled value and generation. Dispatch
    // after that render, otherwise eval can observe the previous DOM value.
    // Capture Expected above so a newer event still retires this callback.
    dioxus::core::queue_effect(move || {
        dioxus::core::spawn_forever(async move {
            if native::snapshot(&expected, None).await.is_err() {
                return;
            }
            let query = {
                let mut s = state.write();
                if !expected.current(&s) {
                    return;
                }
                let q = match Query::capture(&mut s, visit) {
                    Ok(q) => q,
                    Err(error) => {
                        if fixture::enabled(&s) {
                            eprintln!("workflow-activity-member-query-refused: {error:?}");
                        }
                        return;
                    }
                };
                let q = Arc::new(q);
                let d = document_mut(&mut s, visit).unwrap();
                d.activity.member_pending = Some(q.clone());
                d.activity.member_receipt = Some(q.clone());
                d.activity.member_results = None;
                d.activity.member_notice = None;
                q
            };
            let result = run(state, query.clone()).await;
            let mut s = state.write();
            if !query.current(&s) {
                return;
            }
            let d = document_mut(&mut s, visit).unwrap();
            d.activity.member_pending = None;
            match result {
                Ok(results) => {
                    d.activity.member_results = Some(Arc::new(std::sync::Mutex::new(results)))
                }
                Err(error) => {
                    record_failure(&mut s, &query, error);
                }
            }
        });
    });
}

/// This surface has persistent, scoped feedback. A second global toast can
/// cover the metadata actions after returning, and must not replace another
/// feature's toast. Retain the original error for owner-bound recovery.
pub(super) fn record_failure(s: &mut ShellState, query: &Query, error: Error) {
    if !query.current(s) {
        return;
    }
    document_mut(s, query.visit).unwrap().activity.member_notice = Some(
        if error.is_offline() {
            "People are unavailable while offline. Try again when connected."
        } else {
            "People could not be loaded. Try again."
        }
        .into(),
    );
    *query.failure_receipt.lock().unwrap() = Some(error);
}

pub(in crate::app_home::about::management::activity) fn view(
    s: &ShellState,
    visit: Visit,
) -> (u64, Vec<(String, String)>, bool, Option<String>) {
    let Some(d) = document(s, visit) else {
        return (0, vec![], false, None);
    };
    let a = &d.activity;
    let (generation, mut rows) = a
        .member_results
        .as_ref()
        .and_then(|handle| {
            let r = handle.lock().unwrap();
            r.rows(s).map(|rows| {
                (
                    r.generation(),
                    rows.iter()
                        .map(|row| (row.id.clone(), row.label.clone()))
                        .collect::<Vec<_>>(),
                )
            })
        })
        .unwrap_or_default();
    if !a.filters.user.is_empty() && !rows.iter().any(|row| row.0 == a.filters.user) {
        // Display the already admitted filter even after the picker query
        // retires. This retained label is not an option admission receipt.
        let label = a
            .member_selection
            .as_ref()
            .filter(|row| row.0 == a.filters.user)
            .map(|row| row.1.clone())
            .unwrap_or_else(|| "Selected person".into());
        rows.insert(0, (a.filters.user.clone(), label));
    }
    (
        generation,
        rows,
        a.member_pending.as_ref().is_some_and(|q| q.current(s)),
        a.member_notice.clone(),
    )
}
