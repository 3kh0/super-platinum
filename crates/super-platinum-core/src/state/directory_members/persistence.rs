use super::*;

impl Store {
    /// Cache metadata contains no token, cookie, transport or runtime authority.
    pub fn cached(&self) -> Option<Value> {
        self.saved
            .as_ref()
            .and_then(|saved| serde_json::to_value(saved).ok())
    }
    /// Legacy absence and malformed payloads stay unavailable; neither becomes
    /// an empty ordered member collection. Account is checked again at every use.
    pub fn restore(raw: Option<&Value>, owner: &WorkspaceSession) -> Self {
        let saved = raw
            .and_then(|raw| serde_json::from_value::<Persisted>(raw.clone()).ok())
            .filter(|saved| valid(saved, owner));
        Self {
            saved,
            epoch: Arc::new(()),
            revision: 0,
        }
    }
}
fn valid(saved: &Persisted, owner: &WorkspaceSession) -> bool {
    if saved.version != 1
        || saved.identity.account.is_empty()
        || saved.identity.team != owner.team_id
        || saved.identity.user != owner.user_id
        || saved.pending.len() > MAX_PENDING_BATCHES
        || saved.pending.iter().map(batch_size).sum::<usize>() > MAX_PENDING_ROWS
    {
        return false;
    }
    let mut ids = BTreeSet::new();
    for row in &saved.records {
        let Ok(id) = member_id(&row.source) else {
            return false;
        };
        if !ids.insert(id.to_owned())
            || member_id(&row.native).ok() != Some(id)
            || member_id(&row.wire).ok() != Some(id)
            || [
                "_real_name_normalized_lc",
                "_display_name_normalized_lc",
                "_email_normalized_lc",
            ]
            .iter()
            .any(|field| !row.source[*field].is_string())
        {
            return false;
        }
        let Ok(native) =
            serde_json::from_value::<crate::slack::models::User>(row.native_witness.clone())
        else {
            return false;
        };
        if native.id != id {
            return false;
        }
    }
    for batch in &saved.pending {
        if validate_disconnection(
            batch.origin,
            &batch.rows,
            &batch.interactions,
            batch.disconnection.as_ref(),
        )
        .is_err()
            || IncomingInteractions::decode(&batch.interactions).is_err()
            || batch.rows.iter().any(|row| member_id(row).is_err())
            || !matches!(batch.origin, Origin::Directory | Origin::MemberInfo)
                && batch.interactions.as_object().is_none_or(|m| !m.is_empty())
        {
            return false;
        }
    }
    let Ok(scope) = Scope::capture(&saved.identity.account, owner, 0, "cache-validation") else {
        return false;
    };
    let Ok(flags) = Flags::decode(&serde_json::json!({"ok":true,"assignments":{}})) else {
        return false;
    };
    if Plan::capture_direct(
        scope,
        saved.records.iter().map(Record::current).collect(),
        vec![],
        flags,
    )
    .is_err()
    {
        return false;
    }
    if saved
        .disconnected
        .keys()
        .any(|id| member_id(&serde_json::json!({"id":id})).is_err())
    {
        return false;
    }
    saved
        .invalidated
        .iter()
        .all(|id| member_id(&serde_json::json!({"id":id})).is_ok())
}
