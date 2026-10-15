//! Private remote command recovery. No Client/Action, native consent or HTTP permit.
use super::super::super::Basis as StoredBasis;
use super::*;
use crate::drafts::sync::{RemoteDraft, reconciliation::resolve_reviewed_remote};
use crate::slash_commands::{api::Reply as CommandReply, controller::Phase as CommandPhase};
#[path = "api_slash_remote_cleanup.rs"]
mod cleanup;
#[path = "api_slash_remote_continuity.rs"]
mod continuity;
#[path = "api_slash_remote_storage.rs"]
mod disk_store;
#[path = "api_slash_remote_archive.rs"]
mod finish;
#[path = "api_slash_remote_model.rs"]
pub(super) mod model;
#[path = "api_slash_remote_read.rs"]
mod reads;
pub(super) use cleanup::{claim_cleanup, reserve_cleanup, retain_cleanup};
pub(super) use continuity::validate_retained as validate_retained_read;
pub(super) use disk_store::{audit, audit_remote, enabled, migrate};
pub(super) use finish::{archive_cleanup, preserve};
#[cfg(not(test))]
use model::{Action, CleanupKind, ReadKind};
#[cfg(test)]
pub(super) use model::{Action, CleanupKind, ReadKind, choice, cleanup_report, read_report};
use model::{
    Archive, Choice, Cleanup, CleanupClaim, CleanupEvidence, CleanupReport, CleanupTicket,
    Decision, Fence, Finished, Job, ReadClaim, ReadEvidence, ReadReport, ReadTicket, Stage,
};
pub(super) use reads::{claim_read, reserve, retain_read};

fn bad() -> AppError {
    invalid("remote command ownership/proof changed")
}
fn capture(view: &Review) -> Result<views::InventoryRecord, AppError> {
    views::InventoryRecord::capture(&views::local_from_review(view))
}
fn restore(value: &views::InventoryRecord) -> Result<Review, AppError> {
    views::managed(value.restore()?).map_err(|_| bad())
}
fn exact(conn: &Connection, view: &Review) -> Result<Review, AppError> {
    views::exact(conn, view).map_err(|_| bad())
}
fn full_writer(
    conn: &Connection,
    view: &Review,
    writer: &Writer,
    serial: u64,
) -> Result<String, AppError> {
    if writer.scope() != view.scope() || writer.enrollment() != view.row().lineage().enrollment() {
        return Err(bad());
    }
    let record = writers::matching(conn, writer).map_err(|_| bad())?;
    let raw = serde_json::to_string(&record)?;
    if !writers::slash_receipt_matches(&raw, writer.id(), serial, view)? {
        return Err(bad());
    }
    Ok(raw)
}
fn clean(view: &Review) -> Result<(), AppError> {
    if view.blocked().is_some()
        || !view.alternates().is_empty()
        || view.uncommitted().is_some()
        || view.row().state().consumption().is_some()
        || view.row().state().blocked_observation().is_some()
    {
        return Err(bad());
    }
    archive::pair(view)
}
fn remote_idle(view: &Review) -> Result<&RemoteDraft, AppError> {
    clean(view)?;
    if !matches!(view.row().state().workflow(), Workflow::Idle) {
        return Err(bad());
    }
    let base = view.row().state().acknowledged().ok_or_else(bad)?;
    if !base.snapshot.file_ids.is_empty() || base.snapshot.destinations.len() != 1 {
        return Err(bad());
    }
    codec::decode(&view.scope().key, &base.snapshot).map_err(|_| bad())?;
    Ok(base)
}
fn team(conn: &Connection, stream: &Stream, scope: &Scope) -> Result<Team, AppError> {
    if !stream.is_live() || stream.account() != scope.account_id || stream.team() != scope.key.team
    {
        return Err(bad());
    }
    let team = storage::team(conn, stream.team())?;
    if !stream.0.matches(&team) {
        return Err(bad());
    }
    Ok(team)
}
fn known(
    conn: &Connection,
    scope: &Scope,
    intent: &str,
) -> Result<(bindings::Binding, String, String, String), AppError> {
    let (binding, raw) = bindings::exact(conn, scope, intent)?;
    if !history::active(conn, scope, intent, &raw)? {
        return Err(bad());
    }
    let (terminal, settlement) = completion::settlement_raw(conn, scope, intent)?;
    let attestation =
        archive_storage::attestation_raw(conn, &scope.account_id, intent)?.ok_or_else(bad)?;
    let proof = archive_storage::proof(&attestation)?;
    if !matches!(
        terminal.phase(),
        CommandPhase::Completed(CommandReply::Acknowledged { .. } | CommandReply::Rejected { .. })
    ) && !(matches!(terminal.phase(), CommandPhase::NotSent) && proof.before_execute())
    {
        return Err(bad());
    }
    // Other unresolved commands in the same authenticated journal owner block progress.
    if slash_journal::protected_inventory(conn, &scope.account_id)
        .map_err(|_| bad())?
        .iter()
        .any(|r| {
            r.id() != intent
                && r.owner().team == terminal.owner().team
                && r.owner().user == terminal.owner().user
                && matches!(
                    r.phase(),
                    CommandPhase::DispatchPossible
                        | CommandPhase::Unknown
                        | CommandPhase::Completed(CommandReply::Unknown { .. })
                )
        })
    {
        return Err(bad());
    }
    Ok((binding, raw, settlement, attestation))
}
fn writer_unchanged(conn: &Connection, fence: &Fence) -> Result<(), AppError> {
    let raw: Option<String> = conn
        .query_row(
            "SELECT content FROM composer_draft_writer_lineage WHERE writer_id=?1",
            [&fence.writer],
            |r| r.get(0),
        )
        .optional()?;
    if raw.as_deref() != Some(&fence.receipt) {
        return Err(bad());
    }
    Ok(())
}
fn open_exact(conn: &Connection, job: &Job, stream: &Stream) -> Result<Review, AppError> {
    let view = restore(&job.original.view)?;
    let current = exact(conn, &view)?;
    writer_unchanged(conn, &job.original)?;
    let t = live_settled(conn, job, stream)?;
    if t.pending.is_some()
        || t.sequence != job.pending.sequence
        || readback::unresolved(conn, stream.team())?
        || recovery::unresolved(conn, stream.team())?
    {
        return Err(bad());
    }
    if !history::active(conn, &job.scope, &job.intent, &job.binding)? {
        return Err(bad());
    }
    inventory::acknowledged(conn, &current.inner)?;
    Ok(current)
}
fn positive(job: &Job) -> Result<RemoteDraft, AppError> {
    let read = job.read.as_ref().ok_or_else(bad)?;
    if read.kind != ReadKind::Response || read.user != job.user {
        return Err(bad());
    }
    let raw = read.body.as_ref().ok_or_else(bad)?;
    let value =
        crate::slash_commands::codec::decode(raw.as_bytes(), 2 * 1024 * 1024).map_err(|_| bad())?;
    let view = restore(&job.original.view)?;
    let result = RemoteReview::prepare(view.inner.review.clone(), value, &read.user)?;
    let remote = result.remote();
    if remote.id != job.base.id || remote.client_msg_id != job.base.client_msg_id {
        return Err(bad());
    }
    resolve_reviewed_remote(
        view.row().state(),
        remote,
        view.row().state().desired().cloned(),
    )
    .map_err(|_| bad())?;
    Ok(remote.clone())
}

