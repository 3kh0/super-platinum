//! Profile-owned ordinary-channel ranking observations. Shared account cache
//! and draft presence are reused; no Home/Activity selection owner is borrowed.
use super::*;
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};
use super_platinum_core::{
    block_actions::{home::workflow::directory_observations::Entity, name_policy},
    composer_emoji::history::History,
    slack::api::custom::texty::matching,
};
mod drafts;
pub(super) use drafts::schedule as schedule_drafts;
#[cfg(test)]
mod tests;

#[derive(Clone)]
pub(super) struct Input {
    pub(super) selectors: Value,
    pub(super) history: History,
    pub(super) cache: autocomplete::Ranking,
    pub(super) now_ms: f64,
    pub(super) passthrough: bool,
    pub(super) badge_all_unreads: bool,
    preferences: Arc<Preferences>,
    navigation: autocomplete::Navigation,
    source: Arc<canonical::Canonical>,
    ids: BTreeSet<String>,
    native: Value,
    draft_revision: u64,
}
impl Input {
    pub(super) fn preferences(&self) -> Arc<Preferences> {
        self.preferences.clone()
    }
    pub(super) fn current(&self, shell: &ShellState, search: &Search) -> bool {
        self.base_current(shell, search)
            && draft_revision(shell, search.lease.team()) == Some(self.draft_revision)
            && native(shell, search.lease.team(), &self.ids).is_ok_and(|raw| raw == self.native)
    }
    fn base_current(&self, shell: &ShellState, search: &Search) -> bool {
        search.current(shell)
            && self.source.current_for(shell, search.lease.team())
            && self.cache.current(shell)
            && self.navigation.current(shell)
            && self.preferences.current(shell)
    }
    /// Rejoin only an accepted lazy draft-presence change. A changed local
    /// composer, preference, source, native rank fact or account needs a new query.
    pub(super) fn refresh_drafts(
        &mut self,
        shell: &ShellState,
        search: &Search,
    ) -> Result<(), Error> {
        if !self.base_current(shell, search) {
            return Err(unavailable());
        }
        let current = native(shell, search.lease.team(), &self.ids)?;
        if without_drafts(&current) != without_drafts(&self.native) {
            return Err(unavailable());
        }
        self.selectors["metadata"] = current["metadata"].clone();
        self.native = current;
        self.draft_revision = draft_revision(shell, search.lease.team()).ok_or_else(unavailable)?;
        Ok(())
    }
}

/// The canonical collection is local, not a claim that remote search is complete.
/// Each supplied row must be the original accepted source object, not typed or
/// wire-provided replacement facts. Unsupported categories stay outside scope.
fn admitted_rows(source: &[Value], rows: &[Value]) -> Result<BTreeSet<String>, Error> {
    let source: BTreeMap<_, _> = source
        .iter()
        .filter_map(|row| row["id"].as_str().map(|id| (id, row)))
        .collect();
    let mut ids = BTreeSet::new();
    for row in rows {
        let id = row["id"].as_str().ok_or_else(unavailable)?;
        if !matching::ordinary_local(row) || source.get(id).copied() != Some(row) {
            return Err(unavailable());
        }
        ids.insert(id.to_owned());
    }
    Ok(ids)
}

