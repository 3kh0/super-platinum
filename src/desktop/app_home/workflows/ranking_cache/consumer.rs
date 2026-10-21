//! Additional owned consumer of the existing account cache. No second model,
//! offline observation, similarity store, freshness timer or pending namespace.
use super::*;
#[cfg(test)]
mod tests;

/// The caller checks its own view/query both here and when using the returned
/// immutable snapshot. Background cache receipts are account-owned and do not
/// adopt UI or authorize a selection after that query has retired.
pub(in crate::app_home) fn capture_for(
    mut state: Signal<ShellState>,
    context: Arc<Context>,
    current: impl Fn(&ShellState) -> bool,
) -> Result<Snapshot, Error> {
    let (snapshot, jobs, owner, cookie) = {
        let mut s = state.write();
        if !current(&s)
            || (crate::fixture::is_fixture()
                && !crate::app_home::about::management::activity::members::fixture::enabled(&s))
                && !crate::profile_edit::sections::fixture::completion::enabled(&s)
                && !crate::slack_lists::comments::fixture::completion::enabled(&s)
        {
            return Err(unavailable());
        }
        let owner = Account {
            context,
            account: s.core.active_account.clone(),
            cookie: s
                .core
                .session
                .as_ref()
                .ok_or_else(unavailable)?
                .d_cookie
                .clone(),
        };
        if !owner.current(&s) {
            return Err(unavailable());
        }
        if s.app_home
            .workflow_ranking
            .as_ref()
            .is_none_or(|cache| !cache.owner.current(&s))
        {
            s.app_home.workflow_ranking = Some(Cache {
                owner: owner.clone(),
                model: Slot::cold(Model::fallback()),
                offline: Slot::cold(observed::Offline::default()),
                similarity: Arc::new(std::sync::Mutex::new(SimilarityCache::default())),
            });
        }
        let cookie = s
            .core
            .session
            .as_ref()
            .ok_or_else(unavailable)?
            .d_cookie
            .clone();
        let permitted = !cooling(&s, &owner.context);
        let cache = s.app_home.workflow_ranking.as_mut().unwrap();
        let now = Instant::now();
        let mut jobs = Vec::new();
        if permitted {
            if let Some(token) = cache.model.reserve(now) {
                jobs.push((Kind::Model, token));
            }
            let team = &owner.context.session.team_id;
            if team.len() >= 9
                && matches!(team.as_bytes().first(), Some(b'T' | b'E'))
                && team
                    .bytes()
                    .skip(1)
                    .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
            {
                if let Some(token) = cache.offline.reserve(now) {
                    jobs.push((Kind::Offline, token));
                }
            } else if cache.offline.observed.is_none() {
                cache.offline.observed = Some(now);
            }
        }
        let snapshot = (!cache.model.malformed && !cache.offline.malformed).then(|| Snapshot {
            owner: cache.owner.clone(),
            model: cache.model.value.clone(),
            offline: cache.offline.value.clone(),
            similarity: cache.similarity.clone(),
        });
        (snapshot, jobs, owner, cookie)
    };
    for (kind, token) in jobs {
        refresh_for(state, owner.clone(), cookie.clone(), kind, token);
    }
    snapshot.ok_or_else(unavailable)
}

fn outcome_current(s: &ShellState, owner: &Account, cookie: &str) -> bool {
    owner.current(s)
        && s.core
            .session
            .as_ref()
            .is_some_and(|v| v.d_cookie == cookie)
}

fn pending(s: &ShellState, owner: &Account, cookie: &str, kind: Kind, token: u64) -> bool {
    outcome_current(s, owner, cookie)
        && s.app_home
            .workflow_ranking
            .as_ref()
            .filter(|cache| cache.owner.current(s))
            .is_some_and(|cache| {
                match kind {
                    Kind::Model => cache.model.pending,
                    Kind::Offline => cache.offline.pending,
                }
                .is_some_and(|(actual, _)| actual == token)
            })
}

fn retire_cooled(s: &mut ShellState, owner: &Account, cookie: &str, kind: Kind, token: u64) {
    if !pending(s, owner, cookie, kind, token) || !cooling(s, &owner.context) {
        return;
    }
    let cache = s.app_home.workflow_ranking.as_mut().unwrap();
    match kind {
        Kind::Model => cache.model.pending = None,
        Kind::Offline => cache.offline.pending = None,
    }
}

