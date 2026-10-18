use super::*;

impl Metadata {
    pub(super) fn retain_workspaces(&mut self, raw: Option<&Value>) {
        let Some(items) = raw.and_then(Value::as_array).filter(|v| !v.is_empty()) else {
            return;
        };
        let mut workspaces = BTreeMap::new();
        let mut internal = Vec::new();
        for item in items {
            let Some(key) = item
                .get("id")
                .and_then(Value::as_str)
                .filter(|v| v.starts_with('T') && id(v))
            else {
                return;
            };
            if !item.is_object() || workspaces.contains_key(key) {
                return;
            }
            let Ok(external) = flag(item, "allow_external_users") else {
                return;
            };
            if !external {
                internal.push(key.into());
            }
            workspaces.insert(key.into(), selected(item, &["id", "enterprise_id", "plan"]));
        }
        self.workspaces.extend(workspaces);
        self.internal_workspaces = Some(internal);
    }

    pub(super) fn retain_account_types(&mut self, raw: Option<&Value>) {
        let Some(raw) = raw.filter(|v| v.is_object()) else {
            return;
        };
        let get = |key| match raw.get(key) {
            None => Some(Vec::new()),
            Some(v) => strings(v),
        };
        let (Some(admin), Some(owner), Some(primary)) =
            (get("is_admin"), get("is_owner"), get("is_primary_owner"))
        else {
            return;
        };
        // kH+p inserts all three booleans only for this union. An unlisted
        // workspace is absent, not a known non-admin workspace.
        for key in admin.iter().chain(&owner).chain(&primary) {
            if key.starts_with('T') {
                self.workspace_admin
                    .insert(key.clone(), admin.contains(key));
            }
        }
    }

    /// UaAK's regular client branch (default appContext=team). Enterprise admin
    /// pages have a separate membership/cap branch and are not represented here.
    pub fn relevant_workspaces(
        &self,
        config: &Configuration,
        channel: &Value,
    ) -> Result<Vec<String>> {
        self.check(config)?;
        if !config.unified.ok_or(Unknown::Configuration)? {
            return Ok(vec![self.team_id.clone()]);
        }
        if flag(channel, "is_org_shared")? {
            let own = self
                .internal_workspaces
                .as_ref()
                .ok_or(Unknown::Membership)?;
            if flag(channel, "is_global_shared")?
                || flag(channel, "is_im")?
                || flag(channel, "is_mpim")?
            {
                return Ok(own.clone());
            }
            let raw = channel
                .get("internalTeamIds")
                .or_else(|| channel.get("internal_team_ids"))
                .ok_or(Unknown::Membership)?;
            let ids = strings(raw).ok_or(Unknown::Shape)?;
            // No useWorkspaceMembership fallback: JM7a uses the default false.
            return Ok(ids.into_iter().filter(|key| own.contains(key)).collect());
        }
        let key = channel
            .get("context_team_id")
            .and_then(Value::as_str)
            .filter(|v| v.starts_with('T') && id(v))
            .ok_or(Unknown::Membership)?;
        Ok(vec![key.into()])
    }

    /// Exact contextual admin observation. Missing modern workspace entries
    /// require users.info; global is_admin cannot fill them in.
    pub fn channel_admin(&self, config: &Configuration, channel: &Value) -> Result<bool> {
        let teams = self.relevant_workspaces(config, channel)?;
        let unified = config.unified.ok_or(Unknown::Configuration)?;
        let modern = self.modern_workspace_roles(config)?;
        let mut unknown = false;
        for team in teams {
            let result = if modern {
                either(
                    self.enterprise_admin(config, &team),
                    self.workspace_role(&team),
                )
            } else if !unified && config.enterprise.is_none() && self.boot_enterprise.is_none() {
                // The non-Grid legacy branch targets the selected workspace.
                // Grid legacy accountTypes/team.admins.list is intentionally
                // unknown until an actual complete role inventory is acquired.
                flag(&self.member, "is_admin")
            } else {
                Err(Unknown::Role)
            };
            match result {
                Ok(true) => return Ok(true),
                Ok(false) => {}
                Err(_) => unknown = true,
            }
        }
        if unknown {
            Err(Unknown::Role)
        } else {
            Ok(false)
        }
    }

