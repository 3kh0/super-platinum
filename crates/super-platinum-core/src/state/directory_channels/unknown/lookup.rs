//! A selector observation is not a canonical row constructor. In particular,
//! raw marker-shaped HTTP/cache data cannot grant runtime placeholder custody.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LookupKind {
    Known,
    Unknown,
    NonExistent,
    Unobserved,
}

#[derive(Clone)]
pub struct Lookup {
    id: String,
    kind: LookupKind,
    row: Option<Value>,
    manager: Arc<AtomicU8>,
    pending: Option<Arc<AtomicU8>>,
    observed: Arc<Observation>,
    phase: lifecycle::CapturedPhase,
}
impl Lookup {
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn kind(&self) -> LookupKind {
        self.kind
    }
    pub fn row(&self) -> Option<&Value> {
        self.row.as_ref()
    }
    /// Only absent selector misses register UnknownFetchManager work. A proved
    /// existing runtime_unknown row is a source hit for conditional freshness.
    pub fn is_transient(&self) -> bool {
        self.pending.is_some()
    }
    pub fn current(
        &self,
        manager: &Manager,
        scope: &Scope,
        store: &Store,
        generation: &str,
        native: &BTreeMap<String, Channel>,
    ) -> bool {
        manager.current(scope)
            && Arc::ptr_eq(&self.manager, &manager.alive)
            && self.manager.load(Ordering::Acquire) == 1
            && store
                .lifecycle
                .accepts(&self.phase, &scope.account, &scope.owner, scope.epoch)
            && self.observed.current(
                store,
                &scope.account,
                &scope.owner,
                scope.epoch,
                generation,
                native,
            )
            && self.pending.as_ref().is_none_or(|token| {
                token.load(Ordering::Acquire) == 1
                    && manager
                        .lookups
                        .get(&self.id)
                        .is_some_and(|current| Arc::ptr_eq(current, token))
            })
    }
}
impl Manager {
    #[allow(clippy::too_many_arguments)]
    pub fn lookup(
        &mut self,
        scope: &Scope,
        store: &Store,
        observed: Arc<Observation>,
        generation: &str,
        native: &BTreeMap<String, Channel>,
        id: &str,
        allow_fetch: bool,
    ) -> Result<Lookup> {
        if !self.current(scope)
            || !observed.current(
                store,
                &scope.account,
                &scope.owner,
                scope.epoch,
                generation,
                native,
            )
        {
            return Err(Error::MissingObservation);
        }
        if self.phase.as_ref().is_some_and(|phase| {
            !store
                .lifecycle
                .accepts(phase, &scope.account, &scope.owner, scope.epoch)
        }) {
            return Err(Error::MissingObservation);
        }
        let phase = store
            .lifecycle
            .capture(&scope.account, &scope.owner, scope.epoch)?;
        self.phase = Some(phase.clone());
        let mut result = Lookup {
            id: id.into(),
            kind: LookupKind::Unobserved,
            row: None,
            manager: self.alive.clone(),
            pending: None,
            observed,
            phase,
        };
        // Retained kdyP selector boundary. Z has no Store projection namespace.
        if id.len() < 9
            || !matches!(id.as_bytes()[0], b'C' | b'G' | b'D' | b'Z')
            || !id
                .bytes()
                .skip(1)
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
        {
            return Ok(result);
        }
        if id.starts_with('Z') {
            return Err(Error::MissingObservation);
        }
        projection::id(&serde_json::json!({"id":id}))?;
        let saved = store.identity(&scope.account, &scope.owner)?;
        if saved.invalidated.contains(id)
            || saved.pending.iter().any(|batch| {
                batch.rows.iter().any(|row| row["id"].as_str() == Some(id))
                    || batch
                        .action
                        .as_ref()
                        .is_some_and(|action| action.ids().iter().any(|key| key == id))
            })
        {
            return Err(Error::MissingObservation);
        }
        if let Some(record) = saved
            .records
            .iter()
            .find(|record| record.source["id"] == id)
        {
            let current = native.get(id).ok_or(Error::MissingObservation)?;
            let value = serde_json::to_value(current).map_err(|_| Error::Shape)?;
            if !source_native_equal(&record.native, &value) {
                return Err(Error::MissingObservation);
            }
            result.kind = if record.source["isNonExistent"] == true && record.runtime_missing {
                LookupKind::NonExistent
            } else if record.source["isUnknown"] == true && record.runtime_unknown {
                LookupKind::Unknown
            } else if marker_present(&record.source["isNonExistent"])
                || marker_present(&record.source["isUnknown"])
            {
                return Err(Error::MissingObservation);
            } else {
                result.row = Some(record.source.clone());
                LookupKind::Known
            };
            return Ok(result);
        }
        if native.contains_key(id) {
            return Err(Error::MissingObservation);
        }
        if !allow_fetch {
            return Ok(result);
        }
        if !self.pending.iter().any(|key| key == id) {
            self.pending.push(id.into());
        }
        let token = self
            .lookups
            .entry(id.into())
            .or_insert_with(|| Arc::new(AtomicU8::new(1)))
            .clone();
        result.kind = LookupKind::Unknown;
        result.pending = Some(token);
        Ok(result)
    }
}
fn marker_present(value: &Value) -> bool {
    !matches!(value, Value::Null | Value::Bool(false))
}