fn refresh_for(
    mut state: Signal<ShellState>,
    owner: Account,
    cookie: String,
    kind: Kind,
    token: u64,
) {
    dioxus::core::spawn_forever(async move {
        let request = {
            let s = state.peek();
            if !pending(&s, &owner, &cookie, kind, token) {
                return;
            }
            if cooling(&s, &owner.context) {
                // The reservation was admitted before a concurrent429. Retire
                // only this unsent job; leave the prior observation untouched.
                drop(s);
                retire_cooled(&mut state.write(), &owner, &cookie, kind, token);
                return;
            }
            match kind {
                Kind::Model => {
                    observed::model_request(&owner.context.client, &owner.context.session)
                }
                Kind::Offline => {
                    observed::offline_request(&owner.context.client, &owner.context.session)
                }
            }
        };
        let result = if crate::profile_edit::sections::fixture::completion::enabled(&state.peek()) {
            crate::profile_edit::sections::fixture::completion::ranking_request(
                &state.peek(),
                &request,
            )
            .map(|(raw, keys)| match kind {
                Kind::Model => Reply::Value(raw),
                Kind::Offline => Reply::Offline(raw, keys),
            })
        } else if crate::slack_lists::comments::fixture::completion::enabled(&state.peek()) {
            crate::slack_lists::comments::fixture::completion::ranking_request(
                &state.peek(),
                &request,
            )
            .map(|(raw, keys)| match kind {
                Kind::Model => Reply::Value(raw),
                Kind::Offline => Reply::Offline(raw, keys),
            })
        } else if crate::fixture::is_fixture() {
            crate::app_home::about::management::activity::members::fixture::ranking_request(
                &state.peek(),
                &request,
            )
            .map(Reply::Value)
        } else {
            match kind {
                Kind::Model => owner
                    .context
                    .transport
                    .execute(request)
                    .await
                    .map(Reply::Value),
                Kind::Offline => owner
                    .context
                    .transport
                    .execute_autocomplete_priorities_current(request, || {
                        pending(&state.peek(), &owner, &cookie, kind, token)
                    })
                    .await
                    .map(|response| {
                        let (raw, channels, users) = response.into_parts();
                        Reply::Priorities(raw, channels, users)
                    }),
            }
        };
        complete_observation(&mut state.write(), &owner, &cookie, kind, token, result);
    });
}

#[cfg(test)]
fn complete(
    s: &mut ShellState,
    owner: &Account,
    cookie: &str,
    kind: Kind,
    token: u64,
    result: Result<Value, Error>,
) {
    complete_observation(s, owner, cookie, kind, token, result.map(Reply::Value));
}
fn complete_observation(
    s: &mut ShellState,
    owner: &Account,
    cookie: &str,
    kind: Kind,
    token: u64,
    result: Result<Reply, Error>,
) {
    // Account-owned outcome accounting outlives a superseded cache reservation.
    // Full credentials, Transport identity and cookie still fence the receipt.
    if !outcome_current(s, owner, cookie) {
        return;
    }
    if let Err(error) = &result {
        failure(s, &owner.context, error);
    }
    // Only the original reservation may adopt a value or retire its pending job.
    if !pending(s, owner, cookie, kind, token) {
        return;
    }
    let cache = s.app_home.workflow_ranking.as_mut().unwrap();
    settle_observation(cache, kind, token, Instant::now(), result);
}

#[cfg(test)]
fn settle(cache: &mut Cache, kind: Kind, token: u64, now: Instant, result: Result<Value, Error>) {
    settle_observation(cache, kind, token, now, result.map(Reply::Value));
}
fn settle_observation(
    cache: &mut Cache,
    kind: Kind,
    token: u64,
    now: Instant,
    result: Result<Reply, Error>,
) {
    match kind {
        Kind::Model => {
            let (value, error) = match result {
                Ok(raw) => (
                    observed::decode_model(&raw.raw()).map(|v| v.unwrap_or_else(Model::fallback)),
                    None,
                ),
                Err(error) => (Ok(Model::fallback()), Some(error)),
            };
            cache.model.settle(token, now, value, error);
        }
        Kind::Offline => {
            let (value, error) = match result {
                Ok(raw) => (raw.offline(), None),
                Err(error) => (Ok(observed::Offline::default()), Some(error)),
            };
            cache.offline.settle(token, now, value, error);
        }
    }
}
