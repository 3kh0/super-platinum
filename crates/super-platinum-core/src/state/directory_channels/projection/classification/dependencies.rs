//! IDs needed by the actual pre-name row. This never publishes projected rows.
use super::super::{merged_before_names, strings, truth};
use super::{ClassificationState, Error, id};
use serde_json::Value;
use std::collections::BTreeSet;

impl ClassificationState {
    /// Each row sees the same pre-dispatch shadow, including duplicate IDs.
    /// Capture real synchronous member observations for this set before step.
    /// The set is not an absence claim, a recovery dispatch, or a Ready witness.
    pub fn required_member_ids(
        &self,
        incoming: &[Value],
        self_id: &str,
    ) -> Result<BTreeSet<String>, Error> {
        if incoming.len() > 100_000 || self_id.is_empty() {
            return Err(Error::Shape);
        }
        let mut required = BTreeSet::new();
        for wire in incoming {
            let key = id(wire)?;
            if self.unavailable.contains(key) {
                return Err(Error::MissingObservation);
            }
            let before = self.indexes.get(key).map(|index| &self.shadow[*index]);
            let row = merged_before_names(before, wire)?;
            if truth(&row["is_mpim"]) {
                if !truth(&row["properties"]["has_custom_mpdm_name"]) {
                    // Actual 7tpZ calls PdIx for every nonself member before
                    // testing the count, even when fewer than two remain.
                    for member in strings(row.get("members"))? {
                        if member != self_id {
                            required.insert(member);
                        }
                    }
                }
            } else if truth(&row["is_im"]) {
                let member = row["user"]
                    .as_str()
                    .filter(|id| !id.is_empty())
                    .ok_or(Error::MissingObservation)?;
                required.insert(member.to_owned());
            }
        }
        Ok(required)
    }
}

#[cfg(test)]
mod tests;
