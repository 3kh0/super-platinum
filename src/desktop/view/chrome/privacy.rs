//! Passive private-header label data from an already current canonical row.
//! No freshness, navigation, action, resource or Source-ready authority is made.
use crate::{app_home::channel_fetches::canonical, state::ShellState};
use serde_json::{Value, json};
use super_platinum_core::slack::models::Channel;

const FIXTURE_CHANNEL: &str = "CHEADERPRIVATE";
const FIXTURE_NAME: &str = "private-planning";
// Defensive native label bound, not a Source channel-name schema assertion.
const MAX_NAME_BYTES: usize = 4096;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Provenance {
    Owned,
    Layout,
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct Title {
    label: String,
    provenance: Provenance,
}

impl Title {
    pub(crate) fn label(&self) -> &str {
        &self.label
    }

    pub(crate) fn provenance(&self) -> &'static str {
        match self.provenance {
            Provenance::Owned => "owned",
            Provenance::Layout => "layout",
        }
    }
}

fn falsey(value: Option<&Value>) -> bool {
    matches!(value, None | Some(Value::Null | Value::Bool(false)))
}

fn native_flag(value: Option<&Value>, native: bool) -> bool {
    match value {
        Some(Value::Bool(value)) => *value == native,
        None | Some(Value::Null) => !native,
        _ => false,
    }
}

fn falsey_extra(row: &Value, native: &Channel, key: &str) -> bool {
    row.get(key) == native.extra.get(key) && falsey(row.get(key))
}

fn no_record(row: &Value, native: &Channel) -> bool {
    let properties = row.get("properties");
    if properties != native.extra.get("properties") {
        return false;
    }
    let properties = match properties {
        None | Some(Value::Null) => return true,
        Some(Value::Object(properties)) => properties,
        Some(_) => return false,
    };
    let record = match properties.get("record_channel") {
        None | Some(Value::Null) => return true,
        Some(Value::Object(record)) => record,
        Some(_) => return false,
    };
    matches!(record.get("record_id"), None | Some(Value::Null))
        || record.get("record_id").and_then(Value::as_str) == Some("")
}

pub(super) fn qualified(row: &Value, native: &Channel) -> bool {
    if !row.is_object()
        || row["id"].as_str() != Some(native.id.as_str())
        || row["name"].as_str() != native.name.as_deref()
        || native.is_im
        || native.is_mpim
        || !(native.is_private || native.is_group)
        || !(row.get("is_private") == Some(&Value::Bool(true))
            || row.get("is_group") == Some(&Value::Bool(true)))
    {
        return false;
    }
    for (key, flag) in [
        ("is_channel", native.is_channel),
        ("is_group", native.is_group),
        ("is_private", native.is_private),
        ("is_im", native.is_im),
        ("is_mpim", native.is_mpim),
        ("is_archived", native.is_archived),
        ("is_ext_shared", native.is_ext_shared),
    ] {
        if !native_flag(row.get(key), flag) {
            return false;
        }
    }
    // These are actual preserved JSON lookups, including exact absence/null.
    // A defaulted typed bool cannot substitute for this source/native pair.
    for key in [
        "is_frozen",
        "is_file",
        "isUnknown",
        "isNonExistent",
        "is_user_deleted",
        "is_deleted",
        "deleted",
    ] {
        if !falsey_extra(row, native, key) {
            return false;
        }
    }
    // A false record-id path also makes KyqZ.$n/fiUV.w false before their
    // membership readers. Unrelated properties and state flags are not borrowed.
    // Reuse only the existing pure ordinary-name refusals. This local fixed
    // helper tag is never returned, dispatched or used as an App Home grant.
    no_record(row, native)
        && canonical::ordinary_label(
            &json!({"type":"channel","channel_id":native.id.as_str()}),
            row,
        )
        .is_ok()
}

