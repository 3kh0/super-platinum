use super::*;
use serde_json::Value;
use super_platinum_core::slack::PreparedRequest;

struct Lease {
    scope: Arc<Scope>,
    job: Arc<Job>,
}
impl Drop for Lease {
    fn drop(&mut self) {
        if let Ok(mut p) = self.scope.phase.lock() {
            if p.job.as_ref().is_some_and(|j| Arc::ptr_eq(j, &self.job)) {
                p.job = None;
            }
        }
    }
}
pub(super) async fn load(
    source: Source,
    state: Signal<ShellState>,
    scope: Arc<Scope>,
    job: Arc<Job>,
) {
    #[cfg(test)]
    run(source, state, scope, job, |_, request, _| async move {
        profiles::fixture_member_read(request).await
    })
    .await;
    #[cfg(not(test))]
    {
        let dispatch = source.clone();
        let owned = scope.clone();
        run(source, state, scope, job, move |transport, request, job| {
            let source = dispatch.clone();
            let scope = owned.clone();
            let ids = match &request.body {
                super_platinum_core::slack::client::RequestBody::Json(body) => body["updated_ids"]
                    .as_object()
                    .map(|m| m.keys().cloned().collect::<Vec<_>>())
                    .unwrap_or_default(),
                _ => Vec::new(),
            };
            async move {
                transport
                    .execute_current(request, || {
                        state.try_peek().is_ok_and(|s| {
                            scope.io_current(&source, &s, &job)
                                && !ids.is_empty()
                                && ids.iter().all(|id| {
                                    job.targets.iter().any(|(key, actor)| {
                                        actor == id
                                            && scope.current_id(&s, key, true) == Some(id.as_str())
                                    })
                                })
                        })
                    })
                    .await
            }
        })
        .await;
    }
}
async fn run<F, Fut>(
    source: Source,
    state: Signal<ShellState>,
    scope: Arc<Scope>,
    job: Arc<Job>,
    mut fetch: F,
) where
    F: FnMut(Arc<Transport>, PreparedRequest, Arc<Job>) -> Fut,
    Fut: Future<Output = Result<Value, Error>>,
{
    let lease = Lease {
        scope: scope.clone(),
        job: job.clone(),
    };
    batches(&source, state, &scope, &job, &mut fetch).await;
    let current = state
        .try_peek()
        .is_ok_and(|s| scope.io_current(&source, &s, &job));
    drop(lease);
    // Re-drive only additional genuine visible coordinates. Attempted keys do
    // not automatically retry, including after failure/missing human rows.
    if current {
        schedule(state);
    }
}
async fn batches<F, Fut>(
    source: &Source,
    mut state: Signal<ShellState>,
    scope: &Arc<Scope>,
    job: &Arc<Job>,
    fetch: &mut F,
) where
    F: FnMut(Arc<Transport>, PreparedRequest, Arc<Job>) -> Fut,
    Fut: Future<Output = Result<Value, Error>>,
{
    let ids = job
        .targets
        .values()
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    for batch in ids.chunks(100) {
        let ids = {
            let Ok(s) = state.try_peek() else { return };
            if !scope.io_current(&source, &s, &job) {
                return;
            }
            batch
                .iter()
                .filter(|id| {
                    job.targets.iter().any(|(key, actor)| {
                        actor == *id && scope.current_id(&s, key, true) == Some(id.as_str())
                    })
                })
                .cloned()
                .collect::<Vec<_>>()
        };
        if ids.is_empty() {
            continue;
        }
        let Ok(request) = super_platinum_core::slack::edge::profile_users_info(
            &source.client,
            &source.session,
            &ids,
        ) else {
            return;
        };
        let result = fetch(source.transport.clone(), request, job.clone()).await;
        let Ok(s) = state.try_peek() else { return };
        if !scope.io_current(&source, &s, &job) {
            return;
        }
        let Some(rows) = result.ok().and_then(|raw| {
            super_platinum_core::profile_reading::from_reply(&raw, &ids, &scope.team).ok()
        }) else {
            return;
        };
        let mut labels = Vec::new();
        for row in rows {
            let id = &row.user().id;
            let Some(value) = job.policy.label(row.user()) else {
                continue;
            };
            for (key, actor) in &job.targets {
                if actor == id && scope.current_id(&s, key, true) == Some(id.as_str()) {
                    labels.push((
                        key.clone(),
                        Label {
                            id: id.clone(),
                            value: value.clone(),
                            policy: job.policy.clone(),
                        },
                    ));
                }
            }
        }
        drop(s);
        // Publish label data and notify the typed renderer, never canonical users.
        let Ok(s) = state.try_write() else { return };
        if !scope.io_current(&source, &s, &job) {
            return;
        }
        let Ok(mut p) = scope.phase.lock() else {
            return;
        };
        p.labels.extend(labels);
    }
}