/// Completed Profile-local preference reads can be reused across queries in the
/// same session, without reusing a stale query's insertion authority.
pub(in crate::profile_edit::sections::rich) struct Preferences {
    context: Arc<crate::scheduled::Context>,
    account: String,
    cookie: String,
    revision: u64,
    history: History,
    experiments: Value,
    assignments: Value,
    user_prefs: Value,
    passthrough: bool,
    badge_all_unreads: bool,
}
impl Preferences {
    #[cfg(test)]
    pub(super) fn fixture(
        context: Arc<crate::scheduled::Context>,
        account: String,
        cookie: String,
        revision: u64,
        assignments: Value,
        user_prefs: Value,
    ) -> Result<Arc<Self>, Error> {
        let history = history(&user_prefs)?;
        let (experiments, passthrough, badge_all_unreads) = experiments(&assignments)?;
        Ok(Arc::new(Self {
            context,
            account,
            cookie,
            revision,
            history,
            experiments,
            assignments,
            user_prefs,
            passthrough,
            badge_all_unreads,
        }))
    }
    pub(super) fn history(&self) -> &History {
        &self.history
    }
    pub(super) fn experiments(&self) -> &Value {
        &self.experiments
    }
    pub(super) fn passthrough(&self) -> bool {
        self.passthrough
    }
    pub(super) fn assignments(&self) -> &Value {
        &self.assignments
    }
    pub(super) fn user_prefs(&self) -> &Value {
        &self.user_prefs
    }
    pub(in crate::profile_edit::sections::rich) fn current(&self, shell: &ShellState) -> bool {
        self.context.current(shell)
            && shell.core.active_account.as_deref() == Some(self.account.as_str())
            && shell.core.session.as_ref().is_some_and(|session| {
                session.d_cookie == self.cookie
                    && session
                        .workspaces
                        .get(&self.context.session.team_id)
                        .is_some_and(|actual| {
                            crate::emoji_preferences::same_session(actual, &self.context.session)
                        })
            })
            && self.revision == autocomplete::preferences_revision(shell)
    }
}
pub(super) async fn capture_preferences(
    state: Signal<ShellState>,
    search: &Search,
    prior: Option<Arc<Preferences>>,
) -> Result<Arc<Preferences>, Error> {
    if !search.current(&state.peek()) {
        return Err(unavailable());
    }
    if let Some(prior) = prior.filter(|p| p.current(&state.peek())) {
        return Ok(prior);
    }
    let (revision, cookie) = {
        let shell = state.peek();
        (
            autocomplete::preferences_revision(&shell),
            shell
                .core
                .session
                .as_ref()
                .ok_or_else(unavailable)?
                .d_cookie
                .clone(),
        )
    };
    let ctx = &search.context;
    let raw = search
        .request(state, name_policy::request::user(&ctx.client, &ctx.session))
        .await?;
    let history = history(&raw)?;
    let user_prefs = raw;
    let raw = search
        .request(state, api::list_experiments(&ctx.client, &ctx.session))
        .await?;
    let (experiments, passthrough, badge_all_unreads) = experiments(&raw)?;
    let prefs = Arc::new(Preferences {
        context: ctx.clone(),
        account: search.lease.account().into(),
        cookie,
        revision,
        history,
        experiments,
        assignments: raw,
        user_prefs,
        passthrough,
        badge_all_unreads,
    });
    if !search.current(&state.peek()) || !prefs.current(&state.peek()) {
        return Err(unavailable());
    }
    Ok(prefs)
}
pub(super) async fn capture(
    mut state: Signal<ShellState>,
    search: &Search,
    source: Arc<canonical::Canonical>,
    rows: &[Value],
) -> Result<Input, Error> {
    let prior = search.preferences(&state.peek());
    let input = capture_with_preferences(state, search, source, rows, prior).await?;
    if !search.retain_preferences(&mut state.write(), input.preferences()) {
        return Err(unavailable());
    }
    Ok(input)
}
pub(super) async fn capture_with_preferences(
    state: Signal<ShellState>,
    search: &Search,
    source: Arc<canonical::Canonical>,
    rows: &[Value],
    prior: Option<Arc<Preferences>>,
) -> Result<Input, Error> {
    {
        let shell = state.peek();
        if !search.current(&shell) || !source.current_for(&shell, search.lease.team()) {
            return Err(unavailable());
        }
    }
    let ids = admitted_rows(source.rows(), rows)?;
    let preferences = capture_preferences(state, search, prior).await?;
    let navigation = search.navigation(state).await?;
    let ctx = &search.context;
    let (native, draft_revision) = {
        let shell = state.peek();
        if !search.current(&shell)
            || !source.current_for(&shell, search.lease.team())
            || !preferences.current(&shell)
            || !navigation.current(&shell)
        {
            return Err(unavailable());
        }
        (
            native(&shell, search.lease.team(), &ids)?,
            draft_revision(&shell, search.lease.team()).ok_or_else(unavailable)?,
        )
    };
    let cache = search.ranking.clone();
    let mut selectors = json!({"selfId":ctx.session.user_id,"teamId":ctx.session.team_id,
        "enterprise":ctx.session.enterprise_id.is_some(),"metadata":native["metadata"],
        "navigationHistory":navigation.value(),"experiments":preferences.experiments,
        "offline":{"channel":{}},"entityCtr":{"channel":cache.offline().entity_ctr(Entity::Channel)},
        "channelPriority":{}});
    for id in &ids {
        if let Some(f) = cache.offline().features(id, Entity::Channel) {
            selectors["offline"]["channel"][id] =
                json!({"team_ctr_90dd":f.team_ctr_90dd,"user_ctr_30dd":f.user_ctr_30dd});
        }
        selectors["channelPriority"][id] = json!(cache.offline().priority(id));
    }
    let value = Input {
        selectors,
        history: preferences.history.clone(),
        cache,
        now_ms: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| unavailable())?
            .as_secs_f64()
            * 1000.0,
        passthrough: preferences.passthrough,
        badge_all_unreads: preferences.badge_all_unreads,
        preferences,
        navigation,
        source,
        ids,
        native,
        draft_revision,
    };
    if !value.current(&state.peek(), search) {
        return Err(unavailable());
    }
    Ok(value)
}

