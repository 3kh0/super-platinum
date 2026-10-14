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
    content: PreparedKind,
}
enum PreparedKind {
    Event(DraftObservation),
    Delete {
        scope: Scope,
        draft_id: String,
        event_ts: String,
        client_msg_id: Option<String>,
    },
}
impl Prepared {
    fn scope(&self) -> &Scope {
        match &self.content {
            PreparedKind::Event(event) => &event.scope,
            PreparedKind::Delete { scope, .. } => scope,
        }
    }
    /// Resolve an omitted UUID only from the exact currently acknowledged
    /// identity, under the owner's gate. Pending creates and UI state cannot
    /// supply this proof; CAS retry repeats the lookup against the new record.
    fn resolve(&self, state: &DraftSync) -> Result<Option<DraftObservation>, Fault> {
        match &self.content {
            PreparedKind::Event(event) => Ok(Some(event.clone())),
            PreparedKind::Delete {
                scope,
                draft_id,
                event_ts,
                client_msg_id,
            } => {
                let client_msg_id = match client_msg_id {
                    Some(id) => id.clone(),
                    None => match state
                        .acknowledged()
                        .filter(|base| &base.scope == scope && &base.id == draft_id)
                    {
                        Some(base) => base.client_msg_id.clone(),
                        None if state.has_observed_retirement_at_or_after(draft_id, event_ts) => {
                            return Ok(None);
                        }
                        None => return Err(Fault::InvalidReceipt),
                    },
                };
                Ok(Some(DraftObservation {
                    scope: scope.clone(),
                    draft_id: draft_id.clone(),
                    event_ts: event_ts.clone(),
                    client_msg_id,
                    kind: ObservationKind::Deleted,
                }))
            }
        }
    }
}

pub(crate) struct Observed {
    pub before: View,
    pub after: View,
    pub changed: bool,
}

/// Capture once when accepting the authenticated socket generation, before
/// awaiting any frames. Never recapture a source around an already queued frame.
/// Binding owns credentials, so this type deliberately does not implement Debug.
pub(crate) struct Source {
    binding: network::Binding,
    account: String,
    team: String,
    user: String,
    generation: u64,
}
impl Source {
    pub fn capture(shell: &ShellState, team: &str, generation: u64) -> Result<Self, Invalid> {
        if crate::fixture::is_fixture() || generation == 0 {
            return Err(Invalid::Source);
        }
        let workspace = shell.core.workspaces.get(team).ok_or(Invalid::Source)?;
        if workspace.rt_generation != generation {
            return Err(Invalid::Source);
        }
        Ok(Self {
            binding: network::Binding::capture(shell, team).map_err(|_| Invalid::Source)?,
            account: shell.core.active_account.clone().ok_or(Invalid::Source)?,
            team: team.into(),
            user: workspace.self_user_id.clone(),
            generation,
        })
    }

    /// The candidate destination must already have a durable owner. That owner
    /// rechecks its server/client identity when applying this immutable event.
    pub fn prepare(
        &self,
        shell: &ShellState,
        key: &DraftKey,
        raw: &RawEvent,
    ) -> Result<Prepared, Invalid> {
        if !self.binding.current(shell)
            || key.team != self.team
            || shell
                .core
                .workspaces
                .get(&self.team)
                .is_none_or(|workspace| {
                    workspace.rt_generation != self.generation
                        || workspace.self_user_id != self.user
                })
        {
            return Err(Invalid::Source);
        }
        decode(
            &Scope {
                account_id: self.account.clone(),
                key: key.clone(),
            },
            &self.user,
            raw,
        )
    }
}

fn identifier(value: &str) -> bool {
    !value.is_empty() && !value.chars().any(|c| c.is_whitespace() || c.is_control())
}

