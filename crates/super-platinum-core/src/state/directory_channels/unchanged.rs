//! HP6x.Pw is a real source action, distinct from a suppressed 4XO+ upsert.
use super::*;

fn truth(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::Number(value) => value.as_f64().is_some_and(|value| value != 0.0),
        Value::String(value) => !value.is_empty(),
        _ => true,
    }
}

pub struct Unchanged {
    commit: Commit,
    missing: Vec<String>,
}
impl Unchanged {
    /// Missing IDs are selector requests, never verified server absence.
    pub fn into_parts(self) -> (Commit, Vec<String>) {
        (self.commit, self.missing)
    }
}
impl Store {
    /// The caller owns the original response/owner fence. Only resolved IDs
    /// without returned rows reach HP6x's explicit unchanged action. It may
    /// refresh the lazy name map even when canonical records are identical.
    #[allow(clippy::too_many_arguments)]
    pub fn process_unchanged_channels(
        &self,
        observed: &Observation,
        account: &str,
        owner: &WorkspaceSession,
        epoch: u64,
        generation: &str,
        native: &BTreeMap<String, Channel>,
        resolved: &[String],
        returned: &[String],
    ) -> Result<Unchanged> {
        if !observed.current(self, account, owner, epoch, generation, native) {
            return Err(Error::MissingObservation);
        }
        let saved = self.identity(account, owner)?;
        let mut rows = Vec::new();
        let mut missing = Vec::new();
        for id in resolved.iter().chain(returned) {
            projection::id(&serde_json::json!({"id":id}))?;
        }
        for id in resolved.iter().filter(|id| !returned.contains(id)) {
            // A pending or invalidated row cannot authenticate the old source
            // object that the original selector would have read.
            if saved.invalidated.contains(id)
                || saved.pending.iter().any(|batch| {
                    batch
                        .rows
                        .iter()
                        .any(|row| row["id"].as_str() == Some(id.as_str()))
                })
            {
                return Err(Error::MissingObservation);
            }
            let Some(record) = saved.records.iter().find(|r| r.source["id"] == *id) else {
                missing.push(id.clone());
                continue;
            };
            let current = native.get(id).ok_or(Error::MissingObservation)?;
            let value = serde_json::to_value(current).map_err(|_| Error::Shape)?;
            if !source_native_equal(&record.native, &value) {
                return Err(Error::MissingObservation);
            }
            // zdO.Zx excludes both runtime placeholders. A missing selector
            // schedules recovery; an existing placeholder does not prove a
            // missing HTTP resource and is not supplied to the index.
            if truth(&record.source["isUnknown"]) || truth(&record.source["isNonExistent"]) {
                continue;
            }
            rows.push(record.source.clone());
        }
        let mut store = self.clone();
        if let Some(index) = &mut store.names {
            let has_names = rows.iter().try_fold(false, |found, row| {
                Ok::<_, Error>(found || names::raw_name(row)?.is_some())
            })?;
            if has_names {
                Arc::make_mut(index).update(&rows, &BTreeMap::new())?;
                store.changed();
            }
        }
        Ok(Unchanged {
            commit: Commit {
                store,
                native: native.clone(),
            },
            missing,
        })
    }
}
