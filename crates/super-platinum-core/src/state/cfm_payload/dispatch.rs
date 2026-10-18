use super::wire::Node;
use super::*;
pub(super) fn init<A: Adopter>(r: &mut Run<'_, A>, node: &Node) -> Result<(), Error> {
    let v = node.value();
    let self_data = v
        .get("self")
        .filter(|v| v.is_object())
        .ok_or(Error::Malformed("self"))?;
    r.emit(
        Kind::SelfId,
        json!({"user_id":self_data.get("id").ok_or(Error::Malformed("self.id"))?}),
    )?;
    let screen_reader = r
        .adopter
        .facts()
        .screen_reader_enabled
        .ok_or(Error::Unavailable("screen reader observation"))?;
    r.emit(Kind::ScreenReaderEnabled, screen_reader.into())?;
    r.emit(
        Kind::TeamUpsert,
        json!({"team":v.get("team").cloned().unwrap_or(Value::Null)}),
    )?;
    let workspaces = match v.get("workspaces") {
        None | Some(Value::Null) => None,
        Some(v) => Some(v.as_array().ok_or(Error::Malformed("workspaces"))?),
    };
    if let Some(workspaces) = workspaces {
        for team in workspaces {
            r.emit(Kind::TeamUpsert, json!({"team":team}))?;
        }
    }
    r.emit(Kind::MemberUpsert, json!({"members":[self_data]}))?;
    roles::member(r, self_data)?;
    account_types(r, v.get("account_types"))?;
    roles::self_account_types(r, self_data)?;
    if let Some(workspaces) = workspaces.filter(|v| !v.is_empty()) {
        let ids: Vec<String> = workspaces
            .iter()
            .map(|w| {
                w.get("id")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
                    .ok_or(Error::Malformed("workspace.id"))
            })
            .collect::<Result<_, _>>()?;
        // Source compares previous internal+external ids against ALL incoming ids.
        let mut old = Vec::new();
        for id in r
            .adopter
            .facts()
            .workspace_ids
            .ok_or(Error::Unavailable("observed self workspace ids"))?
        {
            if !old.contains(&id) {
                old.push(id);
            }
        }
        if old != ids {
            r.emit(Kind::ResetCommands, Value::Null)?;
        }
        let internal: Vec<_> = workspaces
            .iter()
            .zip(ids)
            .filter(|(w, _)| w.get("allow_external_users") != Some(&json!(true)))
            .map(|(_, id)| id)
            .collect();
        r.emit(Kind::InternalWorkspaces, json!({"internalIds":internal}))?;
        let facts = r.adopter.facts();
        if facts.team.as_ref().is_some_and(|id| enterprise_id(id)) {
            let current = facts
                .workspace_ids
                .ok_or(Error::Unavailable("current internal workspace ids"))?;
            let auth = facts
                .auth_workspace_ids
                .ok_or(Error::Unavailable("actual auth workspace keys"))?;
            if current.iter().any(|id| !auth.contains(id)) {
                r.emit(Kind::ForceAuthAtStartup, Value::Null)?;
            }
        }
    }
    if let Some(prefs) = v.get("prefs").filter(|p| truth(p)) {
        let prefs = prefs::deserialize(prefs)?;
        r.emit(Kind::UserPrefs, prefs.clone())?;
        // Source parses the original all_notifications_prefs independently.
        if let Some(notifications) = prefs.get("all_notifications_prefs") {
            muted(r, notifications)?;
        }
        let hidden = v
            .get("prefs")
            .and_then(|v| v.get("hidden_users"))
            .filter(|v| truth(v))
            .cloned()
            .unwrap_or(json!(""));
        let hidden = hidden.as_str().ok_or(Error::Malformed("hidden_users"))?;
        let map: serde_json::Map<String, Value> = if hidden.is_empty() {
            Default::default()
        } else {
            hidden
                .split(',')
                .filter(|id| *id != "__proto__")
                .map(|id| (id.into(), json!(true)))
                .collect()
        };
        r.emit(Kind::HiddenUsers, Value::Object(map))?;
    }
    if let Some(onboarding) = v.get("onboarding").filter(|v| truth(v)) {
        let mut merged = serde_json::Map::new();
        for key in ["user_data", "team_data"] {
            if let Some(value) = onboarding.get(key).filter(|v| !v.is_null()) {
                merged.extend(
                    value
                        .as_object()
                        .ok_or(Error::Malformed("onboarding data"))?
                        .clone(),
                );
            }
        }
        r.emit(Kind::OnboardingData, Value::Object(merged))?;
        if let Some(v) = onboarding.get("onboarding_channel_id").filter(|v| truth(v)) {
            r.emit(Kind::OnboardingChannel, v.clone())?;
        }
    }
    let onboarding = v.get("onboarding").unwrap_or(&Value::Null);
    emit_truth(r, onboarding, "acquisition_source", Kind::AcquisitionSource)?;
    emit_present(r, &v, "user_date_created", Kind::UserDateCreated)?;
    emit_present(
        r,
        onboarding,
        "should_redirect_joiner_to_slackbot",
        Kind::RedirectSlackbot,
    )?;
    emit_present(
        r,
        onboarding,
        "is_eligible_for_joiner_onboarding_big_mode",
        Kind::EligibleBigMode,
    )?;
    if let Some(value) = onboarding.get("should_redirect_joiner_to_slackbot_big_mode") {
        r.emit(Kind::RedirectBigMode, value.clone())?;
        if truth(value) {
            r.emit(
                Kind::SlackbotPermission,
                json!({"permissions":["ACCESS_SLACK_AI_SLACKBOT"]}),
            )?;
        }
    }
    emit_truth(r, onboarding, "slackbot_channel_id", Kind::SlackbotChannel)
}
fn account_types<A: Adopter>(r: &mut Run<'_, A>, v: Option<&Value>) -> Result<(), Error> {
    let Some(v) = v.filter(|v| truth(v)) else {
        return Ok(());
    };
    let mut groups = Vec::new();
    let mut all = Vec::new();
    for key in ["is_primary_owner", "is_owner", "is_admin"] {
        let list = match v.get(key).filter(|v| truth(v)) {
            None => vec![],
            Some(v) => v
                .as_array()
                .ok_or(Error::Malformed("account types"))?
                .iter()
                .map(|v| {
                    v.as_str()
                        .map(str::to_owned)
                        .ok_or(Error::Malformed("role team id"))
                })
                .collect::<Result<Vec<_>, _>>()?,
        };
        for id in &list {
            if !all.contains(id) {
                all.push(id.clone());
            }
        }
        groups.push(list);
    }
    for id in &all {
        for (kind, list) in [Kind::PrimaryOwner, Kind::Owner, Kind::Admin]
            .into_iter()
            .zip(&groups)
        {
            r.emit(kind, json!({"teamId":id,"value":list.contains(id)}))?;
        }
    }
    let facts = r.adopter.facts();
    if let Some(id) = facts
        .enterprise
        .filter(|id| !id.is_empty())
        .or(facts.team.filter(|id| !id.is_empty()))
    {
        r.emit(Kind::AccountTypes, json!({"teamId":id,"teamIds":all}))?;
    }
    Ok(())
}
fn muted<A: Adopter>(r: &mut Run<'_, A>, v: &Value) -> Result<(), Error> {
    let Some(channels) = v.get("channels").filter(|v| truth(v)) else {
        return Ok(());
    };
    let channels = channels
        .as_object()
        .ok_or(Error::Malformed("notification channels"))?;
    let facts = r.adopter.facts();
    let temporary = facts
        .temp_mute_enabled
        .ok_or(Error::Unavailable("temp_mute assignment"))?;
    let now = facts
        .unix_seconds
        .filter(|v| v.is_finite())
        .ok_or(Error::Unavailable("current unix seconds"))?;
    let mut map = serde_json::Map::new();
    for (id, prefs) in channels {
        if prefs.is_null() {
            return Err(Error::Malformed("notification channel"));
        }
        let permanent = prefs.get("muted").is_some_and(truth);
        let expiration = if !permanent && temporary {
            prefs::number(
                prefs
                    .get("temp_mute_expiration_ts")
                    .filter(|v| !v.is_null())
                    .unwrap_or(&json!(0)),
            )?
        } else {
            0.0
        };
        if permanent || expiration > now {
            map.insert(id.clone(), json!(true));
        }
    }
    // Empty derived set is intentionally NOT dispatched by oiEO.
    if !map.is_empty() {
        r.emit(Kind::MutedChannels, Value::Object(map))?;
        if temporary {
            r.emit(Kind::TempMuteTimer, Value::Object(channels.clone()))?;
        }
    }
    Ok(())
}
pub(super) fn extras<A: Adopter>(r: &mut Run<'_, A>, node: &Node) -> Result<(), Error> {
    let v = node.value();
    // The C handler table order is fixed, independent of response field order.
    emit_truth(r, &v, "feature_usage_data", Kind::FeatureUsage)?;
    emit_truth(r, &v, "salesforce_features", Kind::SalesforceFeatures)?;
    if let Some(prefs) = v.get("team_prefs").filter(|v| truth(v)) {
        if let Some(id) = r.adopter.facts().team.filter(|id| !id.is_empty()) {
            if let Some(p) = prefs.get("toplevel_prefs").filter(|v| truth(v)) {
                r.emit(Kind::TeamPrefs, json!({"teamId":id,"teamPrefs":p}))?;
            }
        }
        if prefs.get("workspace_prefs").is_some_and(truth) {
            let entries = node
                .get("team_prefs")
                .and_then(|v| v.get("workspace_prefs"))
                .and_then(Node::entries)
                .ok_or(Error::Malformed("workspace preferences"))?;
            for (id, p) in entries {
                r.emit(Kind::TeamPrefs, json!({"teamId":id,"teamPrefs":p.value()}))?;
            }
        }
    }
    emit_truth(r, &v, "plan_info", Kind::PlanInfo)?;
    if let Some(m) = v.get("misc").filter(|v| truth(v)) {
        emit_truth(r, m, "team_classification", Kind::TeamClassification)?;
        emit_truth(r, m, "creator_email_type", Kind::CreatorEmailType)?;
        let mut fields = serde_json::Map::new();
        for (src, dst) in [
            ("is_europe", "isEurope"),
            ("is_content_reporting_enabled", "isContentReportingEnabled"),
        ] {
            if let Some(v) = m.get(src) {
                fields.insert(dst.into(), v.clone());
            }
        }
        fields.insert(
            "complianceFlags".into(),
            m.get("compliance_flags")
                .filter(|v| !v.is_null())
                .cloned()
                .unwrap_or(json!([])),
        );
        r.emit(Kind::Misc, Value::Object(fields))?;
        emit_present(r, m, "is_sso_required", Kind::SsoRequired)?;
        if let Some(ts) = m
            .get("transfer_links")
            .and_then(|v| v.get("domains_ts"))
            .filter(|v| truth(v))
        {
            r.emit(Kind::TransferLinks, json!({"serverCacheTs":ts}))?;
        }
        if let Some(migrations) = m.get("external_org_migrations").filter(|v| truth(v)) {
            if let Some(id) = r.adopter.facts().team.filter(|id| !id.is_empty()) {
                r.emit(
                    Kind::TeamUpsert,
                    json!({"team":{"id":id,"external_org_migrations":migrations}}),
                )?;
            }
        }
    }
    if let Some(count) = v.get("stats").and_then(|v| v.get("hidden_messages_count")) {
        r.emit(Kind::Misc, json!({"hidden_messages_count":count}))?;
    }
    if let Some(groups) = v.get("subteams").filter(|v| truth(v)) {
        r.emit(Kind::Subteams, json!({"userGroups":groups}))?;
    }
    if let Some(cache) = v.get("cache_timestamps").filter(|v| truth(v)) {
        if cache.get("emoji_cache_ts").is_some_and(truth) {
            r.emit(
                Kind::EmojiReload,
                json!({"caller":"dispatch-client-extras"}),
            )?;
        }
        if let Some(ts) = cache.get("app_commands_cache_ts").filter(|v| truth(v)) {
            r.emit(
                Kind::AppCommandsReload,
                json!({"cacheTs":ts,"reason":"dispatch-client-extras"}),
            )?;
        }
    }
    if let Some(features) = v.get("features").filter(|v| truth(v)) {
        // Missing and null paid_features remain distinguishable to the adopter.
        let payload = features
            .get("paid_features")
            .map(|v| json!({"present":true,"value":v}))
            .unwrap_or(json!({"present":false}));
        r.emit(Kind::PaidFeatures, payload)?;
        for (key, kind) in [
            ("enterprise_search_enabled", Kind::EnterpriseSearch),
            ("is_eligible_invited_user_glow_up", Kind::InvitedGlowUp),
            ("is_skills_announcement_eligible", Kind::SkillsAnnouncement),
            (
                "is_skills_announcement_day_one_eligible",
                Kind::SkillsDayOne,
            ),
        ] {
            emit_present(r, features, key, kind)?;
        }
    }
    if let Some(id) = v.get("default_workspace").filter(|v| truth(v)) {
        r.emit(Kind::DefaultWorkspace, json!({"id":id}))?;
    }
    if v.get("salesforce_features")
        .and_then(|v| v.get("is_sfdc_auto_slack"))
        == Some(&json!(true))
    {
        r.emit(Kind::SalesforceNux,json!({"reason":"dispatch-client-extras-data-default-on-nux","includeDefaultOnNux":true}))?;
    }
    Ok(())
}
fn emit_truth<A: Adopter>(
    r: &mut Run<'_, A>,
    v: &Value,
    key: &str,
    kind: Kind,
) -> Result<(), Error> {
    if let Some(v) = v.get(key).filter(|v| truth(v)) {
        r.emit(kind, v.clone())?;
    }
    Ok(())
}
fn emit_present<A: Adopter>(
    r: &mut Run<'_, A>,
    v: &Value,
    key: &str,
    kind: Kind,
) -> Result<(), Error> {
    if let Some(v) = v.get(key) {
        r.emit(kind, v.clone())?;
    }
    Ok(())
}
fn enterprise_id(id: &str) -> bool {
    id.len() >= 9
        && id.starts_with('E')
        && id
            .bytes()
            .skip(1)
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
}
