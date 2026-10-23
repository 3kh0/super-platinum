//! Save-owned channel ranking facts. No popup Search or selection lease is used.
use super::*;

pub(crate) struct ChannelRank {
    pub(crate) selectors: Value,
    pub(crate) history: History,
    pub(crate) cache: autocomplete::Ranking,
    pub(crate) now_ms: f64,
    context: Arc<crate::scheduled::Context>,
    source: Arc<canonical::Canonical>,
    navigation: autocomplete::Navigation,
    preferences_revision: u64,
    ids: BTreeSet<String>,
    native: Value,
    draft_revision: u64,
}

impl ChannelRank {
    /// The caller retains its original Save policy and edit revision. This
    /// observation owns only the shared ranking inputs consumed by that Save.
    pub(crate) async fn capture(
        state: Signal<ShellState>,
        source: Arc<canonical::Canonical>,
        rows: &[Value],
        proof: &Value,
        current: impl Fn(&ShellState) -> bool,
    ) -> Result<Self, Error> {
        let (context, account, preferences_revision, ids) = {
            let shell = state.peek();
            if !current(&shell) || !source.current(&shell) {
                return Err(unavailable());
            }
            let context =
                Arc::new(crate::scheduled::Context::capture(&shell).ok_or_else(unavailable)?);
            let account = shell.core.active_account.clone().ok_or_else(unavailable)?;
            let mut ids = BTreeSet::new();
            for row in rows {
                let id = row["id"].as_str().ok_or_else(unavailable)?;
                if source.row(id) != Some(row) || !ids.insert(id.to_owned()) {
                    return Err(unavailable());
                }
            }
            (
                context,
                account,
                autocomplete::preferences_revision(&shell),
                ids,
            )
        };
        let team = &context.session.team_id;
        let live = |shell: &ShellState| {
            current(shell)
                && context.current(shell)
                && source.current_for(shell, team)
                && preferences_revision == autocomplete::preferences_revision(shell)
        };
        let history = history(&proof["userPreferences"])?;
        let (experiments, _, _) = experiments(&proof["assignments"])?;
        let cache = autocomplete::Ranking::capture(state, context.clone(), &live)?;
        let navigation = autocomplete::Navigation::capture(state, &account, team, &live).await?;
        let (native, draft_revision) = {
            let shell = state.peek();
            if !live(&shell) || !cache.current(&shell) || !navigation.current(&shell) {
                return Err(unavailable());
            }
            (
                native(&shell, team, &ids)?,
                draft_revision(&shell, team).ok_or_else(unavailable)?,
            )
        };
        let mut selectors = json!({"selfId":context.session.user_id,"teamId":team,
            "enterprise":context.session.enterprise_id.is_some(),"metadata":native["metadata"],
            "navigationHistory":navigation.value(),"experiments":experiments,
            "offline":{"channel":{}},"entityCtr":{"channel":cache.offline().entity_ctr(Entity::Channel)},
            "channelPriority":{}});
        for id in &ids {
            if let Some(f) = cache.offline().features(id, Entity::Channel) {
                selectors["offline"]["channel"][id] =
                    json!({"team_ctr_90dd":f.team_ctr_90dd,"user_ctr_30dd":f.user_ctr_30dd});
            }
            selectors["channelPriority"][id] = json!(cache.offline().priority(id));
        }
        let value = Self {
            selectors,
            history,
            cache,
            now_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|_| unavailable())?
                .as_secs_f64()
                * 1000.0,
            context,
            source,
            navigation,
            preferences_revision,
            ids,
            native,
            draft_revision,
        };
        if !current(&state.peek()) || !value.current(&state.peek()) {
            return Err(unavailable());
        }
        Ok(value)
    }

    pub(crate) fn current(&self, shell: &ShellState) -> bool {
        let team = &self.context.session.team_id;
        self.context.current(shell)
            && self.source.current_for(shell, team)
            && self.cache.current(shell)
            && self.navigation.current(shell)
            && self.preferences_revision == autocomplete::preferences_revision(shell)
            && draft_revision(shell, team) == Some(self.draft_revision)
            && native(shell, team, &self.ids).is_ok_and(|value| value == self.native)
    }
}