fn same_successor(before: &Review, after: &Review, row: &ProtectedRow) -> Result<bool, AppError> {
    let mut expected = views::local_from_review(before);
    expected.review.stored = Some(row.stored.clone());
    expected.row = Some(Row::new(ProtectedRow {
        stored: row.stored.clone(),
        ownership: row.ownership.clone(),
    }));
    Ok(views::managed(expected).map_err(|_| bad())?.same(after))
}

fn begin(store: &DraftStore) -> Result<adapter_atomic::Atomic<'_>, AppError> {
    if !store.0.is_autocommit() {
        return Err(bad());
    }
    adapter_atomic::begin(&store.0)
}
fn settled_team(job: &Job) -> Team {
    let mut team = job.team.clone();
    team.pending = None;
    team
}
fn live_settled(conn: &Connection, job: &Job, stream: &Stream) -> Result<Team, AppError> {
    let t = team(conn, stream, &job.scope)?;
    if t != settled_team(job) || recovery::deadline(conn, stream.team())? != job.known_deadline {
        return Err(bad());
    }
    Ok(t)
}

// Archival only: a new lease may retain an already-applied cleanup. This does
// not authorize a read, Delete, reducer retry, or verification of a remote base.
fn cleanup_archive_team(job: &Job, t: &Team) -> Result<(), AppError> {
    sql_number(t.epoch)?;
    sql_number(t.sequence)?;
    sql_number(t.read_retry_at_ms)?;
    if t.mode != Mode::Open
        || t.pending.is_some()
        || t.epoch < job.team.epoch
        || t.sequence < job.pending.sequence
        || t.read_retry_at_ms < job.team.read_retry_at_ms
        || t.lease_id.as_deref().is_none_or(|id| !valid_uuid(id))
        || t.last_interrupted_id
            .as_deref()
            .is_some_and(|id| !valid_uuid(id))
        || (t.epoch == job.team.epoch && t.lease_id != job.team.lease_id)
    {
        return Err(bad());
    }
    Ok(())
}

#[cfg(test)]
pub(super) fn duplicate_read_ticket(t: &model::ReadTicket) -> model::ReadTicket {
    model::ReadTicket {
        raw: t.raw.clone(),
        stream: t.stream.clone(),
    }
}
#[cfg(test)]
pub(super) fn duplicate_cleanup_ticket(t: &model::CleanupTicket) -> model::CleanupTicket {
    model::CleanupTicket {
        raw: t.raw.clone(),
        stream: t.stream.clone(),
    }
}
#[cfg(test)]
pub(super) fn mutate_reserved_deadline(store: &DraftStore, raw: &str) -> Result<(), AppError> {
    let tx = begin(store)?;
    let mut j = disk_store::exact(&tx, raw)?;
    j.known_deadline = j.known_deadline.checked_add(1).ok_or_else(bad)?;
    j.stage = Stage::ReadDispatched;
    disk_store::cas(&tx, raw, &mut j)?;
    tx.commit()
}
