//! Human selection is local. Delivery resolves the person only on explicit Share.
use super::*;
use serde_json::json;
use super_platinum_core::slack::models::Channel;

#[derive(Clone)]
pub(super) struct Person {
    pub id: String,
    raw: Value,
}
impl Person {
    pub(super) fn capture(shell: &ShellState, id: &str) -> Option<Self> {
        if !(9..=64).contains(&id.len())
            || !id.starts_with('U')
            || !id
                .bytes()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
        {
            return None;
        }
        let team = shell.core.active_team.as_ref()?;
        let workspace = shell.core.workspaces.get(team)?;
        let user = workspace.users.get(id)?;
        if user.id != id || id == workspace.self_user_id || user.deleted || user.is_bot {
            return None;
        }
        let raw = serde_json::to_value(user).ok()?;
        if !eligible_raw(&raw, team) || !policy_owner::bounded(&raw) {
            return None;
        }
        Some(Self { id: id.into(), raw })
    }
    pub(super) fn current(&self, shell: &ShellState) -> bool {
        Self::capture(shell, &self.id).is_some_and(|p| p.raw == self.raw)
    }
    pub(super) fn fresh(&self, raw: &Value, team: &str) -> bool {
        recipients::raw_peer(raw, &self.id).is_some_and(|p| {
            p["deleted"] == false && p["is_bot"] == false && eligible_raw(&p, team)
        })
    }
}
fn eligible_raw(raw: &Value, team: &str) -> bool {
    [
        "isUnknown",
        "isNonExistent",
        "is_stranger",
        "is_invited_user",
    ]
    .iter()
    .all(|k| raw.get(*k).is_none_or(|v| v == &Value::Bool(false)))
        && raw.get("team_id").is_none_or(|v| v.as_str() == Some(team))
}
pub(super) fn append_people(shell: &ShellState, query: &str, out: &mut Vec<Destination>) {
    if query.trim().is_empty() || query.starts_with('#') {
        return;
    }
    let Some(workspace) = shell
        .core
        .active_team
        .as_ref()
        .and_then(|t| shell.core.workspaces.get(t))
    else {
        return;
    };
    for person in crate::new_dm::people(workspace, query, &[]) {
        if out.len() >= 30 {
            break;
        }
        if Person::capture(shell, &person.id).is_none()
            || workspace.channels.values().any(|c| {
                c.is_im
                    && c.user.as_deref() == Some(person.id.as_str())
                    && recipients::eligible(shell, &c.id)
            })
        {
            continue;
        }
        out.push(Destination {
            id: format!("person:{}", person.id),
            name: person.name,
            private: true,
        });
    }
}
pub(super) fn capture_person(shell: &ShellState, id: &str) -> Option<Value> {
    Person::capture(shell, id.strip_prefix("person:")?).map(|p| p.raw)
}
pub(super) fn selected_person(shell: &ShellState, id: &str) -> Option<Destination> {
    let person = Person::capture(shell, id.strip_prefix("person:")?)?;
    let workspace = shell
        .core
        .workspaces
        .get(shell.core.active_team.as_ref()?)?;
    Some(Destination {
        id: id.into(),
        name: workspace.display_name(&person.id),
        private: true,
    })
}

pub(super) fn opened_id(raw: &Value) -> Result<String, Refusal> {
    if !policy_owner::bounded(raw)
        || raw["ok"] != true
        || raw.get("error").is_some_and(|v| !v.is_null())
    {
        return Err(Refusal::RecipientResolution);
    }
    let channel = raw
        .get("channel")
        .filter(|v| v.is_object())
        .ok_or(Refusal::RecipientResolution)?;
    let id = channel["id"].as_str().ok_or(Refusal::RecipientResolution)?;
    if !(9..=64).contains(&id.len())
        || !id.starts_with('D')
        || !id
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
    {
        return Err(Refusal::RecipientResolution);
    }
    Ok(id.into())
}
pub(super) fn verified_channel(raw: &Value, id: &str, person: &str) -> Result<Channel, Refusal> {
    if !policy_owner::bounded(raw)
        || raw["ok"] != true
        || raw.get("error").is_some_and(|v| !v.is_null())
    {
        return Err(Refusal::Destination);
    }
    let channel = &raw["channel"];
    if channel["id"] != id
        || channel["is_im"] != true
        || channel["user"] != person
        || channel["is_ext_shared"] != false
        || channel
            .get("is_open")
            .is_some_and(|v| v != &Value::Bool(true))
    {
        return Err(Refusal::Destination);
    }
    let mut channel: Channel =
        serde_json::from_value(channel.clone()).map_err(|_| Refusal::Destination)?;
    if channel.is_mpim || channel.is_channel || channel.is_archived {
        return Err(Refusal::Destination);
    }
    for key in [
        "isUnknown",
        "isNonExistent",
        "is_frozen",
        "is_pending_ext_shared",
        "is_file",
    ] {
        if channel
            .extra
            .get(key)
            .is_some_and(|v| v != &Value::Bool(false))
        {
            return Err(Refusal::Destination);
        }
    }
    // The just-accepted open is the evidence for this fact if info omits it.
    channel.extra.insert("is_open".into(), json!(true));
    Ok(channel)
}
pub(super) fn remember(
    shell: &mut ShellState,
    channel: Channel,
    person: &Person,
) -> Result<Destination, Refusal> {
    if !person.current(shell) {
        return Err(Refusal::Changed);
    }
    let team = shell.core.active_team.as_ref().ok_or(Refusal::Owner)?;
    let workspace = shell.core.workspaces.get_mut(team).ok_or(Refusal::Owner)?;
    let id = channel.id.clone();
    if workspace.channels.get(&id).is_some_and(|c| {
        !c.is_im || c.is_mpim || c.is_archived || c.user.as_deref() != Some(person.id.as_str())
    }) {
        return Err(Refusal::Destination);
    }
    // Retain warm read/unread fields and known metadata on an existing entry.
    workspace
        .channels
        .entry(id.clone())
        .and_modify(|c| {
            c.extra.insert("is_open".into(), json!(true));
        })
        .or_insert(channel);
    admission::selected(shell, &id).ok_or(Refusal::Destination)
}
