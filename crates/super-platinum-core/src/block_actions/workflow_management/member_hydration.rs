//! Shared directory hydration observations. No store, renderer or network owner.
//! Source name/profile projection and native partial preservation are distinct.
use crate::{
    block_actions::home::workflow::directory_local::derive_member_names, config::WorkspaceSession,
    slack::Error,
};
use serde::Deserialize;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
mod profile;
#[cfg(test)]
mod tests;

fn invalid() -> Error {
    Error::Api("directory_member_hydration_unavailable".into())
}
fn key(raw: &Value) -> Result<&str, Error> {
    raw.as_object().ok_or_else(invalid)?;
    let id = raw["id"].as_str().ok_or_else(invalid)?;
    if id.len() > 255
        || !id
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
        || !(matches!(
            id,
            "USLACKBOT" | "USLACK" | "USEARCH" | "UCHANNELEXPERT" | "USLACKSECURITY"
        ) || id.len() >= 9 && matches!(id.as_bytes()[0], b'U' | b'W'))
    {
        return Err(invalid());
    }
    Ok(id)
}
#[derive(Clone, PartialEq)]
pub struct Flags {
    raw: Value,
    normalize_external: bool,
    preserve_self_email: bool,
}
impl Flags {
    /// Named retained observation only; callers must retain its original native owner.
    pub fn channel_threads_enabled(&self) -> bool {
        crate::channel_threads::enabled(&self.raw)
    }

