//! Shared direct eH membership reducer; callers supply original source authority.
use super::*;
// Shared exact eH reducer body. Public owners must validate their own opaque
// flight/batch, exact original Observation and response-ID subset first.
pub(in crate::state::directory_channels) fn apply_joined(
    store: &Store,
    native: &BTreeMap<String, Channel>,
    joined: &[String],
) -> Result<Commit> {
    let mut store = store.clone();
    let mut native = native.clone();
    let saved = store.saved.as_mut().ok_or(Error::MissingObservation)?;
    let mut changed = false;
    let mut name_updates = Vec::new();
    let mut seen = BTreeSet::new();
    for id in joined {
        if !seen.insert(id) {
            continue;
        }
        if saved.invalidated.contains(id)
            || saved
                .pending
                .iter()
                .any(|batch| batch.rows.iter().any(|row| row["id"] == *id))
        {
            return Err(Error::MissingObservation);
        }
        if let Some(record) = saved.records.iter_mut().find(|r| r.source["id"] == *id) {
            let current = native.get_mut(id).ok_or(Error::MissingObservation)?;
            let value = serde_json::to_value(&*current).map_err(|_| Error::Shape)?;
            if !source_native_equal(&record.native, &value) {
                return Err(Error::MissingObservation);
            }
            let row = &record.source;
            let private =
                (truth(&row["is_private"]) || truth(&row["is_group"])) && !truth(&row["is_mpim"]);
            let next = !truth(&row["is_archived"]) || private;
            let old = truth(&row["is_member"]);
            if (!old || next) && old != next {
                record.source["is_member"] = next.into();
                current.extra.insert("is_member".into(), Value::Bool(next));
                record.native = serde_json::to_value(&*current).map_err(|_| Error::Shape)?;
                name_updates.push(record.source.clone());
                changed = true;
            }
        } else {
            if native.contains_key(id) {
                return Err(Error::MissingObservation);
            }
            // EmXR returns this ephemeral object; eH's direct reducer
            // action then inserts it with membership. It bypasses 340b.
            let source = serde_json::json!({"id":id,"isUnknown":true,"is_member":true});
            let channel: Channel =
                serde_json::from_value(source.clone()).map_err(|_| Error::Shape)?;
            let witness = serde_json::to_value(&channel).map_err(|_| Error::Shape)?;
            native.insert(id.clone(), channel);
            name_updates.push(source.clone());
            saved.records.push(Record {
                source,
                wire: Value::Null,
                native: witness,
                runtime_missing: false,
                runtime_unknown: true,
            });
            changed = true;
        }
    }
    if changed {
        if let Some(index) = &mut store.names {
            // eH's direct f0 action has a nonempty BULK_UPDATE_CHANNELS
            // reason. Replay only its actual changed objects; names did
            // not change, so existing exact lowercase observations suffice.
            Arc::make_mut(index).update(&name_updates, &BTreeMap::new())?;
        }
        store.changed();
    }
    Ok(Commit { store, native })
}
fn truth(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(v) => *v,
        Value::Number(v) => v.as_f64().is_some_and(|n| n != 0.0),
        Value::String(v) => !v.is_empty(),
        _ => true,
    }
}
