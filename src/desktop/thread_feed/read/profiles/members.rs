//! Original-client raw member reply, isolated from canonical user publication.
use super::*;
use serde_json::Value;

pub(super) async fn load(source: Source, state: Signal<ShellState>) {
    #[cfg(test)]
    {
        // Explicit fixture IO only. Newly visible authors must never turn an
        // otherwise offline mounted unit test into a real Slack read.
        let io = super::test_io::current();
        load_all_with(source, state, move |_transport, request| {
            let io = io.clone();
            async move {
                match io {
                    Some(io) => io.execute(request).await,
                    None => Err(Error::Offline("No scoped profile fixture IO".into())),
                }
            }
        })
        .await;
    }
    #[cfg(not(test))]
    {
        load_batches(
            source,
            state,
            usize::MAX,
            move |transport, request, scope| {
                // The runner supplies THIS batch's exact scope. Dispatch never
                // recaptures a newer scope/permit from public current state.
                let ids = match &request.body {
                    super_platinum_core::slack::client::RequestBody::Json(body) => body
                        .get("updated_ids")
                        .and_then(Value::as_object)
                        .map(|ids| ids.keys().cloned().collect::<Vec<_>>())
                        .unwrap_or_default(),
                    _ => Vec::new(),
                };
                async move {
                    transport
                        .execute_current(request, || {
                            !ids.is_empty()
                                && state.try_peek().is_ok_and(|s| {
                                    scope.acquisition.current(&s, &scope)
                                        && ids.iter().all(|id| {
                                            scope.actors.iter().any(|(key, a)| {
                                                &a.user == id && scope.actor_current(&s, key)
                                            })
                                        })
                                })
                        })
                        .await
                }
            },
        )
        .await;
    }
}
/// Preserve the existing single-response injection used by earlier regressions.
/// Production and the multi-batch journey share the same guarded runner below.
#[cfg(test)]
pub(super) async fn load_with<F, Fut>(source: Source, state: Signal<ShellState>, fetch: F)
where
    F: FnOnce(Arc<Transport>, super_platinum_core::slack::client::PreparedRequest) -> Fut,
    Fut: Future<Output = Result<Value, Error>>,
{
    let mut fetch = Some(fetch);
    load_batches(source, state, 1, move |transport, request, _scope| {
        fetch.take().expect("single-response test injection")(transport, request)
    })
    .await;
}
pub(super) async fn load_all_with<F, Fut>(source: Source, state: Signal<ShellState>, mut fetch: F)
where
    F: FnMut(Arc<Transport>, super_platinum_core::slack::client::PreparedRequest) -> Fut,
    Fut: Future<Output = Result<Value, Error>>,
{
    load_batches(
        source,
        state,
        usize::MAX,
        move |transport, request, _scope| fetch(transport, request),
    )
    .await;
}
async fn load_batches<F, Fut>(
    source: Source,
    mut state: Signal<ShellState>,
    maximum_batches: usize,
    mut fetch: F,
) where
    F: FnMut(
        Arc<Transport>,
        super_platinum_core::slack::client::PreparedRequest,
        Arc<Scope>,
    ) -> Fut,
    Fut: Future<Output = Result<Value, Error>>,
{
    let mut scope = {
        let Ok(shell) = state.try_peek() else { return };
        if !source.admitted_current(&shell) {
            return;
        }
        let Some(scope) = shell
            .thread_feed_reads
            .profiles
            .as_ref()
            .filter(|s| s.current(&shell))
            .cloned()
        else {
            return;
        };
        if scope.started.swap(true, Ordering::AcqRel) {
            return;
        }
        if !scope.acquisition.begin(&source, &shell, &scope) {
            // Admission refused before any owner/request was installed. Do not
            // consume the roster's one driver reservation on that refusal.
            scope.started.store(false, Ordering::Release);
            return;
        }
        scope
    };
    let mut ids = scope
        .actors
        .values()
        .filter(|a| !scope.members.contains_key(&a.user))
        .map(|a| a.user.clone())
        .collect::<Vec<_>>();
    ids.sort();
    ids.dedup();
    // A finite roster from this original native publication only. This index
    // does not add actors or infer authority from the public workspace cache.
    let mut actors = HashMap::<String, Vec<Key>>::new();
    for (key, actor) in &scope.actors {
        actors
            .entry(actor.user.clone())
            .or_default()
            .push(key.clone());
    }
    for original_ids in ids.chunks(100).take(maximum_batches) {
        let ids = {
            let Ok(shell) = state.try_peek() else { return };
            if !scope.acquisition.current(&shell, &scope) {
                return;
            }
            original_ids
                .iter()
                .filter(|id| {
                    actors
                        .get(*id)
                        .is_some_and(|keys| keys.iter().any(|key| scope.actor_current(&shell, key)))
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
        // Serial requests retain the original per-request100 bound and shared
        // Transport rate/Retry-After behavior. No zero-revision freshness claim.
        let result = fetch(source.transport.clone(), request, scope.clone()).await;
        let Ok(shell) = state.try_peek() else { return };
        if !scope.acquisition.current(&shell, &scope) {
            return;
        }
        let Some(rows) = result.ok().and_then(|raw| {
            super_platinum_core::profile_reading::from_reply(&raw, &ids, &scope.team).ok()
        }) else {
            return;
        };
        let mut members = scope.members.clone();
        for row in rows {
            if actors
                .get(&row.user().id)
                .is_some_and(|keys| keys.iter().any(|key| scope.actor_current(&shell, key)))
            {
                members.insert(row.user().id.clone(), Arc::new(Member::new(row)));
            }
        }
        drop(shell);
        let Ok(mut shell) = state.try_write() else {
            return;
        };
        if !scope.acquisition.current(&shell, &scope) {
            return;
        }
        // Publish accepted earlier batches immediately. Follow only this exact
        // successor, never a scope replaced by a peer/refresh/actor owner.
        let next = Arc::new(Scope {
            members,
            retired: Arc::new(AtomicBool::new(false)),
            ..scope.as_ref().clone()
        });
        shell.thread_feed_reads.profiles = Some(next.clone());
        scope = next;
    }
}
