//! Conservative, disconnected observation boundary. No realtime hook, remote
//! enrollment, request dispatch or visible-composer replacement is enabled.
use super::*;
use crate::state::ShellState;
use serde_json::Value;
use super_platinum_core::{
    drafts::sync::{DraftObservation, ObservationKind},
    slack::events::RawEvent,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Invalid {
    Source,
    Identity,
    Timestamp,
    Event,
}

/// Constructed only after captured authenticated-source and raw identity checks.
/// Owning storage still revalidates its current server/client identity at commit.
pub(crate) struct Prepared {
    event: DraftObservation,
}

pub(crate) struct Observed {
    pub before: View,
    pub after: View,
    pub changed: bool,
}

/// Future realtime integration calls this before yielding its shell snapshot.
/// The destination is a candidate existing owner, not permission to adopt one.
pub(crate) fn prepare(
    shell: &ShellState,
    team: &str,
    generation: u64,
    key: &DraftKey,
    raw: &RawEvent,
) -> Result<Prepared, Invalid> {
    if crate::fixture::is_fixture()
        || key.team != team
        || shell
            .core
            .workspaces
            .get(team)
            .is_none_or(|workspace| workspace.rt_generation != generation)
    {
        return Err(Invalid::Source);
    }
    // Capture verifies UUID, token/user/workspace consistency and transport.
    // No credentials enter Prepared, storage, Debug output or fixture evidence.
    let binding = network::Binding::capture(shell, team).map_err(|_| Invalid::Source)?;
    if !binding.current(shell) {
        return Err(Invalid::Source);
    }
    let scope = Scope {
        account_id: shell.core.active_account.clone().ok_or(Invalid::Source)?,
        key: key.clone(),
    };
    let user = &shell.core.workspaces.get(team).ok_or(Invalid::Source)?.self_user_id;
    decode(&scope, user, raw)
}

fn identifier(value: &str) -> bool {
    !value.is_empty() && !value.chars().any(|c| c.is_whitespace() || c.is_control())
}
fn event_timestamp(value: &str) -> bool {
    value.split_once('.').is_some_and(|(seconds, fraction)| {
        !seconds.is_empty()
            && seconds.bytes().all(|b| b.is_ascii_digit())
            && seconds.parse::<u64>().is_ok_and(|s| s > 0)
            && fraction.len() == 6
            && fraction.bytes().all(|b| b.is_ascii_digit())
    })
}
fn optional_identity(raw: &Value, name: &str, expected: &str) -> bool {
    raw.get(name).is_none_or(|value| value.as_str() == Some(expected))
}
fn decode(scope: &Scope, user: &str, raw: &RawEvent) -> Result<Prepared, Invalid> {
    if !super_platinum_core::drafts::sync::valid_account_id(&scope.account_id)
        || !identifier(user)
        || !identifier(&scope.key.team)
        || !identifier(&scope.key.channel)
    {
        return Err(Invalid::Source);
    }
    if !matches!(raw.kind.as_str(), "draft_create" | "draft_update" | "draft_delete") {
        return Err(Invalid::Event);
    }
    let envelope = Value::Object(raw.rest.clone().into_iter().collect());
    for object in [&envelope, raw.rest.get("draft").unwrap_or(&Value::Null)] {
        if !optional_identity(object, "team_id", &scope.key.team)
            || !optional_identity(object, "user_id", user)
            || !optional_identity(object, "user", user)
        {
            return Err(Invalid::Source);
        }
    }
    let id = raw.rest.get("draft_id").and_then(Value::as_str)
        .filter(|id| identifier(id)).ok_or(Invalid::Identity)?;
    let stamp = raw.rest.get("event_ts").and_then(Value::as_str)
        .filter(|stamp| event_timestamp(stamp)).ok_or(Invalid::Timestamp)?;
    let draft = raw.rest.get("draft");
    let client_id = raw.rest.get("client_msg_id").and_then(Value::as_str)
        .or_else(|| draft?.get("client_msg_id")?.as_str()).ok_or(Invalid::Identity)?;
    // Reuse the reducer's UUID validation without a new public UUID helper.
    if !super_platinum_core::drafts::sync::valid_account_id(client_id)
        || raw.rest.get("client_msg_id").is_some_and(|value| value.as_str() != Some(client_id))
        || draft.is_some_and(|draft| {
            draft.get("id").and_then(Value::as_str) != Some(id)
                || draft.get("client_msg_id").and_then(Value::as_str) != Some(client_id)
        })
    {
        return Err(Invalid::Identity);
    }
    let kind = if raw.kind == "draft_delete" {
        // Recorded deletes carry no draft object. Ambiguous hybrids are not
        // interpreted as retirement, regardless of their optional fields.
        if draft.is_some() { return Err(Invalid::Event); }
        ObservationKind::Deleted
    } else {
        let draft = draft.filter(|draft| draft.is_object()).ok_or(Invalid::Event)?;
        match active(scope, id, client_id, draft) {
            Some(remote) => ObservationKind::Active(remote),
            None => ObservationKind::Unsupported(draft.clone()),
        }
    };
    Ok(Prepared {
        event: DraftObservation {
            scope: scope.clone(), draft_id: id.into(), client_msg_id: client_id.into(),
            event_ts: stamp.into(), kind,
        },
    })
}

fn active(scope: &Scope, id: &str, client: &str, raw: &Value) -> Option<RemoteDraft> {
    // This is intentionally stricter than the display queue parser. Partial
    // frames and unverified lifecycle/extension fields must block owned writes.
    const FIELDS: &[&str] = &[
        "id", "client_msg_id", "team_id", "user", "user_id", "last_updated_ts",
        "blocks", "file_ids", "destinations", "is_deleted", "is_sent",
        "date_scheduled", "date_created", "attachments",
    ];
    if raw.as_object()?.keys().any(|field| !FIELDS.contains(&field.as_str()))
        || raw.get("is_deleted")?.as_bool()? || raw.get("is_sent")?.as_bool()?
        || raw.get("attachments").is_some_and(|value| !matches!(value, Value::Array(values) if values.is_empty()))
    { return None; }
    let schedule = raw.get("date_scheduled")?;
    if schedule.as_i64().or_else(|| schedule.as_str()?.parse().ok())? != 0 { return None; }
    let remote = RemoteDraft {
        scope: scope.clone(), id: id.into(), client_msg_id: client.into(),
        last_updated_ts: raw.get("last_updated_ts")?.as_str()?.into(),
        snapshot: Snapshot {
            blocks: serde_json::from_value(raw.get("blocks")?.clone()).ok()?,
            file_ids: serde_json::from_value(raw.get("file_ids")?.clone()).ok()?,
            destinations: serde_json::from_value(raw.get("destinations")?.clone()).ok()?,
        },
    };
    DraftSync::adopt_remote(scope, remote.clone()).ok()?;
    Some(remote)
}

impl Owner {
    /// Persist an admitted event to its captured owner even if navigation has
    /// changed. Callers must guard later UI projection separately. No automatic
    /// adoption, editor rebase, network request or replay is performed here.
    pub async fn observe_event(&self, prepared: Prepared) -> Result<Observed, Fault> {
        let event = prepared.event;
        if event.scope != self.scope { return Err(Fault::InvalidReceipt); }
        let _guard = self.gate.lock().await;
        for _ in 0..4 {
            let before = self.current().await?;
            if before.blocked.is_some() || before.uncommitted.is_some() {
                return Err(Fault::AlternateEdits);
            }
            let mut stored = before.stored.clone().ok_or(Fault::Unmanaged)?;
            let changed = stored.state.observe_event(&event).map_err(|_| Fault::InvalidReceipt)?;
            if !changed {
                return Ok(Observed { after: before.clone(), before, changed: false });
            }
            let old = before.stored.as_ref().unwrap().state.desired();
            let local = if old == stored.state.desired() {
                before.local.clone()
            } else {
                match stored.state.desired() {
                    Some(snapshot) => LocalDraft {
                        composer: codec::decode(&self.scope.key, snapshot)
                            .map_err(|_| Fault::Unsupported)?.composer().clone(),
                        attachments: vec![],
                    },
                    None => LocalDraft::default(),
                }
            };
            match self.save(Some(stored.revision), local.clone(), stored.state).await {
                Ok(stored) => return Ok(Observed {
                    before,
                    after: View { local, stored: Some(stored), blocked: None, uncommitted: None },
                    changed: true,
                }),
                Err(Fault::Stale) => continue,
                Err(error) => return Err(error),
            }
        }
        Err(Fault::Stale)
    }
}

#[cfg(test)]
mod tests;
