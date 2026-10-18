//! Direct eH membership action in the private receipt shadow; no 340b pass.
use super::{ClassificationState, ClassificationStep, Error, classify, id, truth};
use serde_json::{Value, json};
use std::collections::BTreeMap;

impl ClassificationState {
    /// Apply the same owned source action before a later raw-channel step.
    /// The caller must independently publish that action under its original
    /// receipt/owner. This is classification only, not a Commit or fetch permit.
    /// An absent joined ID uses the source transient unknown object, without
    /// claiming server absence or inventing a terminal missing marker.
    pub fn membership_step(
        &mut self,
        left: &[String],
        joined: &[String],
        allow_private: bool,
    ) -> Result<ClassificationStep, Error> {
        if left.len().saturating_add(joined.len()) > 100_000 {
            return Err(Error::Shape);
        }
        let mut wanted = Vec::<(String, bool)>::new();
        let mut positions = BTreeMap::<String, usize>::new();
        for (ids, value) in [(left, false), (joined, true)] {
            for key in ids {
                id(&json!({"id":key}))?;
                if self.unavailable.contains(key) {
                    return Err(Error::MissingObservation);
                }
                if let Some(index) = positions.get(key) {
                    wanted[*index].1 = value;
                } else {
                    positions.insert(key.clone(), wanted.len());
                    wanted.push((key.clone(), value));
                }
            }
        }
        let mut changes = Vec::new();
        for (key, wanted) in &wanted {
            let before = self.indexes.get(key).map(|i| &self.shadow[*i]);
            let fallback = json!({"id":key,"isUnknown":true});
            let row = before.unwrap_or(&fallback);
            let old = truth(&row["is_member"]);
            let private =
                (truth(&row["is_private"]) || truth(&row["is_group"])) && !truth(&row["is_mpim"]);
            let next = *wanted && (!truth(&row["is_archived"]) || (allow_private && private));
            if (!old || next) && old != next {
                let mut changed = row.clone();
                changed
                    .as_object_mut()
                    .ok_or(Error::Shape)?
                    .insert("is_member".into(), Value::Bool(next));
                changes.push((key.clone(), changed));
            }
        }
        if self.shadow.len().saturating_add(
            changes
                .iter()
                .filter(|(id, _)| !self.indexes.contains_key(id))
                .count(),
        ) > 100_000
        {
            return Err(Error::Shape);
        }
        let action_ids = changes.iter().map(|(id, _)| id.clone()).collect();
        // All validation precedes mutation. Preserve exact flags/name/lower
        // token; eH bypasses naming and isUnknown/isNonExistent normalization.
        for (id, row) in changes {
            if let Some(index) = self.indexes.get(&id) {
                self.shadow[*index] = row;
            } else {
                self.indexes.insert(id, self.shadow.len());
                self.shadow.push(row);
            }
        }
        let rows = wanted
            .iter()
            .map(|(key, _)| classify(self.indexes.get(key).map(|i| &self.shadow[*i]), key))
            .collect();
        Ok(ClassificationStep { action_ids, rows })
    }
}

#[cfg(test)]
mod tests;
