//! Save-owned mixed member/group ranking inputs; no popup query is borrowed.
use super::*;

pub(crate) struct SaveMemberRank {
    pub(crate) selectors: Value,
    pub(crate) history: History,
    pub(crate) cache: autocomplete::Ranking,
    pub(crate) now_ms: f64,
    pub(crate) passthrough: bool,
    context: Arc<crate::scheduled::Context>,
    names: Arc<member_names::SaveMemberNames>,
    source: Arc<Snapshot>,
    groups: Arc<group_source::Snapshot>,
    navigation: autocomplete::Navigation,
    preferences_revision: u64,
    native: Value,
    ids: BTreeSet<String>,
    draft_revision: u64,
}

impl SaveMemberRank {
    /// Member rows must be exact original local objects, before renderer-derived
    /// fields. Both canonical inventories remain attached through final Save.
    pub(crate) async fn capture(
        state: Signal<ShellState>,
        names: Arc<member_names::SaveMemberNames>,
        source: Arc<Snapshot>,
        groups: Arc<group_source::Snapshot>,
        rows: &[Value],
        proof: &Value,
        lower: impl Fn(&str) -> Result<String, rank::Error>,
        current: impl Fn(&ShellState) -> bool,
    ) -> Result<Self, Error> {
        let (context, account, preferences_revision) = {
            let shell = state.peek();
            if !current(&shell)
                || !names.current(&shell)
                || !source.current(&shell)
                || !groups.current(&shell)
            {
                return Err(unavailable());
            }
            let context =
                Arc::new(crate::scheduled::Context::capture(&shell).ok_or_else(unavailable)?);
            if proof["workspace"].as_str() != Some(&context.session.team_id)
                || proof["user"].as_str() != Some(&context.session.user_id)
            {
                return Err(unavailable());
            }
            (
                context,
                shell.core.active_account.clone().ok_or_else(unavailable)?,
                autocomplete::preferences_revision(&shell),
            )
        };
        let live = |shell: &ShellState| {
            current(shell)
                && context.current(shell)
                && names.current(shell)
                && source.current(shell)
                && groups.current(shell)
                && preferences_revision == autocomplete::preferences_revision(shell)
        };
        let history = ranking::history(&proof["userPreferences"])?;
        let (experiments, passthrough, _) = ranking::experiments(&proof["assignments"])?;
        let originals = source
            .rows()
            .iter()
            .map(|r| r.source.clone())
            .collect::<Vec<_>>();
        let mut members = admitted(&originals, rows)?;
        let self_id = &context.session.user_id;
        let team = &context.session.team_id;
        if names.gantry["self"]["id"].as_str() != Some(self_id.as_str()) {
            return Err(unavailable());
        }
        members.insert(self_id.clone(), names.gantry["self"].clone());
        let ids = members.keys().cloned().collect();
        let cache = autocomplete::Ranking::capture(state, context.clone(), &live)?;
        let navigation = autocomplete::Navigation::capture(state, &account, team, &live).await?;
        let (native, draft_revision) = {
            let shell = state.peek();
            if !live(&shell) || !cache.current(&shell) || !navigation.current(&shell) {
                return Err(unavailable());
            }
            (
                native(&shell, team, &ids)?,
                revision(&shell, team).ok_or_else(unavailable)?,
            )
        };
        let mut observed = BTreeMap::new();
        for (id, row) in &members {
            let mut row = row.clone();
            row["is_self"] = json!(id == self_id);
            let comparison = names
                .policy
                .member_name(&row, "Name hidden")
                .map_err(|_| unavailable())?;
            observed.insert(id.clone(), selector(&derive(&row, &lower)?, &comparison)?);
        }
        let mut ims = serde_json::Map::new();
        for (id, row) in native["ims"].as_object().ok_or_else(unavailable)? {
            let comparison = observed
                .get(id)
                .and_then(|r| r["facts"]["comparisonName"].as_str())
                .ok_or_else(unavailable)?;
            ims.insert(id.clone(), selector(row, comparison)?);
        }
        if names.enterprise.is_some()
            && names.default_workspace.is_none()
            && observed
                .values()
                .any(|row| row["enterprise_id"].as_str() == names.enterprise.as_deref())
        {
            return Err(unavailable());
        }
        let mut selectors = json!({"selfId":self_id,"self":observed.get(self_id),"teamId":team,
            "enterpriseId":names.enterprise,"enterprise":names.enterprise.is_some(),
            "searcherTeams":names.policy.workspace_ids().collect::<Vec<_>>(),"defaultTeam":names.default_workspace,
            "members":observed,"ims":ims,"metadata":native["metadata"],"navigationHistory":navigation.value(),
            "experiments":experiments,"offline":{"channel":{},"mpim":{}},"entityCtr":{},
            "remoteDm":{},"remoteMpim":{},"affinity":{},"channelPriority":{}});
        for kind in [Entity::Member, Entity::Channel, Entity::Mpim, Entity::Im] {
            selectors["entityCtr"][kind.key()] = json!(cache.offline().entity_ctr(kind));
        }
        selectors["entityCtr"]["userGroup"] = json!(cache.offline().other_ctr());
        for id in members.keys() {
            for kind in [Entity::Channel, Entity::Mpim] {
                if let Some(f) = cache.offline().features(id, kind) {
                    selectors["offline"][kind.key()][id] =
                        json!({"team_ctr_90dd":f.team_ctr_90dd,"user_ctr_30dd":f.user_ctr_30dd});
                }
            }
            if let Some(v) = cache.offline().dm(id) {
                selectors["remoteDm"][id] = v.clone();
            }
            if let Some(v) = cache.offline().mpdm(id) {
                selectors["remoteMpim"][id] = v.clone();
            }
            selectors["affinity"][id] = json!(cache.offline().affinity(id));
            selectors["channelPriority"][id] = json!(cache.offline().priority(id));
        }
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| unavailable())?
            .as_secs_f64()
            * 1000.0;
        if !now_ms.is_finite() || !live(&state.peek()) {
            return Err(unavailable());
        }
        let result = Self {
            selectors,
            history,
            cache,
            now_ms,
            passthrough,
            context,
            names,
            source,
            groups,
            navigation,
            preferences_revision,
            native,
            ids,
            draft_revision,
        };
        if !current(&state.peek()) || !result.current(&state.peek()) {
            return Err(unavailable());
        }
        Ok(result)
    }

    pub(crate) fn current(&self, shell: &ShellState) -> bool {
        let team = &self.context.session.team_id;
        self.context.current(shell)
            && self.names.current(shell)
            && self.source.current(shell)
            && self.groups.current(shell)
            && self.cache.current(shell)
            && self.navigation.current(shell)
            && self.preferences_revision == autocomplete::preferences_revision(shell)
            && revision(shell, team) == Some(self.draft_revision)
            && native(shell, team, &self.ids).is_ok_and(|v| v == self.native)
    }
}
