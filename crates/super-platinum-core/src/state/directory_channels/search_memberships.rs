//! 6OO5.eY/eV: query membership companions precede normalized returned rows.
//! No conditional-fetch batch or destination/posting permission is minted here.
use super::*;
#[cfg(test)]
mod tests;
fn truth(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(v) => *v,
        Value::Number(v) => v.as_f64().is_some_and(|v| v != 0.0),
        Value::String(v) => !v.is_empty(),
        _ => true,
    }
}
/// Clone the completed search aggregate, retaining original wire pages elsewhere.
/// An empty companion list leaves every returned field untouched (source eV).
pub fn returned(rows: &[Value], membership: &[String]) -> Result<Vec<Value>> {
    if rows.len() > 10_000 || membership.len() > 100_000 {
        return Err(Error::Shape);
    }
    for id in membership {
        projection::id(&serde_json::json!({"id":id}))?;
    }
    let members: BTreeSet<_> = membership.iter().map(String::as_str).collect();
    rows.iter()
        .map(|row| {
            let id = projection::id(row)?;
            let mut next = row.clone();
            if !members.is_empty() {
                let private = (truth(&row["is_private"]) || truth(&row["is_group"]))
                    && !truth(&row["is_mpim"]);
                next["is_member"] =
                    Value::Bool(members.contains(id) && (!truth(&row["is_archived"]) || private));
            }
            Ok(next)
        })
        .collect()
}
impl Store {
    /// Original query observation authorizes only members-minus-returned from
    /// its completed response. Text queries have no resolved-ID left action.
    /// Queue before the same response's raw ingress; install both atomically.
    #[allow(clippy::too_many_arguments)]
    pub fn queue_search_memberships(
        &self,
        observed: &Observation,
        account: &str,
        owner: &WorkspaceSession,
        epoch: u64,
        generation: &str,
        native: &BTreeMap<String, Channel>,
        returned_ids: &[String],
        membership: &[String],
    ) -> Result<Commit> {
        if !observed.current(self, account, owner, epoch, generation, native) {
            return Err(Error::MissingObservation);
        }
        if returned_ids.len() > 10_000 || membership.len() > 100_000 {
            return Err(Error::Shape);
        }
        for id in returned_ids.iter().chain(membership) {
            projection::id(&serde_json::json!({"id":id}))?;
        }
        let returned: BTreeSet<_> = returned_ids.iter().collect();
        let joined: Vec<_> = membership
            .iter()
            .filter(|id| !returned.contains(id))
            .cloned()
            .collect();
        let mut store = self.clone();
        if !joined.is_empty() {
            store.enqueue_batch(Batch {
                origin: Origin::Direct,
                rows: vec![],
                action: Some(actions::Action::Joined(joined)),
                boot_in_flight: None,
                receipt_facts: None,
            })?;
        }
        Ok(Commit {
            store,
            native: native.clone(),
        })
    }
}
