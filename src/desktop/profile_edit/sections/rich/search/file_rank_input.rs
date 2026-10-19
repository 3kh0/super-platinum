//! File rank inputs supplement one existing member/group cache observation.
use super::{Search, file_source};
use crate::{app_home::autocomplete, state::ShellState};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
use super_platinum_core::{
    block_actions::home::workflow::directory_observations::{Entity, Offline},
    slack::Error,
};

fn unavailable() -> Error {
    Error::Api("profile_file_rank_input_unavailable".into())
}
#[derive(Clone)]
pub(super) struct Input {
    source: Arc<file_source::Snapshot>,
    creators: BTreeSet<String>,
    native: Value,
    ignore_fuzzy: bool,
}
impl Input {
    pub(super) fn current(&self, shell: &ShellState, search: &Search) -> bool {
        self.source.current(shell)
            && native(shell, search.lease.team(), &self.creators).is_ok_and(|v| v == self.native)
    }
    pub(super) fn ignore_fuzzy(&self) -> bool {
        self.ignore_fuzzy
    }
}
fn treatment(raw: &Value) -> Result<bool, Error> {
    if raw["ok"] != true || raw.get("error").is_some_and(|v| !v.is_null()) {
        return Err(unavailable());
    }
    let assignments = raw["assignments"].as_object().ok_or_else(unavailable)?;
    Ok(
        match assignments.get("ml_autocomplete_ignore_file_fuzzy_score") {
            None => false,
            Some(value) => value["group"].as_str().ok_or_else(unavailable)? == "treatment",
        },
    )
}
pub(super) fn capture(
    shell: &ShellState,
    search: &Search,
    source: Arc<file_source::Snapshot>,
    cache: &autocomplete::Ranking,
    assignments: &Value,
    selectors: &mut Value,
) -> Result<Input, Error> {
    if !search.current(shell) || !source.current(shell) || !cache.current(shell) {
        return Err(unavailable());
    }
    let mut ids = Ids::default();
    for row in source.rows() {
        ids.add(row.effective())?;
    }
    let native = native(shell, search.lease.team(), &ids.creators)?;
    augment(selectors, cache.offline(), &ids, &native)?;
    Ok(Input {
        source,
        creators: ids.creators,
        native,
        ignore_fuzzy: treatment(assignments)?,
    })
}
#[derive(Default)]
struct Ids {
    creators: BTreeSet<String>,
    channels: BTreeSet<String>,
    groups: BTreeSet<String>,
}
impl Ids {
    fn add(&mut self, raw: &Value) -> Result<(), Error> {
        match raw.get("user") {
            None | Some(Value::Null) => {}
            Some(Value::String(id)) => {
                if !id.is_empty() {
                    self.creators.insert(id.clone());
                }
            }
            _ => return Err(unavailable()),
        }
        for (key, target) in [
            ("channels", &mut self.channels),
            ("groups", &mut self.groups),
        ] {
            if let Some(value) = raw.get(key).filter(|v| !v.is_null()) {
                for id in value.as_array().ok_or_else(unavailable)? {
                    let id = id
                        .as_str()
                        .filter(|id| !id.is_empty())
                        .ok_or_else(unavailable)?;
                    target.insert(id.into());
                }
            }
        }
        Ok(())
    }
}
/// These are the real current native IM selector entries. No fabricated peer,
/// creator member record, or DM lookup from a file's remote user object.
fn native(shell: &ShellState, team: &str, creators: &BTreeSet<String>) -> Result<Value, Error> {
    let workspace = shell.core.workspaces.get(team).ok_or_else(unavailable)?;
    let mut ims = BTreeMap::new();
    for channel in workspace.channels.values().filter(|c| c.is_im) {
        let Some(peer) = channel.user.as_ref().filter(|id| creators.contains(*id)) else {
            continue;
        };
        if ims
            .insert(
                peer.clone(),
                serde_json::to_value(channel).map_err(|_| unavailable())?,
            )
            .is_some()
        {
            return Err(unavailable());
        }
    }
    serde_json::to_value(ims).map_err(|_| unavailable())
}
fn augment(
    selectors: &mut Value,
    offline: &Offline,
    ids: &Ids,
    native: &Value,
) -> Result<(), Error> {
    if !selectors["offline"].is_object()
        || !selectors["remoteDm"].is_object()
        || !selectors["ims"].is_object()
    {
        return Err(unavailable());
    }
    if selectors["offline"].get("member").is_none() {
        selectors["offline"]["member"] = json!({});
    }
    selectors["entityCtr"]["file"] = json!(offline.other_ctr());
    for (kind, values) in [
        (Entity::Member, &ids.creators),
        (Entity::Channel, &ids.channels),
        (Entity::Mpim, &ids.groups),
    ] {
        for id in values {
            let value = offline.features(id, kind).ok_or_else(unavailable)?;
            selectors["offline"][kind.key()][id] =
                json!({"team_ctr_90dd":value.team_ctr_90dd,"user_ctr_30dd":value.user_ctr_30dd});
        }
    }
    for id in &ids.creators {
        if let Some(value) = offline.dm(id) {
            selectors["remoteDm"][id] = value.clone();
        }
        if let Some(value) = native.get(id) {
            // Existing member projection can add immutable selector facts; the
            // file feature only consumes the same native IM's existence.
            if selectors["ims"].get(id).is_none() {
                selectors["ims"][id] = value.clone();
            }
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests;
