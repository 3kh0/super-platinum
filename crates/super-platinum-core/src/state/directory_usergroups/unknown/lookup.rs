use super::*;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LookupKind {
    Known,
    Unknown,
    NonExistent,
}

/// Private construction binds canonical effective state to an exact shared
/// snapshot. The raw source remains untouched; it is not a label preimage.
#[derive(Clone)]
pub struct Lookup {
    scope: Scope,
    id: String,
    kind: LookupKind,
    effective: Arc<Value>,
    source_hit: bool,
    alive: Arc<AtomicU8>,
    token: Option<Arc<AtomicU8>>,
    snapshot: Snapshot,
}
impl Lookup {
    /// A read-only source hit needs no missing manager. Observed absence is an
    /// error here and grants no placeholder or scheduling capability.
    pub fn observed(scope: &Scope, store: &Store, native: &Native, id: &str) -> Result<Self> {
        if !valid_id(id) {
            return Err(Error::Shape);
        }
        let snapshot = scope.snapshot(store, native)?;
        let row = snapshot
            .rows()
            .iter()
            .find(|row| row.effective["id"].as_str() == Some(id))
            .ok_or(Error::Unknown)?;
        let effective = Arc::new(row.effective.clone());
        let kind = classify(&effective);
        Ok(Self {
            scope: scope.clone(),
            id: id.into(),
            kind,
            effective,
            source_hit: true,
            alive: Arc::new(AtomicU8::new(1)),
            token: None,
            snapshot,
        })
    }
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn kind(&self) -> LookupKind {
        self.kind
    }
    pub fn effective(&self) -> &Value {
        &self.effective
    }
    pub fn is_source_hit(&self) -> bool {
        self.source_hit
    }
    pub fn is_transient(&self) -> bool {
        !self.source_hit
    }
    pub fn current(&self, scope: &Scope, store: &Store, native: &Native) -> bool {
        self.scope.matches(scope)
            && self.alive.load(Ordering::Acquire) == 1
            && self
                .token
                .as_ref()
                .is_none_or(|t| t.load(Ordering::Acquire) == 1)
            && self
                .snapshot
                .current(store, &scope.account, &scope.owner, scope.epoch, native)
    }
}
impl Manager {
    pub fn lookup(
        &mut self,
        scope: &Scope,
        store: &Store,
        native: &Native,
        id: &str,
    ) -> Result<Lookup> {
        self.lookup_if(scope, store, native, id, true)?
            .ok_or(Error::Unknown)
    }
    pub fn lookup_if(
        &mut self,
        scope: &Scope,
        store: &Store,
        native: &Native,
        id: &str,
        allow_fetch: bool,
    ) -> Result<Option<Lookup>> {
        if !self.current(scope) {
            return Err(Error::Owner);
        }
        if !valid_id(id) {
            return Err(Error::Shape);
        }
        let snapshot = scope.snapshot(store, native)?;
        if !self.same_source(&snapshot.observed) {
            return Err(Error::Stale);
        }
        let existing = snapshot
            .rows()
            .iter()
            .find(|row| row.effective["id"].as_str() == Some(id));
        let (effective, source_hit, token) = if let Some(row) = existing {
            (Arc::new(row.effective.clone()), true, None)
        } else {
            // Self-membership may create a native id-only row without metadata.
            // A source-bound absence is still a selector miss in that case.
            if !allow_fetch {
                return Ok(None);
            }
            if !self.pending.iter().any(|key| key == id) {
                if self.pending.len() >= MAX_ROWS {
                    return Err(Error::Limit);
                }
                self.pending.push(id.into());
            }
            let (token, value) = self.lookups.entry(id.into()).or_insert_with(|| {
                (
                    Arc::new(AtomicU8::new(1)),
                    Arc::new(serde_json::json!({"id":id,"name":id,"handle":id,"isUnknown":true})),
                )
            });
            (value.clone(), false, Some(token.clone()))
        };
        let kind = classify(&effective);
        self.source = Some(snapshot.observed.clone());
        Ok(Some(Lookup {
            scope: scope.clone(),
            id: id.into(),
            kind,
            effective,
            source_hit,
            alive: self.alive.clone(),
            token,
            snapshot,
        }))
    }
}
fn classify(effective: &Value) -> LookupKind {
    if projection::truth(&effective["isNonExistent"]) {
        LookupKind::NonExistent
    } else if projection::truth(&effective["isUnknown"]) {
        LookupKind::Unknown
    } else {
        LookupKind::Known
    }
}
