use super::*;

/// A read-only boot preimage; dropping it has no effect on the store.
pub struct BootLease {
    identity: Arc<()>,
    revision: u64,
    tail: u64,
    native: NativeProof,
}

impl Store {
    pub fn begin_boot(&self, native: &Native) -> Result<BootLease> {
        if self.is_legacy() || self.has_pending() || self.gap.load(Ordering::Acquire) {
            return Err(Error::Unknown);
        }
        let native = native_proof(native)?;
        if self.fresh && !native.is_empty()
            || self.saved.as_ref().is_some_and(|s| s.native != native)
        {
            return Err(Error::Stale);
        }
        Ok(BootLease {
            identity: self.identity.clone(),
            revision: self.revision,
            tail: self.tail,
            native,
        })
    }

    /// Called only for an actually accepted full userBoot response. This does not
    /// turn legacy cached native rows into a complete source collection.
    pub fn observe_boot(
        &self,
        lease: BootLease,
        account: &str,
        owner: &WorkspaceSession,
        epoch: u64,
        native: &Native,
        raw: &Value,
    ) -> Result<Commit> {
        if account.is_empty()
            || raw.get("ok") != Some(&Value::Bool(true))
            || raw.get("error").is_some_and(projection::truth)
            || raw.pointer("/team/id").and_then(Value::as_str) != Some(owner.team_id.as_str())
            || raw.pointer("/self/id").and_then(Value::as_str) != Some(owner.user_id.as_str())
        {
            return Err(Error::Owner);
        }
        if !Arc::ptr_eq(&lease.identity, &self.identity)
            || lease.revision != self.revision
            || lease.tail != self.tail
            || self.has_pending()
            || self.gap.load(Ordering::Acquire)
            || native_proof(native)? != lease.native
        {
            return Err(Error::Stale);
        }
        let mut store = self.clone();
        let mut saved = match &self.saved {
            Some(s)
                if s.account == account
                    && s.team == owner.team_id
                    && s.user == owner.user_id
                    && s.native == lease.native =>
            {
                s.clone()
            }
            Some(_) => return Err(Error::Owner),
            None if self.fresh && native.is_empty() => Saved {
                version: 1,
                account: account.into(),
                team: owner.team_id.clone(),
                user: owner.user_id.clone(),
                rows: vec![],
                membership: None,
                membership_flags: BTreeMap::new(),
                native: lease.native,
                pending: vec![],
            },
            None => return Err(Error::Unknown),
        };
        let mut native = native.clone();
        if let Some(value) = raw.pointer("/subteams/self") {
            if !value.is_null() {
                let ids = projection::strings(value)?;
                projection::apply(
                    &mut saved,
                    &mut native,
                    &Operation::Membership { ids },
                    &BTreeMap::new(),
                )?;
            }
        }
        saved.native = native_proof(&native)?;
        store.saved = Some(saved);
        store.fresh = false;
        store.binding = Some(Arc::new(Binding {
            account: account.into(),
            owner: owner.clone(),
            epoch,
        }));
        store.revision = store.revision.checked_add(1).ok_or(Error::Limit)?;
        store.check_size()?;
        Ok(Commit {
            store,
            native,
            results: vec![],
            completed: None,
        })
    }

    /// No auth material is serialized. A cancelled source history cannot be
    /// persisted as a complete history. Pending accepted operations remain raw.
    pub fn save(&self) -> Value {
        if self.gap.load(Ordering::Acquire) {
            return Value::Null;
        }
        let Some(mut saved) = self.saved.clone() else {
            return Value::Null;
        };
        saved.pending = self
            .queue
            .iter()
            .map(|q| (q.generation.clone(), q.operation.clone()))
            .collect();
        serde_json::to_value(saved)
            .ok()
            .filter(|v| serde_json::to_vec(v).is_ok_and(|b| b.len() <= MAX_BYTES))
            .unwrap_or(Value::Null)
    }

    /// Restored history has no live ownership. Only a subsequent full boot can
    /// bind it. Pending operations lack their original async owner after restart
    /// and therefore conservatively leave history unavailable.
    pub fn restore(value: Option<&Value>, native: &Native) -> Self {
        fn checked(value: &Value, native: &Native) -> Result<Store> {
            if serde_json::to_vec(value).map_err(|_| Error::Shape)?.len() > MAX_BYTES {
                return Err(Error::Limit);
            }
            let saved: Saved = serde_json::from_value(value.clone()).map_err(|_| Error::Shape)?;
            if saved.version != 1
                || saved.account.is_empty()
                || saved.team.is_empty()
                || saved.user.is_empty()
                || saved.native != native_proof(native)?
                || !saved.pending.is_empty()
            {
                return Err(Error::Shape);
            }
            let mut ids = std::collections::HashSet::new();
            for row in &saved.rows {
                let id = projection::id(&row.source)?;
                if projection::id(&row.effective)? != id
                    || !ids.insert(id)
                    || !saved.native.contains_key(id)
                {
                    return Err(Error::Shape);
                }
            }
            if let Some(ids) = &saved.membership {
                projection::validate(&Operation::Membership { ids: ids.clone() })?;
            }
            let result = Store {
                saved: Some(saved),
                ..Store::default()
            };
            result.check_size()?;
            Ok(result)
        }
        value
            .and_then(|value| checked(value, native).ok())
            .unwrap_or_default()
    }
}
