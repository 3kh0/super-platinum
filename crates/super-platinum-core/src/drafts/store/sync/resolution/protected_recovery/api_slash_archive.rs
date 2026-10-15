//! Private local-only archival; no UI consent factory, worker action or dispatch.
use super::*;
use crate::slash_commands::{api::Reply as CommandReply, controller::Phase};
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) enum Reason {
    LocalAcknowledged,
    ProvenNotSent,
    RejectedPreserveRetire,
}
#[derive(Debug)]
pub(super) enum ArchiveError {
    Storage,
    Stale,
    Pending,
    NeedsRemoteReview,
    ChoiceRequired,
}
fn db<T>(r: Result<T, impl std::fmt::Display>) -> Result<T, ArchiveError> {
    r.map_err(|_| ArchiveError::Storage)
}
/// Exact current state plus immutable historical command. Never Deserialize.
pub(super) struct ArchiveReview {
    scope: Scope,
    intent: String,
    binding: String,
    settlement: String,
    attestation: String,
    current: Review,
    writer: String,
    serial: u64,
    writer_receipt: String,
    stream: Stream,
    team: Team,
}
/// Unavailable to production until a native exact-review action can mint it.
/// Never Clone/Deserialize; an arbitrary enum is not an explicit rejection choice.
pub(super) struct RejectionChoice {
    review: ArchiveReview,
    id: String,
}
pub(super) struct Archived {
    record: archive_storage::ArchivedRecord,
}
impl Archived {
    pub(super) fn original_intent(&self) -> &str {
        &self.record.intent
    }
    pub(super) fn reason(&self) -> Reason {
        self.record.reason
    }
    /// Historical successor only. A fresh current Editor adoption is still required.
    pub(super) fn historical_review(&self) -> Result<Review, ArchiveError> {
        views::managed(db(self.record.after.restore())?).map_err(|_| ArchiveError::Storage)
    }
}
pub(super) fn pair(view: &Review) -> Result<(), AppError> {
    if view.row().state().desired().is_some() {
        facade::original_pair(&view.inner)
    } else if same_local(view.local(), &LocalDraft::default()) {
        Ok(())
    } else {
        Err(invalid(
            "empty desired does not match exact empty canonical input",
        ))
    }
}
pub(super) fn local_only(view: &Review) -> Result<(), AppError> {
    let state = view.row().state();
    if state.has_remote_identity_history()
        || state.pending().is_some()
        || view.row().lineage().basis() != Basis::Local
        || !matches!(
            view.row().inner.ownership.origin,
            Origin::LocalIssued { .. }
        )
        || view.blocked().is_some()
        || !view.alternates().is_empty()
        || view.uncommitted().is_some()
    {
        return Err(invalid("local-only archival unavailable"));
    }
    pair(view)
}
pub(super) fn inspect(
    store: &DraftStore,
    stream: &Stream,
    expected: &Review,
    writer: &Writer,
    serial: u64,
    intent: &str,
) -> Result<ArchiveReview, ArchiveError> {
    let tx = db(adapter_atomic::begin(&store.0))?;
    db(history::require(&tx, &expected.scope().account_id))?;
    db(archive_storage::audit(&tx, &expected.scope().account_id))?;
    let current = views::exact(&tx, expected).map_err(|_| ArchiveError::Stale)?;
    if current.row().state().has_remote_identity_history() {
        return Err(ArchiveError::NeedsRemoteReview);
    }
    db(local_only(&current))?;
    let (binding, raw) = db(bindings::exact(&tx, current.scope(), intent))?;
    if binding.enrollment != current.row().lineage().enrollment()
        || !db(history::active(&tx, current.scope(), intent, &raw))?
    {
        return Err(ArchiveError::Stale);
    }
    if stream.account() != current.scope().account_id
        || stream.team() != current.scope().key.team
        || !stream.is_live()
    {
        return Err(ArchiveError::Stale);
    }
    let team = db(storage::team(&tx, stream.team()))?;
    if !stream.0.matches(&team)
        || team.pending.is_some()
        || team.epoch != current.row().lineage().verified_epoch()
        || db(readback::unresolved(&tx, stream.team()))?
        || db(recovery::unresolved(&tx, stream.team()))?
    {
        return Err(ArchiveError::Pending);
    }
    db(inventory::local_identity(&tx, &current.inner))?;
    if writer.scope() != current.scope() || writer.enrollment() != binding.enrollment || serial == 0
    {
        return Err(ArchiveError::Stale);
    }
    let receipt = writers::matching(&tx, writer).map_err(|_| ArchiveError::Stale)?;
    let writer_receipt = db(serde_json::to_string(&receipt))?;
    if !db(writers::slash_receipt_matches(
        &writer_receipt,
        writer.id(),
        serial,
        &current,
    ))? {
        return Err(ArchiveError::Stale);
    }
    let (_, settlement) = db(completion::settlement_raw(&tx, current.scope(), intent))?;
    let attestation = db(archive_storage::attestation_raw(
        &tx,
        &current.scope().account_id,
        intent,
    ))?
    .ok_or(ArchiveError::Pending)?;
    let review = ArchiveReview {
        scope: current.scope().clone(),
        intent: intent.into(),
        binding: raw,
        settlement,
        attestation,
        current,
        writer: writer.id().into(),
        serial,
        writer_receipt,
        stream: stream.clone(),
        team,
    };
    db(tx.commit())?;
    Ok(review)
}
pub(super) fn archive_known(
    store: &DraftStore,
    review: &ArchiveReview,
) -> Result<Archived, ArchiveError> {
    archive(store, review, None)
}
pub(super) fn archive_rejection(
    store: &DraftStore,
    choice: &RejectionChoice,
) -> Result<Archived, ArchiveError> {
    archive(store, &choice.review, Some(&choice.id))
}
#[cfg(test)]
pub(super) fn isolated_rejection_choice(review: ArchiveReview) -> RejectionChoice {
    RejectionChoice {
        review,
        id: uuid::Uuid::new_v4().to_string(),
    }
}
fn archive(
    store: &DraftStore,
    review: &ArchiveReview,
    choice: Option<&str>,
) -> Result<Archived, ArchiveError> {
    if !store.0.is_autocommit() {
        return Err(ArchiveError::Storage);
    }
    let tx = db(adapter_atomic::begin(&store.0))?;
    db(history::require(&tx, &review.scope.account_id))?;
    db(archive_storage::audit(&tx, &review.scope.account_id))?;
    if let Some(raw) = db(history::archived(&tx, &review.intent))? {
        let saved: archive_storage::ArchivedRecord = db(wire::strict(&raw))?;
        if saved.binding != review.binding
            || saved.settlement != review.settlement
            || saved.attestation != review.attestation
            || saved.writer_receipt != review.writer_receipt
            || saved.serial != review.serial
            || saved.writer != review.writer
            || saved.choice_id.as_deref() != choice
            || !views::managed(db(saved.before.restore())?)
                .map_err(|_| ArchiveError::Storage)?
                .same(&review.current)
        {
            return Err(ArchiveError::Stale);
        }
        db(tx.commit())?;
        return Ok(Archived { record: saved });
    }
    if !db(history::active(
        &tx,
        &review.scope,
        &review.intent,
        &review.binding,
    ))? {
        return Err(ArchiveError::Stale);
    }
    let current = views::exact(&tx, &review.current).map_err(|_| ArchiveError::Stale)?;
    if current.row().state().has_remote_identity_history() {
        return Err(ArchiveError::NeedsRemoteReview);
    }
    db(local_only(&current))?;
    if !review.stream.is_live() || db(storage::team(&tx, review.stream.team()))? != review.team {
        return Err(ArchiveError::Stale);
    }
    if db(readback::unresolved(&tx, review.stream.team()))?
        || db(recovery::unresolved(&tx, review.stream.team()))?
    {
        return Err(ArchiveError::Pending);
    }
    db(inventory::local_identity(&tx, &current.inner))?;
    let writer: Option<String> = db(tx
        .query_row(
            "SELECT content FROM composer_draft_writer_lineage WHERE writer_id=?1",
            [&review.writer],
            |r| r.get(0),
        )
        .optional())?;
    if writer.as_deref() != Some(review.writer_receipt.as_str()) {
        return Err(ArchiveError::Stale);
    }
    let (terminal, settlement) = db(completion::settlement_raw(
        &tx,
        &review.scope,
        &review.intent,
    ))?;
    if settlement != review.settlement
        || db(archive_storage::attestation_raw(
            &tx,
            &review.scope.account_id,
            &review.intent,
        ))?
        .as_deref()
            != Some(review.attestation.as_str())
    {
        return Err(ArchiveError::Stale);
    }
    let evidence = db(archive_storage::proof(&review.attestation))?;
    let reason = match terminal.phase() {
        Phase::NotSent if evidence.before_execute() && choice.is_none() => Reason::ProvenNotSent,
        Phase::Completed(CommandReply::Acknowledged { .. }) if choice.is_none() => {
            Reason::LocalAcknowledged
        }
        Phase::Completed(CommandReply::Rejected { .. }) if choice.is_some_and(valid_uuid) => {
            Reason::RejectedPreserveRetire
        }
        Phase::Completed(CommandReply::Rejected { .. }) => {
            return Err(ArchiveError::ChoiceRequired);
        }
        _ => return Err(ArchiveError::Pending),
    };
    let retired = db(slash_journal::protected_retire(
        &tx,
        &review.scope.account_id,
        &terminal,
    ))?;
    let row = db(successor(
        &current.row().inner,
        current.row().state().clone(),
    ))?;
    db(storage::write_row(&tx, &row))?;
    let after = views::review(&tx, &review.scope).map_err(|_| ArchiveError::Storage)?;
    let record = archive_storage::ArchivedRecord {
        version: 1,
        scope: review.scope.clone(),
        intent: review.intent.clone(),
        binding: review.binding.clone(),
        settlement: review.settlement.clone(),
        attestation: review.attestation.clone(),
        reason,
        choice_id: choice.map(str::to_owned),
        writer: review.writer.clone(),
        serial: review.serial,
        writer_receipt: review.writer_receipt.clone(),
        team: review.team.clone(),
        before: db(views::InventoryRecord::capture(&views::local_from_review(
            &current,
        )))?,
        after: db(views::InventoryRecord::capture(&views::local_from_review(
            &after,
        )))?,
        retired,
    };
    db(archive_storage::move_active(&tx, &record))?;
    if !review.stream.is_live() {
        return Err(ArchiveError::Stale);
    }
    db(tx.commit())?;
    Ok(Archived { record })
}