/// Synthetic native verification can reuse decoding without impersonating an
/// authenticated Source. Only the preinstalled isolated owner admits a key.
pub(super) fn prepare_fixture(
    shell: &ShellState,
    key: &DraftKey,
    raw: &RawEvent,
) -> Result<Prepared, Invalid> {
    let binding = super::projection_fixture::binding(shell).ok_or(Invalid::Source)?;
    let editor = binding.editors.get(key).ok_or(Invalid::Source)?;
    if !shell
        .draft_writes
        .writers
        .get(super::projection_fixture::ACCOUNT, binding.epoch, key)
        .is_some_and(|current| Arc::ptr_eq(editor, &current))
    {
        return Err(Invalid::Source);
    }
    let user = &shell
        .core
        .workspaces
        .get(&key.team)
        .ok_or(Invalid::Source)?
        .self_user_id;
    decode(
        &Scope {
            account_id: super::projection_fixture::ACCOUNT.into(),
            key: key.clone(),
        },
        user,
        raw,
    )
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
    raw.get(name)
        .is_none_or(|value| value.as_str() == Some(expected))
}
fn decode(scope: &Scope, user: &str, raw: &RawEvent) -> Result<Prepared, Invalid> {
    if !super_platinum_core::drafts::sync::valid_account_id(&scope.account_id)
        || !identifier(user)
        || !identifier(&scope.key.team)
        || !identifier(&scope.key.channel)
    {
        return Err(Invalid::Source);
    }
    if !matches!(
        raw.kind.as_str(),
        "draft_create" | "draft_update" | "draft_delete"
    ) {
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
    let id = raw
        .rest
        .get("draft_id")
        .and_then(Value::as_str)
        .filter(|id| identifier(id))
        .ok_or(Invalid::Identity)?;
    let stamp = raw
        .rest
        .get("event_ts")
        .and_then(Value::as_str)
        .filter(|stamp| event_timestamp(stamp))
        .ok_or(Invalid::Timestamp)?;
    let draft = raw.rest.get("draft");
    if raw.kind == "draft_delete" {
        // Recorded deletes carry no draft object. An optional UUID is checked
        // here; an omitted UUID remains unresolved until the durable owner gate.
        if draft.is_some() {
            return Err(Invalid::Event);
        }
        let client_msg_id = raw
            .rest
            .get("client_msg_id")
            .map(|value| {
                value
                    .as_str()
                    .filter(|id| super_platinum_core::drafts::sync::valid_account_id(id))
                    .map(str::to_owned)
                    .ok_or(Invalid::Identity)
            })
            .transpose()?;
        return Ok(Prepared {
            content: PreparedKind::Delete {
                scope: scope.clone(),
                draft_id: id.into(),
                event_ts: stamp.into(),
                client_msg_id,
            },
        });
    }
    let client_id = raw
        .rest
        .get("client_msg_id")
        .and_then(Value::as_str)
        .or_else(|| draft?.get("client_msg_id")?.as_str())
        .ok_or(Invalid::Identity)?;
    // Reuse the reducer's UUID validation without a new public UUID helper.
    if !super_platinum_core::drafts::sync::valid_account_id(client_id)
        || raw
            .rest
            .get("client_msg_id")
            .is_some_and(|value| value.as_str() != Some(client_id))
        || draft.is_some_and(|draft| {
            draft.get("id").and_then(Value::as_str) != Some(id)
                || draft.get("client_msg_id").and_then(Value::as_str) != Some(client_id)
        })
    {
        return Err(Invalid::Identity);
    }
    let kind = {
        let draft = draft
            .filter(|draft| draft.is_object())
            .ok_or(Invalid::Event)?;
        match active(scope, id, client_id, draft) {
            Some(remote) => ObservationKind::Active(remote),
            None => ObservationKind::Unsupported(draft.clone()),
        }
    };
    Ok(Prepared {
        content: PreparedKind::Event(DraftObservation {
            scope: scope.clone(),
            draft_id: id.into(),
            client_msg_id: client_id.into(),
            event_ts: stamp.into(),
            kind,
        }),
    })
}

fn active(scope: &Scope, id: &str, client: &str, raw: &Value) -> Option<RemoteDraft> {
    // This is intentionally stricter than the display queue parser. Partial
    // frames and unverified lifecycle/extension fields must block owned writes.
    const FIELDS: &[&str] = &[
        "id",
        "client_msg_id",
        "team_id",
        "user",
        "user_id",
        "last_updated_ts",
        "blocks",
        "file_ids",
        "destinations",
        "is_deleted",
        "is_sent",
        "date_scheduled",
        "date_created",
        "attachments",
    ];
    if raw
        .as_object()?
        .keys()
        .any(|field| !FIELDS.contains(&field.as_str()))
        || raw.get("is_deleted")?.as_bool()?
        || raw.get("is_sent")?.as_bool()?
        || raw
            .get("attachments")
            .is_some_and(|value| !matches!(value, Value::Array(values) if values.is_empty()))
    {
        return None;
    }
    let schedule = raw.get("date_scheduled")?;
    if schedule
        .as_i64()
        .or_else(|| schedule.as_str()?.parse().ok())?
        != 0
    {
        return None;
    }
    let remote = RemoteDraft {
        scope: scope.clone(),
        id: id.into(),
        client_msg_id: client.into(),
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
        if prepared.scope() != &self.scope {
            return Err(Fault::InvalidReceipt);
        }
        let _guard = self.gate.lock().await;
        for _ in 0..4 {
            let before = self.current().await?;
            if before.blocked.is_some() || before.uncommitted.is_some() {
                return Err(Fault::AlternateEdits);
            }
            let mut stored = before.stored.clone().ok_or(Fault::Unmanaged)?;
            let Some(event) = prepared.resolve(&stored.state)? else {
                return Ok(Observed {
                    after: before.clone(),
                    before,
                    changed: false,
                });
            };
            let changed = stored
                .state
                .observe_event(&event)
                .map_err(|_| Fault::InvalidReceipt)?;
            if !changed {
                return Ok(Observed {
                    after: before.clone(),
                    before,
                    changed: false,
                });
            }
            let old = before.stored.as_ref().unwrap().state.desired();
            let local = if old == stored.state.desired() {
                before.local.clone()
            } else {
                match stored.state.desired() {
                    Some(snapshot) => LocalDraft {
                        composer: codec::decode(&self.scope.key, snapshot)
                            .map_err(|_| Fault::Unsupported)?
                            .composer()
                            .clone(),
                        attachments: vec![],
                    },
                    None => LocalDraft::default(),
                }
            };
            match self
                .save(Some(stored.revision), local.clone(), stored.state)
                .await
            {
                Ok(stored) => {
                    return Ok(Observed {
                        before,
                        after: View {
                            local,
                            stored: Some(stored),
                            blocked: None,
                            uncommitted: None,
                        },
                        changed: true,
                    });
                }
                Err(Fault::Stale) => continue,
                Err(error) => return Err(error),
            }
        }
        Err(Fault::Stale)
    }
}

#[cfg(test)]
mod tests;