pub(super) fn project(s: &ShellState) -> Option<Title> {
    if std::env::var_os("SUPER_PLATINUM_FIXTURE").is_some()
        || !s.signed_in
        || s.loading
        || !s.detail_open()
        || s.core.screen != super_platinum_core::state::Screen::Main
    {
        return None;
    }
    let team = s.core.active_team.as_deref()?;
    let id = s.core.active_channel.as_deref()?;
    let vm = s.channels.get(s.active_channel)?;
    let native = s.core.workspaces.get(team)?.channels.get(id)?;
    let name = native
        .name
        .as_deref()
        .filter(|name| !name.is_empty() && name.len() <= MAX_NAME_BYTES)?;
    if native.id != id
        || vm.id != id
        || vm.name != name
        || vm.is_im
        || vm.is_mpim
        || !vm.is_private
        || vm.is_private != (native.is_private || native.is_group)
        || vm.is_ext_shared != native.is_ext_shared
    {
        return None;
    }
    let observed = canonical::observed(s, team).ok()?;
    if !observed.current_for(s, team) || !qualified(observed.row(id)?, native) {
        return None;
    }
    let title = Title {
        label: name.to_owned(),
        provenance: Provenance::Owned,
    };
    observed.current_for(s, team).then_some(title)
}

fn fixture_row() -> Value {
    json!({"id":FIXTURE_CHANNEL,"name":FIXTURE_NAME,
        "is_channel":true,"is_private":true,"is_group":false,
        "is_im":false,"is_mpim":false,"is_archived":false,"is_ext_shared":false,
        "is_frozen":false,"is_file":false,"properties":{}})
}

pub(super) fn fixture_model(s: &ShellState) -> Option<Title> {
    if s.core.session.is_some()
        || s.core.transport.is_some()
        || s.core.active_account.is_some()
        || s.loading
        || !s.detail_open()
        || s.core.screen != super_platinum_core::state::Screen::Main
        || s.core.active_channel.as_deref() != Some(FIXTURE_CHANNEL)
    {
        return None;
    }
    let team = s.core.active_team.as_deref()?;
    let native = s.core.workspaces.get(team)?.channels.get(FIXTURE_CHANNEL)?;
    let vm = s.channels.get(s.active_channel)?;
    if vm.id != FIXTURE_CHANNEL
        || vm.name != FIXTURE_NAME
        || vm.is_im
        || vm.is_mpim
        || !vm.is_private
        || vm.is_ext_shared
        || native.name.as_deref() != Some(FIXTURE_NAME)
        || !qualified(&fixture_row(), native)
    {
        return None;
    }
    Some(Title {
        label: FIXTURE_NAME.into(),
        provenance: Provenance::Layout,
    })
}

pub(super) fn display(s: &ShellState) -> Option<Title> {
    match std::env::var_os("SUPER_PLATINUM_FIXTURE") {
        None => project(s),
        Some(value) => match value.to_str() {
            Some(
                "conversation-header-private"
                | "conversation-header-private-narrow"
                | "conversation-header-private-topic"
                | "conversation-header-private-topic-narrow",
            ) => fixture_model(s),
            _ => None,
        },
    }
}

/// Disconnected synthetic layout data; never a canonical observation or owner.
pub(crate) fn fixture(s: &mut ShellState) {
    s.core.session = None;
    s.core.transport = None;
    s.core.active_account = None;
    s.overlay = None;
    if let Some(team) = s.core.active_team.clone() {
        if let Some(workspace) = s.core.workspaces.get_mut(&team) {
            if let Ok(channel) = serde_json::from_value(fixture_row()) {
                workspace.channels.insert(FIXTURE_CHANNEL.into(), channel);
                s.core.active_channel = Some(FIXTURE_CHANNEL.into());
            }
        }
    }
    s.refresh_from_core();
    if let Some(index) = s.channels.iter().position(|c| c.id == FIXTURE_CHANNEL) {
        s.select_channel(index, crate::state::ChannelOpen::Global);
    }
}

#[cfg(test)]
mod tests;
