//! Receipt-time quotient of the existing 340b/4XO+ processor. This private
//! shadow is never a channel store, native projection, name index or Ready.
use super::{Context, Error, batch, id, truth};
use crate::state::directory_channels::{Facts, Observation, Store};
use crate::{config::WorkspaceSession, slack::models::Channel};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClassificationKind {
    Absent,
    Existing,
    NonExistent,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClassifiedRow {
    pub id: String,
    pub kind: ClassificationKind,
    pub name_truthy: bool,
}

pub struct ClassificationStep {
    // Preserve actual changed-action order and duplicates. This is not the
    // complete list of processor returns, nor permission to dispatch anything.
    action_ids: Vec<String>,
    rows: Vec<ClassifiedRow>,
}
impl ClassificationStep {
    pub fn action_ids(&self) -> &[String] {
        &self.action_ids
    }
    pub fn rows(&self) -> &[ClassifiedRow] {
        &self.rows
    }
}

#[derive(Clone)]
pub struct ClassificationState {
    // No Serialize, raw getter, public raw constructor or canonical conversion.
    shadow: Vec<Value>,
    indexes: BTreeMap<String, usize>,
    unavailable: BTreeSet<String>,
}
impl Observation {
    /// Mint only from a still-owned canonical preimage. This does not consume
    /// pending ingress or certify a complete collection. The caller must keep
    /// its original owner/accepted-ingress chain current after every own page.
    pub fn classification(
        &self,
        store: &Store,
        account: &str,
        owner: &WorkspaceSession,
        epoch: u64,
        generation: &str,
        native: &BTreeMap<String, Channel>,
    ) -> Result<ClassificationState, Error> {
        if !self.current(store, account, owner, epoch, generation, native) {
            return Err(Error::MissingObservation);
        }
        let mut state = ClassificationState::capture(self.rows())?;
        state.unavailable = store.required_observations(native);
        for batch in &store.identity(account, owner)?.pending {
            if let Some(action) = &batch.action {
                state.unavailable.extend(action.ids().iter().cloned());
            }
            for row in &batch.rows {
                state.unavailable.insert(id(row)?.to_owned());
            }
        }
        Ok(state)
    }
}
impl ClassificationState {
    fn capture(before: &[Value]) -> Result<Self, Error> {
        if before.len() > 100_000 {
            return Err(Error::Shape);
        }
        let mut indexes = BTreeMap::new();
        let mut shadow = before.to_vec();
        for (index, row) in shadow.iter_mut().enumerate() {
            if indexes.insert(id(row)?.to_owned(), index).is_some() {
                return Err(Error::Shape);
            }
            let raw = match row.get("name") {
                None | Some(Value::Null) => "",
                Some(Value::String(name)) => name,
                _ => return Err(Error::Shape),
            };
            let lower_token = match row.get("_name_lc") {
                None => None,
                Some(Value::String(lower)) if lower.is_empty() => Some(String::new()),
                Some(Value::String(_)) if !raw.is_empty() => Some(raw.to_owned()),
                _ => return Err(Error::Shape),
            };
            let object = row.as_object_mut().ok_or(Error::Shape)?;
            if let Some(lower) = object.get_mut("_name_lc") {
                // Lowercase happens BEFORE isUnknown is cleared. A real row
                // can therefore have a nonempty name but an empty derived slot.
                // Preserve that distinction; all other admitted lower inputs
                // equal the resulting row's raw name. Never expose this token.
                *lower = Value::String(lower_token.unwrap());
            }
        }
        Ok(Self {
            shadow,
            indexes,
            unavailable: BTreeSet::new(),
        })
    }

    /// All facts are synchronous observations at this receipt. Absent members
    /// require the actual source placeholder witness; missing map entries refuse.
    /// Do not await member recovery or renderer lowercase before calling this.
    /// The parent separately owns those effects and later canonical admission.
    pub fn step(
        &mut self,
        incoming: &[Value],
        facts: &Facts,
        boot_in_flight: bool,
    ) -> Result<ClassificationStep, Error> {
        for row in incoming {
            if self.unavailable.contains(id(row)?) {
                return Err(Error::MissingObservation);
            }
        }
        let opaque_lower = |raw: &str| Some(raw.to_owned());
        let context = Context {
            self_id: &facts.self_id,
            current_team: &facts.current_team,
            omit_team: &facts.omit_team,
            boot_in_flight,
            retain_public_membership_on_archive: facts.retain_public_membership_on_archive,
            members: &facts.members,
            lowercase: &opaque_lower,
        };
        // Use the existing full processor and duplicate/action comparator. All
        // inputs in this page see one pre-dispatch shadow, exactly like 4XO+.
        let processed = batch(&self.shadow, incoming, &context)?;
        let action_ids = processed
            .actions()
            .iter()
            .map(|row| id(row).map(str::to_owned))
            .collect::<Result<Vec<_>, _>>()?;
        let indexes = processed
            .rows()
            .iter()
            .enumerate()
            .map(|(index, row)| id(row).map(|key| (key.to_owned(), index)))
            .collect::<Result<BTreeMap<_, _>, _>>()?;
        let mut seen = BTreeSet::new();
        let mut rows = Vec::new();
        for wire in incoming {
            let key = id(wire)?;
            if seen.insert(key) {
                rows.push(classify(
                    indexes.get(key).map(|i| &processed.rows()[*i]),
                    key,
                ));
            }
        }
        // A refused page never changes the retained shadow.
        self.shadow = processed.rows().to_vec();
        self.indexes = indexes;
        Ok(ClassificationStep { action_ids, rows })
    }

    /// Absence here means absent from this post-action shadow, not server 404.
    /// This may be read by the owned completion/timeout-finally boundary even
    /// while canonical member/renderer work for accepted rows remains pending.
    pub fn row(&self, key: &str) -> Result<ClassifiedRow, Error> {
        id(&serde_json::json!({"id": key}))?;
        if self.unavailable.contains(key) {
            return Err(Error::MissingObservation);
        }
        Ok(classify(
            self.indexes.get(key).map(|i| &self.shadow[*i]),
            key,
        ))
    }
    /// EmXR re-reads queued IDs at dispatch and retains only present rows whose
    /// isUnknown is false. Existing classification alone intentionally folds
    /// known and runtime-unknown rows together; neither presence nor a joined
    /// membership action proves that the wrapper returned an object.
    pub fn is_unknown(&self, key: &str) -> Result<bool, Error> {
        self.row(key)?;
        Ok(self
            .indexes
            .get(key)
            .is_some_and(|index| truth(&self.shadow[*index]["isUnknown"])))
    }
    /// Initial predecessor gaps, not server absence or a request to wait on
    /// renderer IO. An owned earlier receipt shadow/replay is a separate proof.
    pub fn unavailable_ids(&self) -> &BTreeSet<String> {
        &self.unavailable
    }
    /// The existing conditional request's admitted `updated || 0` value from
    /// this synchronous post-action predecessor. This exposes no channel row,
    /// renderer result or permission to dispatch a request.
    pub fn conditional_version(&self, key: &str) -> Result<u64, Error> {
        self.row(key)?;
        match self
            .indexes
            .get(key)
            .and_then(|index| self.shadow[*index].get("updated"))
        {
            None | Some(Value::Null) | Some(Value::Bool(false)) => Ok(0),
            Some(Value::Number(n)) => n.as_u64().ok_or(Error::Shape),
            Some(Value::String(s)) if s.is_empty() => Ok(0),
            _ => Err(Error::Shape),
        }
    }
}
fn classify(row: Option<&Value>, key: &str) -> ClassifiedRow {
    ClassifiedRow {
        id: key.into(),
        kind: match row {
            None => ClassificationKind::Absent,
            Some(row) if truth(&row["isNonExistent"]) => ClassificationKind::NonExistent,
            Some(_) => ClassificationKind::Existing,
        },
        name_truthy: row.is_some_and(|row| truth(&row["name"])),
    }
}

#[cfg(test)]
mod tests;

mod dependencies;

mod memberships;

#[cfg(test)]
mod unknown_tests;