fn history(raw: &Value) -> Result<History, Error> {
    name_policy::UserPrefs::decode(raw).map_err(|_| unavailable())?;
    match raw["prefs"].get("frecency") {
        Some(value) => History::from_pref(value).map_err(|_| unavailable()),
        None => Ok(History::default()), // complete successful prefs observation only
    }
}
fn assignment<'a>(raw: &'a Value, key: &str) -> Result<Option<&'a str>, Error> {
    if raw["ok"] != true || raw.get("error").is_some_and(|v| !v.is_null()) {
        return Err(unavailable());
    }
    raw["assignments"]
        .as_object()
        .ok_or_else(unavailable)?
        .get(key)
        .map(|v| v["group"].as_str().ok_or_else(unavailable))
        .transpose()
}
fn experiments(raw: &Value) -> Result<(Value, bool, bool), Error> {
    let mut values = serde_json::Map::new();
    for key in [
        "autocomplete_affinity_score",
        "autocomplete_affinity_ml_model",
    ] {
        values.insert(key.into(), json!(assignment(raw, key)?));
    }
    Ok((
        Value::Object(values),
        assignment(raw, "autocomplete_flannel_fuzzy_match_passthrough")? == Some("treatment"),
        assignment(raw, "badge_all_unreads")? == Some("on"),
    ))
}
fn draft_revision(shell: &ShellState, team: &str) -> Option<u64> {
    Some(shell.core.workspaces.get(team)?.draft_presence.revision())
}
fn without_drafts(raw: &Value) -> Value {
    let mut value = raw.clone();
    if let Some(rows) = value["metadata"].as_object_mut() {
        for row in rows.values_mut() {
            if let Some(row) = row.as_object_mut() {
                row.remove("draft");
            }
        }
    }
    value
}
/// Actual native selectors; this does not deserialize a typed row into source.
fn native(shell: &ShellState, team: &str, ids: &BTreeSet<String>) -> Result<Value, Error> {
    let ws = shell.core.workspaces.get(team).ok_or_else(unavailable)?;
    let mut metadata = serde_json::Map::new();
    for id in ids {
        let channel = ws.channels.get(id).ok_or_else(unavailable)?;
        let messages = ws.messages.get(id);
        let local = shell
            .core
            .drafts
            .items
            .contains_key(&super_platinum_core::drafts::DraftKey {
                team: team.into(),
                channel: id.clone(),
                thread: None,
            });
        metadata.insert(id.clone(),json!({
            "lastRead":messages.and_then(|m|m.last_read.as_ref()).or(channel.last_read.as_ref()),
            "latest":messages.and_then(|m|m.latest.as_ref()),
            "starred":channel.is_starred,
            "unread":messages.map(|m|m.unread_count>0).unwrap_or_else(||channel.unread_count.is_some_and(|n|n>0)),
            "draft":local||ws.draft_presence.has_channel(id),"localDraft":local,
            "priority":ws.priority_scores.get(id),
        }));
    }
    Ok(json!({"metadata":metadata}))
}