    /// hZ9m.GA and XbX7 read these exact assignments. Missing assignment in a
    /// complete successful response is off; an absent response is unavailable.
    pub fn decode(raw: &Value) -> Result<Self, Error> {
        if raw["ok"] != true || raw.get("error").is_some_and(|v| !v.is_null()) {
            return Err(invalid());
        }
        let assignments = raw["assignments"].as_object().ok_or_else(invalid)?;
        let on = |name| match assignments.get(name) {
            None => Ok(false),
            Some(v) => v["group"].as_str().map(|s| s == "on").ok_or_else(invalid),
        };
        Ok(Self {
            raw: raw.clone(),
            normalize_external: on("search_normalize_visible_external_users")?,
            preserve_self_email: on("preserve_omitted_user_change_member_fields")?,
        })
    }
}
#[derive(Clone)]
pub struct Scope {
    account: String,
    owner: WorkspaceSession,
    epoch: u64,
    generation: String,
}
impl Scope {
    pub fn capture(
        account: &str,
        owner: &WorkspaceSession,
        epoch: u64,
        generation: &str,
    ) -> Result<Self, Error> {
        if account.is_empty()
            || generation.is_empty()
            || owner.team_id.is_empty()
            || owner.user_id.is_empty()
        {
            return Err(invalid());
        }
        Ok(Self {
            account: account.into(),
            owner: owner.clone(),
            epoch,
            generation: generation.into(),
        })
    }
    pub fn current(
        &self,
        account: &str,
        owner: &WorkspaceSession,
        epoch: u64,
        generation: &str,
    ) -> bool {
        self.account == account
            && self.epoch == epoch
            && self.generation == generation
            && self.owner.team_id == owner.team_id
            && self.owner.enterprise_id == owner.enterprise_id
            && self.owner.user_id == owner.user_id
            && self.owner.name == owner.name
            && self.owner.url == owner.url
            && self.owner.token == owner.token
    }
}
#[derive(Clone, PartialEq)]
pub struct Current {
    pub native: Value,
    /// Prior actual source-effective/name observation, not serialized User.
    pub source: Value,
    /// Actual separate th9D interaction record. None means no record.
    pub interaction: Option<bool>,
}
#[derive(Clone)]
pub struct Observation {
    pub before: Option<Current>,
    pub wire: Value,
    pub normalized_wire: Value,
    /// Native adaptation: preserve absent fields, including absent profile keys.
    /// Never use this value as the source matching/name projection.
    pub native_preserved: Value,
    pub source_projection: Value,
    pub skipped_same_version: bool,
}
/// Actual ScqY incoming interaction map, captured separately from prior state.
/// An explicitly observed empty object differs from inventing absent evidence.
#[derive(Clone, PartialEq)]
pub struct IncomingInteractions {
    values: BTreeMap<String, bool>,
}
impl IncomingInteractions {
    pub fn decode(raw: &Value) -> Result<Self, Error> {
        let object = raw.as_object().ok_or_else(invalid)?;
        let mut values = BTreeMap::new();
        for (id, value) in object {
            key(&serde_json::json!({"id":id}))?;
            values.insert(id.clone(), value.as_bool().ok_or_else(invalid)?);
        }
        Ok(Self { values })
    }
}
pub struct Plan {
    scope: Scope,
    flags: Flags,
    before: Vec<Current>,
    observations: Vec<Observation>,
    incoming_interactions: IncomingInteractions,
    fields: Vec<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RendererReply {
    pub generation: String,
    pub operation: String,
    pub fields: Vec<String>,
    pub lowered: Vec<String>,
}
pub struct Applied {
    rows: Vec<Current>,
    observations: Vec<Observation>,
    returned: Vec<Value>,
}
impl Applied {
    pub fn rows(&self) -> &[Current] {
        &self.rows
    }
    pub fn observations(&self) -> &[Observation] {
        &self.observations
    }
    /// ScqY returns one lookup per wire ID, after the whole batch. Duplicate
    /// wire IDs remain duplicated and observe the final stored projection.
    pub fn returned(&self) -> &[Value] {
        &self.returned
    }
    /// Native preservation adaptation against the actual typed-cache snapshot.
    /// Source projection stays independent; these values never provide names
    /// to source matching. Output follows first changed input occurrence.
    pub fn native_updates(&self, before: &BTreeMap<String, Value>) -> Result<Vec<Value>, Error> {
        let mut updates = BTreeMap::new();
        let mut order = Vec::new();
        for observation in &self.observations {
            if observation.skipped_same_version {
                continue;
            }
            let id = key(&observation.wire)?;
            let previous = updates.get(id).or_else(|| before.get(id));
            if previous.is_some_and(|raw| key(raw).ok() != Some(id)) {
                return Err(invalid());
            }
            let next = profile::preserve_native(
                previous,
                &observation.normalized_wire,
                &observation.source_projection,
            )?;
            if !updates.contains_key(id) {
                order.push(id.to_owned());
            }
            updates.insert(id.to_owned(), next);
        }
        order
            .into_iter()
            .map(|id| updates.remove(&id).ok_or_else(invalid))
            .collect()
    }
}
impl Plan {
    pub fn capture(
        scope: Scope,
        before: Vec<Current>,
        incoming: Vec<Value>,
        incoming_interactions: IncomingInteractions,
        flags: Flags,
    ) -> Result<Self, Error> {
        Self::capture_mode(
            scope,
            before,
            incoming,
            incoming_interactions,
            flags,
            true,
            true,
        )
    }
    /// Direct XbX7 upsert: no ScqY version partition or directory-only An8m
    /// normalization. Used only when the actual ingress dispatches XbX7.Ay.
    pub fn capture_direct(
        scope: Scope,
        before: Vec<Current>,
        incoming: Vec<Value>,
        flags: Flags,
    ) -> Result<Self, Error> {
        Self::capture_mode(
            scope,
            before,
            incoming,
            IncomingInteractions {
                values: BTreeMap::new(),
            },
            flags,
            false,
            false,
        )
    }
    /// Actual nzRy -> xqiG interaction projection -> ScqY ingress. Incoming
    /// rows must be the retained project_interaction output; no search An8m.
    pub fn capture_member_info(
        scope: Scope,
        before: Vec<Current>,
        incoming: Vec<Value>,
        incoming_interactions: IncomingInteractions,
        flags: Flags,
    ) -> Result<Self, Error> {
        Self::capture_mode(
            scope,
            before,
            incoming,
            incoming_interactions,
            flags,
            true,
            false,
        )
    }
    fn capture_mode(
        scope: Scope,
        before: Vec<Current>,
        incoming: Vec<Value>,
        incoming_interactions: IncomingInteractions,
        flags: Flags,
        directory: bool,
        normalize_search: bool,
    ) -> Result<Self, Error> {
        let mut indices = BTreeMap::new();
        for (i, row) in before.iter().enumerate() {
            let id = key(&row.source)?;
            profile::validate(&row.source)?;
            if key(&row.native)? != id || indices.insert(id.to_owned(), i).is_some() {
                return Err(invalid());
            }
            for name in [
                "_real_name_normalized_lc",
                "_display_name_normalized_lc",
                "_email_normalized_lc",
            ] {
                if !row.source[name].is_string() {
                    return Err(invalid());
                }
            }
        }
        // This leaf owns member-associated interaction records only. Refuse an
        // unrelated map entry rather than silently discard a separate store update.
        for id in incoming_interactions.values.keys() {
            if !indices.contains_key(id) && !incoming.iter().any(|row| row["id"] == id.as_str()) {
                return Err(invalid());
            }
        }
        let mut observations = Vec::new();
        let mut fields = Vec::new();
        let mut seen_fields = BTreeSet::new();
        // XbX7.processMembers reads the state captured before the entire batch.
        // Do not let an earlier duplicate supply a later duplicate's self email.
        for wire in incoming {
            let id = key(&wire)?;
            profile::validate(&wire)?;
            let previous = indices.get(id).map(|i| &before[*i]);
            let normalized =
                profile::normalize(&wire, normalize_search && flags.normalize_external)?;
            let version = wire.get("updated").and_then(Value::as_u64);
            let skipped = directory
                && version.is_some_and(|v| v > 0)
                && previous.is_some_and(|old| {
                    old.source.get("updated") == wire.get("updated")
                        && incoming_interactions
                            .values
                            .get(id)
                            .is_none_or(|observed| old.interaction == Some(*observed))
                });
            let (native, projection) = if skipped {
                let old = previous.ok_or_else(invalid)?;
                (old.native.clone(), old.source.clone())
            } else {
                let effective = profile::effective(
                    &normalized,
                    previous.map(|p| &p.source),
                    &scope.owner.user_id,
                    flags.preserve_self_email,
                )?;
                let native =
                    profile::preserve_native(previous.map(|p| &p.native), &normalized, &effective)?;
                for field in profile::name_inputs(&effective)? {
                    if seen_fields.insert(field.clone()) {
                        fields.push(field);
                    }
                }
                (native, effective)
            };
            observations.push(Observation {
                before: previous.cloned(),
                wire,
                normalized_wire: normalized,
                native_preserved: native,
                source_projection: projection,
                skipped_same_version: skipped,
            });
        }
        Ok(Self {
            scope,
            flags,
            before,
            observations,
            incoming_interactions,
            fields,
        })
    }
    pub fn fields(&self) -> &[String] {
        &self.fields
    }
    pub fn generation(&self) -> &str {
        &self.scope.generation
    }
    /// No mutation occurs here. Parent must recheck transport/config/cookie and
    /// owner/Visit immediately before adopting both returned collections.
    pub fn finish(
        self,
        account: &str,
        owner: &WorkspaceSession,
        epoch: u64,
        generation: &str,
        current: &[Current],
        flags: &Flags,
        reply: RendererReply,
    ) -> Result<Applied, Error> {
        if !self.scope.current(account, owner, epoch, generation)
            || current != self.before
            || flags != &self.flags
            || reply.generation != self.scope.generation
            || reply.operation != "ordinary-toLowerCase"
            || reply.fields != self.fields
            || reply.lowered.len() != self.fields.len()
        {
            return Err(invalid());
        }
        let lowered: BTreeMap<_, _> = self
            .fields
            .iter()
            .zip(reply.lowered)
            .map(|(a, b)| (a.clone(), b))
            .collect();
        let mut observations = self.observations;
        let mut rows = self.before;
        // ScqY partitions using the old state, then dispatches the incoming map
        // before XbX7 applies explicit is_stranger from the unskipped batch.
        for row in &mut rows {
            if let Some(value) = self.incoming_interactions.values.get(key(&row.source)?) {
                row.interaction = Some(*value);
            }
        }
        for observation in &mut observations {
            if observation.skipped_same_version {
                continue;
            }
            observation.source_projection =
                profile::derive(&observation.source_projection, &lowered)?;
            let id = key(&observation.source_projection)?;
            let index = rows.iter().position(|row| row.source["id"] == id);
            observation.native_preserved = profile::preserve_native(
                index.map(|i| &rows[i].native),
                &observation.normalized_wire,
                &observation.source_projection,
            )?;
            let interaction = match observation.normalized_wire.get("is_stranger") {
                Some(Value::Bool(flag)) => Some(!flag),
                None => index
                    .and_then(|i| rows[i].interaction)
                    .or_else(|| self.incoming_interactions.values.get(id).copied()),
                _ => return Err(invalid()),
            };
            let row = Current {
                native: observation.native_preserved.clone(),
                source: observation.source_projection.clone(),
                interaction,
            };
            if let Some(index) = index {
                rows[index] = row;
            } else {
                rows.push(row);
            }
        }
        let returned = observations
            .iter()
            .map(|o| {
                rows.iter()
                    .find(|r| r.source["id"] == o.wire["id"])
                    .map(|r| r.source.clone())
                    .ok_or_else(invalid)
            })
            .collect::<Result<_, _>>()?;
        Ok(Applied {
            rows,
            observations,
            returned,
        })
    }
}
