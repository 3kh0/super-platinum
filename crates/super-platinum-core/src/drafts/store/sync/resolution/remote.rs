//! Explicit choice across the exact local inventory and positive server readback.
//! This module is deliberately unreferenced; it does not alter local settlement.
use super::*;
use crate::drafts::sync::reconciliation::resolve_reviewed_remote;
use crate::drafts::sync::{RemoteDraft, Snapshot};
use serde_json::Value;

#[derive(Clone, Debug)]
pub struct RemoteReview {
    expected: LocalReview,
    remote: RemoteDraft,
    raw_info: Value,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RemoteChoice {
    Local(LocalChoice),
    Remote,
}
#[derive(Debug)]
pub enum RemoteSettlement {
    /// Durable proposal only. The caller must perform the existing Editor/native
    /// projection handoff before replacing the visible composer.
    Applied {
        local: LocalDraft,
        stored: Stored,
    },
    Stale,
    Blocked,
}
impl RemoteReview {
    /// Caller captures LocalReview BEFORE the read, and binds that request to
    /// its account/epoch/transport/authenticated user. This only validates the
    /// returned raw evidence and supported identity/content; it cannot establish
    /// network provenance, current renderer ownership, or a user's choice.
    pub fn prepare(expected: LocalReview, raw_info: Value, user: &str) -> Result<Self, AppError> {
        let row = expected
            .stored
            .as_ref()
            .ok_or_else(|| invalid("remote review requires an owned draft"))?;
        let base = row
            .state
            .acknowledged()
            .ok_or_else(|| invalid("remote review requires an acknowledged server identity"))?;
        let raw = raw_info
            .get("draft")
            .filter(|v| v.is_object())
            .ok_or_else(|| invalid("missing positive draft readback"))?;
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
            "is_from_composer",
            "date_stashed",
            "file_permissions",
        ];
        if !crate::drafts::sync::supports_neutral_metadata(raw)
            || user.is_empty()
            || user.chars().any(|c| c.is_whitespace() || c.is_control())
            || raw_info.get("ok").and_then(Value::as_bool) != Some(true)
            || raw_info.get("error").is_some_and(|error| !error.is_null())
            || raw
                .as_object()
                .unwrap()
                .keys()
                .any(|k| !FIELDS.contains(&k.as_str()))
            || raw.get("id").and_then(Value::as_str) != Some(base.id.as_str())
            || raw.get("client_msg_id").and_then(Value::as_str) != Some(base.client_msg_id.as_str())
            || raw.get("is_deleted").and_then(Value::as_bool) != Some(false)
            || raw.get("is_sent").and_then(Value::as_bool) != Some(false)
            || raw
                .get("is_from_composer")
                .is_some_and(|v| v.as_bool() != Some(false))
            || raw
                .get("attachments")
                .is_some_and(|v| !matches!(v, Value::Array(a) if a.is_empty()))
            || raw
                .get("date_scheduled")
                .and_then(|v| v.as_i64().or_else(|| v.as_str()?.parse().ok()))
                != Some(0)
        {
            return Err(invalid("unsupported draft readback"));
        }
        for object in [&raw_info, raw] {
            for (name, value) in [
                ("team_id", expected.scope.key.team.as_str()),
                ("user", user),
                ("user_id", user),
            ] {
                if object.get(name).is_some_and(|v| v.as_str() != Some(value)) {
                    return Err(invalid("draft readback source mismatch"));
                }
            }
        }
        let remote = RemoteDraft {
            scope: expected.scope.clone(),
            id: base.id.clone(),
            client_msg_id: base.client_msg_id.clone(),
            last_updated_ts: raw
                .get("last_updated_ts")
                .and_then(Value::as_str)
                .ok_or_else(|| invalid("missing draft revision"))?
                .into(),
            snapshot: Snapshot {
                blocks: serde_json::from_value(
                    raw.get("blocks")
                        .cloned()
                        .ok_or_else(|| invalid("missing blocks"))?,
                )?,
                file_ids: serde_json::from_value(
                    raw.get("file_ids")
                        .cloned()
                        .ok_or_else(|| invalid("missing files"))?,
                )?,
                destinations: serde_json::from_value(
                    raw.get("destinations")
                        .cloned()
                        .ok_or_else(|| invalid("missing destinations"))?,
                )?,
            },
        };
        resolve_reviewed_remote(&row.state, &remote, Some(remote.snapshot.clone()))
            .map_err(|_| invalid("draft cannot be reconciled by positive review"))?;
        Ok(Self {
            expected,
            remote,
            raw_info,
        })
    }
    pub fn inventory(&self) -> &LocalReview {
        &self.expected
    }
    pub fn remote(&self) -> &RemoteDraft {
        &self.remote
    }
    pub fn raw_info(&self) -> &Value {
        &self.raw_info
    }
}
impl DraftStore {
    /// A human must review every version exposed by RemoteReview before choosing.
    /// Losing REVIEWED versions are retired using the existing local-settlement
    /// contract. Any new writer/revision/local/base change rejects the entire CAS.
    /// No request, enrollment, Send, Schedule, native projection or retry occurs.
    pub fn reconcile_remote_review(
        &mut self,
        expected: &RemoteReview,
        choice: &RemoteChoice,
    ) -> Result<RemoteSettlement, AppError> {
        let tx = self
            .0
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let scope = &expected.expected.scope;
        owner(&tx, &scope.account_id)?;
        let current = review(&tx, scope)?;
        if !expected.expected.matches(&current) {
            return Ok(RemoteSettlement::Stale);
        }
        let previous = current
            .stored
            .as_ref()
            .ok_or_else(|| invalid("reviewed owner disappeared"))?;
        let (local, desired) = match choice {
            RemoteChoice::Remote => {
                let projection = codec::decode(&scope.key, &expected.remote.snapshot)
                    .map_err(|_| invalid("remote projection unsupported"))?;
                (
                    LocalDraft {
                        composer: projection.composer().clone(),
                        attachments: vec![],
                    },
                    Some(expected.remote.snapshot.clone()),
                )
            }
            RemoteChoice::Local(choice) => {
                let Some(local) = current.chosen_local(choice).cloned() else {
                    return Ok(RemoteSettlement::Stale);
                };
                if !local.attachments.is_empty() {
                    return Ok(RemoteSettlement::Blocked);
                }
                let retained = previous
                    .state
                    .desired()
                    .and_then(|s| codec::decode(&scope.key, s).ok());
                let desired = if let Some(projection) = retained.filter(|p| {
                    p.composer().text == local.composer.text
                        && p.composer().reply_broadcast == local.composer.reply_broadcast
                }) {
                    Some(projection.raw_snapshot().clone())
                } else if local.composer.text.is_empty() {
                    None
                } else {
                    match codec::encode_composer(&scope.key, &local.composer) {
                        Ok(s) => Some(s),
                        Err(_) => return Ok(RemoteSettlement::Blocked),
                    }
                };
                (local, desired)
            }
        };
        let Ok(state) = resolve_reviewed_remote(&previous.state, &expected.remote, desired) else {
            return Ok(RemoteSettlement::Blocked);
        };
        let stored = Stored {
            revision: next_revision(Some(previous))?,
            state,
        };
        validate(scope, &stored.state)?;
        write_local(&tx, &scope.key, &local)?;
        write(&tx, &stored)?;
        let key = serde_json::to_string(&scope.key)?;
        for alternate in &current.alternates {
            tx.execute("INSERT INTO composer_draft_reviewed_writers(destination,writer_id,revision) VALUES(?1,?2,?3) ON CONFLICT(destination,writer_id) DO UPDATE SET revision=MAX(revision,excluded.revision)",params![&key,&alternate.writer_id,alternate.revision as i64])?;
        }
        tx.execute(
            "DELETE FROM composer_draft_blocked WHERE destination=?1",
            [&key],
        )?;
        tx.execute(
            "DELETE FROM composer_draft_alternates WHERE destination=?1",
            [&key],
        )?;
        tx.commit()?;
        Ok(RemoteSettlement::Applied { local, stored })
    }
}
#[cfg(test)]
mod tests;