    /// Only missing modern workspace roles that an existing users.info read can
    /// acquire. Query after an actual ChannelAdmin dependency failure; this is
    /// not a grant and never refreshes already-known roles. Pending/invalid roles
    /// remain missing; the caller owns deduplication of its in-flight requests.
    /// Unsupported legacy/enterprise-only uncertainty is not an empty inventory.
    pub fn missing_workspace_roles(
        &self,
        config: &Configuration,
        channel: &Value,
    ) -> Result<Vec<String>> {
        let teams = self.relevant_workspaces(config, channel)?;
        if self.channel_admin(config, channel).is_ok() {
            return Ok(Vec::new());
        }
        if !self.modern_workspace_roles(config)? || config.enterprise.is_none() {
            return Err(Unknown::Role);
        }
        let mut missing = Vec::new();
        for team in teams {
            if self.enterprise_admin(config, &team) == Ok(true)
                || self.workspace_role(&team).is_ok()
            {
                continue;
            }
            if !team.starts_with('T') || !self.workspaces.contains_key(&team) {
                return Err(Unknown::Membership);
            }
            if !missing.contains(&team) {
                missing.push(team);
            }
        }
        if missing.is_empty() {
            Err(Unknown::Role)
        } else {
            Ok(missing)
        }
    }

    fn modern_workspace_roles(&self, config: &Configuration) -> Result<bool> {
        self.check(config)?;
        let unified = config.unified.ok_or(Unknown::Configuration)?;
        let local_member = self
            .member
            .get("team_id")
            .and_then(Value::as_str)
            .filter(|key| key.starts_with('T') && id(key));
        let has_local =
            local_member.is_some() && self.member.get("enterprise_id").is_none_or(Value::is_null);
        let has_enterprise = config.enterprise.is_some()
            && self
                .member
                .get("enterprise_user")
                .is_some_and(Value::is_object);
        Ok(unified && (!self.workspace_admin.is_empty() || has_local || has_enterprise))
    }

    fn workspace_role(&self, team: &str) -> Result<bool> {
        if !team.starts_with('T') {
            return Ok(false);
        }
        if self.reads.contains_key(team) || self.invalid_workspace_roles.contains(team) {
            return Err(Unknown::Role);
        }
        self.workspace_admin.get(team).copied().ok_or(Unknown::Role)
    }

    fn enterprise_admin(&self, config: &Configuration, team: &str) -> Result<bool> {
        let Some(enterprise) = config.enterprise.as_deref() else {
            return Ok(false);
        };
        let member = self
            .member
            .get("enterprise_user")
            .filter(|v| v.is_object())
            .ok_or(Unknown::Role)?;
        // A contradictory explicit identity must never be rebound to this org.
        if member
            .get("enterprise_id")
            .is_some_and(|v| v.as_str() != Some(enterprise))
        {
            return Err(Unknown::Identity);
        }
        if !flag(member, "is_admin")? {
            return Ok(false);
        }
        if team.starts_with('E') {
            return Ok(team == enterprise);
        }
        let workspace = self.workspaces.get(team).ok_or(Unknown::Membership)?;
        match workspace.get("enterprise_id") {
            None | Some(Value::Null) => Ok(false),
            Some(Value::String(owner)) => Ok(owner == enterprise),
            Some(_) => Err(Unknown::Shape),
        }
    }
}

fn either(left: Result<bool>, right: Result<bool>) -> Result<bool> {
    match (left, right) {
        (Ok(true), _) | (_, Ok(true)) => Ok(true),
        (Ok(false), Ok(false)) => Ok(false),
        _ => Err(Unknown::Role),
    }
}
