//! Lazy account-owned autocomplete observations. No timer grants data or authority.
//! The actual source permits a cold/stale snapshot while an owned read refreshes.
use super::*;
use super_platinum_core::{
    block_actions::home::workflow::directory_observations as observed,
    composer_emoji::{model::Model, ranking::SimilarityCache},
};

const FRESH: Duration = Duration::from_secs(28 * 60 * 60);
const PENDING: Duration = Duration::from_secs(30);

#[derive(Clone)]
struct Account {
    context: Arc<Context>,
    account: Option<String>,
}
impl Account {
    fn current(&self, s: &ShellState) -> bool {
        self.context.current(s)
            && self.account == s.core.active_account
            && s.core
                .session
                .as_ref()
                .and_then(|session| session.workspaces.get(&self.context.session.team_id))
                .is_some_and(|actual| {
                    crate::emoji_preferences::same_session(actual, &self.context.session)
                })
    }
}
struct Slot<T> {
    value: Arc<T>,
    observed: Option<Instant>,
    pending: Option<(u64, Instant)>,
    malformed: bool,
    // Retain actual transport outcome even when source cache semantics choose
    // their defined fallback. This is not an invented successful HTTP response.
    failure: Option<Error>,
}
impl<T> Slot<T> {
    fn cold(value: T) -> Self {
        Self {
            value: Arc::new(value),
            observed: None,
            pending: None,
            malformed: false,
            failure: None,
        }
    }
    fn reserve(&mut self, now: Instant) -> Option<u64> {
        if self
            .pending
            .is_some_and(|(_, start)| now.saturating_duration_since(start) < PENDING)
            || self
                .observed
                .is_some_and(|at| now.saturating_duration_since(at) <= FRESH)
        {
            return None;
        }
        let token = next();
        self.pending = Some((token, now));
        Some(token)
    }
    fn settle(
        &mut self,
        token: u64,
        now: Instant,
        value: Result<T, observed::Error>,
        failure: Option<Error>,
    ) -> bool {
        if self.pending.is_none_or(|(current, _)| current != token) {
            return false;
        }
        self.pending = None;
        self.observed = Some(now);
        self.failure = failure;
        match value {
            Ok(value) => {
                self.value = Arc::new(value);
                self.malformed = false;
            }
            Err(_) => self.malformed = true,
        }
        true
    }
}
pub(in crate::app_home) struct Cache {
    owner: Account,
    model: Slot<Model>,
    offline: Slot<observed::Offline>,
    similarity: Arc<std::sync::Mutex<SimilarityCache>>,
}
#[derive(Clone)]
pub(super) struct Snapshot {
    owner: Account,
    pub(super) model: Arc<Model>,
    pub(super) offline: Arc<observed::Offline>,
    pub(super) similarity: Arc<std::sync::Mutex<SimilarityCache>>,
}
impl Snapshot {
    pub(super) fn current(&self, s: &ShellState) -> bool {
        self.owner.current(s)
    }
}
#[derive(Clone, Copy)]
enum Kind {
    Model,
    Offline,
}

/// Called from the owned async query path, never from render. Capture the
/// immutable observation now; later cache refresh cannot mutate this snapshot.
pub(super) fn capture(
    mut state: Signal<ShellState>,
    query: &entity_search::Query,
) -> Result<Snapshot, Error> {
    let origin = query.owner().clone();
    let (snapshot, jobs) = {
        let mut s = state.write();
        if !query.current(&s) {
            return Err(unavailable());
        }
        if s.app_home
            .workflow_ranking
            .as_ref()
            .is_none_or(|cache| !cache.owner.current(&s))
        {
            s.app_home.workflow_ranking = Some(Cache {
                owner: Account {
                    context: origin.owner.ctx.clone(),
                    account: s.core.active_account.clone(),
                },
                model: Slot::cold(Model::fallback()),
                offline: Slot::cold(observed::Offline::default()),
                similarity: Arc::new(std::sync::Mutex::new(SimilarityCache::default())),
            });
        }
        let permitted = !cooling(&s, &origin.owner.ctx);
        let cache = s.app_home.workflow_ranking.as_mut().unwrap();
        let now = Instant::now();
        let mut jobs = Vec::new();
        if permitted {
            if let Some(token) = cache.model.reserve(now) {
                jobs.push((Kind::Model, token));
            }
            // Exact uVRc.Am gate for this producer, not general Slack ID syntax.
            let team = &origin.owner.ctx.session.team_id;
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
                // Source returns its constructor defaults without IO for this ID.
                cache.offline.observed = Some(now);
            }
        }
        let snapshot = (!cache.model.malformed && !cache.offline.malformed).then(|| Snapshot {
            owner: cache.owner.clone(),
            model: cache.model.value.clone(),
            offline: cache.offline.value.clone(),
            similarity: cache.similarity.clone(),
        });
        (snapshot, jobs)
    };
    for (kind, token) in jobs {
        refresh(state, origin.clone(), kind, token);
    }
    snapshot.ok_or_else(unavailable)
}
fn refresh(mut state: Signal<ShellState>, origin: Origin, kind: Kind, token: u64) {
    dioxus::core::spawn_forever(async move {
        let request = {
            let s = state.peek();
            let Some(cache) = s
                .app_home
                .workflow_ranking
                .as_ref()
                .filter(|cache| cache.owner.current(&s))
            else {
                return;
            };
            let pending = match kind {
                Kind::Model => cache.model.pending,
                Kind::Offline => cache.offline.pending,
            };
            if pending.is_none_or(|(current, _)| current != token) {
                return;
            }
            match kind {
                Kind::Model => {
                    observed::model_request(&origin.owner.ctx.client, &origin.owner.ctx.session)
                }
                Kind::Offline => {
                    observed::offline_request(&origin.owner.ctx.client, &origin.owner.ctx.session)
                }
            }
        };
        let result = io::read(state, &origin, request).await;
        let mut s = state.write();
        if let Err(error) = &result {
            failure(&mut s, &origin.owner.ctx, error);
        }
        let Some(cache) = s
            .app_home
            .workflow_ranking
            .as_ref()
            .filter(|cache| cache.owner.current(&s))
        else {
            return;
        };
        let pending = match kind {
            Kind::Model => cache.model.pending,
            Kind::Offline => cache.offline.pending,
        };
        if pending.is_none_or(|(current, _)| current != token) {
            return;
        }
        let cache = s.app_home.workflow_ranking.as_mut().unwrap();
        let now = Instant::now();
        match kind {
            Kind::Model => {
                let (value, error) = match result {
                    Ok(raw) => (
                        observed::decode_model(&raw).map(|v| v.unwrap_or_else(Model::fallback)),
                        None,
                    ),
                    Err(error) => (Ok(Model::fallback()), Some(error)),
                };
                cache.model.settle(token, now, value, error);
            }
            Kind::Offline => {
                let (value, error) = match result {
                    Ok(raw) => (observed::Offline::decode(&raw), None),
                    Err(error) => (Ok(observed::Offline::default()), Some(error)),
                };
                cache.offline.settle(token, now, value, error);
            }
        }
    });
}
fn unavailable() -> Error {
    Error::Api("home_workflow_ranking_observation_unavailable".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pending_replacement_rejects_late_completion_and_freshness_is_strict() {
        let now = Instant::now();
        let mut slot = Slot::cold(0u8);
        let first = slot.reserve(now).unwrap();
        assert!(
            slot.reserve(now + PENDING - Duration::from_millis(1))
                .is_none()
        );
        let second = slot.reserve(now + PENDING).unwrap();
        assert_ne!(first, second);
        assert!(!slot.settle(first, now + PENDING, Ok(1), None));
        assert_eq!(*slot.value, 0);
        assert!(slot.settle(second, now + PENDING, Ok(2), None));
        assert!(slot.reserve(now + PENDING + FRESH).is_none());
        assert!(
            slot.reserve(now + PENDING + FRESH + Duration::from_millis(1))
                .is_some()
        );
    }
    #[test]
    fn malformed_is_not_default_and_actual_failure_is_retained_with_source_fallback() {
        let now = Instant::now();
        let mut slot = Slot::cold(0u8);
        let first = slot.reserve(now).unwrap();
        slot.settle(first, now, Err(observed::Error::Model), None);
        assert!(slot.malformed);
        let second = slot.reserve(now + FRESH + Duration::from_secs(1)).unwrap();
        slot.settle(
            second,
            now + FRESH + Duration::from_secs(1),
            Ok(0),
            Some(Error::Offline("synthetic".into())),
        );
        assert!(!slot.malformed);
        assert!(matches!(slot.failure, Some(Error::Offline(_))));
    }
    #[test]
    fn account_cache_retires_on_actual_credential_url_or_transport_replacement() {
        let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
        crate::app_home::fixture::install(&mut s);
        let owner = Account {
            context: Arc::new(Context::capture(&s).unwrap()),
            account: s.core.active_account.clone(),
        };
        assert!(owner.current(&s));
        let team = owner.context.session.team_id.clone();
        let old = s.core.session.as_ref().unwrap().workspaces[&team]
            .url
            .clone();
        s.core
            .session
            .as_mut()
            .unwrap()
            .workspaces
            .get_mut(&team)
            .unwrap()
            .url = "https://replacement.slack.com".into();
        assert!(!owner.current(&s));
        s.core
            .session
            .as_mut()
            .unwrap()
            .workspaces
            .get_mut(&team)
            .unwrap()
            .url = old;
        assert!(owner.current(&s));
        s.core.transport = Some(Arc::new(
            super_platinum_core::slack::Transport::new("synthetic").unwrap(),
        ));
        assert!(!owner.current(&s));
    }
}
