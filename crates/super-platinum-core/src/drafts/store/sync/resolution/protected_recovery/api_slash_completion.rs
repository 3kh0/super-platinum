//! Private terminal receipt/local settlement. No dispatch, unlock or remote clear.
use super::*;
use crate::slash_commands::{api::Reply as SlashReply, controller::Phase};
#[path = "api_slash_completion_storage.rs"]
mod receipts;

/// Receipt ownership, not execution authority. Only an admitted Claim can issue it.
pub(crate) struct Target {
    prepared: SlashRecord,
    scope: Scope,
    dispatch: SlashRecord,
    stream: Stream,
}
impl Target {
    pub(super) fn scope(&self) -> &Scope {
        &self.scope
    }
}
pub(super) fn owned_target(claim: Claim) -> Target {
    Target {
        prepared: claim.prepared,
        scope: claim.review.scope().clone(),
        dispatch: claim.record,
        stream: claim.stream,
    }
}
pub(super) fn target(claim: &Claim) -> Target {
    Target {
        prepared: claim.prepared.clone(),
        scope: claim.review.scope().clone(),
        dispatch: claim.record.clone(),
        stream: claim.stream.clone(),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) enum Effect {
    PendingUnknown,
    RetainedNotSent,
    RetainedRejected,
    RetainedKeepInput,
    ClearedExactAcknowledged,
    PreservedCurrent,
    RecoveredPreserved,
}
pub(crate) struct Settlement {
    fresh: Option<FreshClear>,
    terminal: SlashRecord,
    effect: Effect,
}
/// Not Clone/Deserialize. New attested commit only; never restored from Effect.
pub(crate) struct FreshClear {
    prepared: SlashRecord,
    terminal: SlashRecord,
}
impl FreshClear {
    pub(crate) fn matches(&self, prepared: &SlashRecord, terminal: &SlashRecord) -> bool {
        self.prepared == *prepared && self.terminal == *terminal
    }
}
impl Settlement {
    pub(crate) fn take_fresh_clear(&mut self) -> Option<FreshClear> {
        self.fresh.take()
    }

    pub(crate) fn record(&self) -> &SlashRecord {
        &self.terminal
    }
    pub(super) fn effect(&self) -> Effect {
        self.effect
    }
    pub(crate) fn pending(&self) -> bool {
        pending(&self.terminal)
    }
}
fn pending(record: &SlashRecord) -> bool {
    matches!(
        record.phase(),
        Phase::DispatchPossible | Phase::Unknown | Phase::Completed(SlashReply::Unknown { .. })
    )
}
fn result(receipt: receipts::Receipt) -> Settlement {
    Settlement {
        fresh: None,
        terminal: receipt.terminal,
        effect: receipt.effect,
    }
}
fn after_claim(binding: &bindings::Binding) -> Result<Review, Error> {
    let mut inventory = disk(binding.inventory.restore())?;
    let row = disk(wire::decode(
        &binding.scope,
        disk(sql_number(binding.after_revision))?,
        7,
        &binding.after_row,
    ))?;
    inventory.review.stored = Some(row.stored.clone());
    inventory.row = Some(Row::new(row));
    views::managed(inventory)
}
fn current(conn: &Connection, scope: &Scope) -> Result<Option<Review>, Error> {
    // An absent successor can retain the old receipt, but corruption cannot be
    // interpreted as a missing owner. No receipt clears an unrelated enrollment.
    let exists: bool = disk(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM composer_draft_sync WHERE destination=?1)",
        [disk(serde_json::to_string(&scope.key))?],
        |r| r.get(0),
    ))?;
    if exists {
        views::review(conn, scope).map(Some)
    } else {
        Ok(None)
    }
}
fn capture(review: Option<&Review>) -> Result<Option<views::InventoryRecord>, Error> {
    review
        .map(|review| {
            disk(views::InventoryRecord::capture(&views::local_from_review(
                review,
            )))
        })
        .transpose()
}
pub(super) fn install(store: &DraftStore, account: &str) -> Result<(), Error> {
    let tx = disk(adapter_atomic::begin(&store.0))?;
    disk(bindings::require(&tx, account))?;
    disk(receipts::install(&tx, account))?;
    disk(tx.commit())
}

/// Commit response, cooldown, local settlement and its receipt together.
/// A stale/live-revoked target can record history, but cannot clear local input.
pub(super) fn complete(
    store: &DraftStore,
    target: &Target,
    terminal: &SlashRecord,
) -> Result<Settlement, Error> {
    complete_inner(store, target, terminal, None)
}
/// Only a sealed Report-derived value can create durable provenance.
/// Failure retains the caller's value for flush-only retry, never another request.
pub(super) fn complete_attested(
    store: &DraftStore,
    target: &Target,
    outcome: &crate::slash_commands::controller::ManagedTerminal,
) -> Result<Settlement, Error> {
    if target.dispatch != *outcome.dispatch() {
        return Err(Error::Stale);
    }
    complete_inner(store, target, outcome.terminal(), Some(outcome))
}
fn complete_inner(
    store: &DraftStore,
    target: &Target,
    terminal: &SlashRecord,
    attestation: Option<&crate::slash_commands::controller::ManagedTerminal>,
) -> Result<Settlement, Error> {
    if !store.0.is_autocommit() {
        return Err(Error::Storage);
    }
    let tx = disk(adapter_atomic::begin(&store.0))?;
    disk(receipts::require(&tx, &target.scope.account_id))?;
    if attestation.is_some() {
        disk(history::require(&tx, &target.scope.account_id))?;
    }
    let (binding, binding_raw) = disk(bindings::exact(&tx, &target.scope, target.dispatch.id()))?;
    if binding.dispatch != target.dispatch {
        return Err(Error::Stale);
    }
    // Exact retry returns historical disposition; it never repeats the clear.
    if let Some(saved) = disk(receipts::read(
        &tx,
        &target.scope.account_id,
        target.dispatch.id(),
    ))? {
        if saved.binding != binding_raw || saved.terminal != *terminal {
            return Err(Error::Stale);
        }
        if let Some(outcome) = attestation {
            disk(archive_storage::attest(&tx, &target.scope, outcome, true))?;
        }
        disk(tx.commit())?;
        return Ok(result(saved));
    }
    // A first attested settlement must own the dispatch-to-terminal transition.
    // An independently persisted decoded terminal cannot be promoted to proof.
    if attestation.is_some() {
        let records = disk(slash_journal::protected_inventory(
            &tx,
            &target.scope.account_id,
        ))?;
        if records.iter().find(|r| r.id() == binding.intent) != Some(&binding.dispatch) {
            return Err(Error::Stale);
        }
    }
    let before = current(&tx, &target.scope)?;
    let mut effect = match terminal.phase() {
        Phase::NotSent => Effect::RetainedNotSent,
        Phase::Unknown | Phase::Completed(SlashReply::Unknown { .. }) => Effect::PendingUnknown,
        Phase::Completed(SlashReply::Rejected { .. }) => Effect::RetainedRejected,
        Phase::Completed(SlashReply::Acknowledged {
            keep_input: true, ..
        }) => Effect::RetainedKeepInput,
        Phase::Completed(SlashReply::Acknowledged {
            keep_input: false, ..
        }) => Effect::PreservedCurrent,
        _ => return Err(Error::Unsupported),
    };
    // Preserve the original Stream and require its complete captured durable
    // state, not just equal team or epoch. Missing/replaced authority is retain-only.
    let exists: bool = disk(tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM composer_draft_continuity WHERE team=?1)",
        [target.stream.team()],
        |r| r.get(0),
    ))?;
    let live = target.stream.is_live()
        && exists
        && disk(storage::team(&tx, target.stream.team()))? == binding.stream;
    let eligible = if let Some(view) = before.as_ref() {
        let stored: Option<String> = disk(
            tx.query_row(
                "SELECT content FROM composer_draft_writer_lineage WHERE writer_id=?1",
                [&binding.writer],
                |r| r.get(0),
            )
            .optional(),
        )?;
        !view.local().empty()
            && view.same(&after_claim(&binding)?)
            && stored.as_deref() == Some(binding.writer_receipt.as_str())
            && disk(writers::slash_receipt_matches(
                &binding.writer_receipt,
                &binding.writer,
                binding.serial,
                &views::managed(disk(binding.inventory.restore())?)?,
            ))?
    } else {
        false
    };
    disk(slash_journal::protected_complete(
        &tx,
        &target.scope.account_id,
        &binding.dispatch,
        terminal,
    ))?;
    if effect == Effect::PreservedCurrent && live && eligible {
        let before = before.as_ref().ok_or(Error::Stale)?;
        if let Some(state) = before.row().state().clear_acknowledged_slash_input() {
            let row = disk(successor(&before.row().inner, state))?;
            disk(write_local(&tx, &target.scope.key, &LocalDraft::default()))?;
            disk(storage::write_row(&tx, &row))?;
            effect = Effect::ClearedExactAcknowledged;
        }
    }
    let after = current(&tx, &target.scope)?;
    let receipt = receipts::Receipt {
        version: 1,
        scope: target.scope.clone(),
        intent: binding.intent.clone(),
        binding: binding_raw,
        terminal: terminal.clone(),
        effect,
        before: capture(before.as_ref())?,
        after: capture(after.as_ref())?,
    };
    disk(receipts::insert(&tx, &receipt))?;
    if let Some(outcome) = attestation {
        disk(archive_storage::attest(&tx, &target.scope, outcome, false))?;
    }
    if effect == Effect::ClearedExactAcknowledged && !target.stream.is_live() {
        return Err(Error::Stale);
    }
    disk(tx.commit())?;
    let mut settled = result(receipt);
    if attestation.is_some() && effect == Effect::ClearedExactAcknowledged {
        settled.fresh = Some(FreshClear {
            prepared: target.prepared.clone(),
            terminal: terminal.clone(),
        });
    }
    Ok(settled)
}

/// Read-only recovery inventory. It cannot become a Claim/Admission/Permit.
pub(super) struct Recovery {
    scope: Scope,
    binding: String,
    record: SlashRecord,
    current: Option<Review>,
    settlement: Option<Effect>,
}
impl Recovery {
    pub(crate) fn record(&self) -> &SlashRecord {
        &self.record
    }
    pub(super) fn current(&self) -> Option<&Review> {
        self.current.as_ref()
    }
    pub(super) fn effect(&self) -> Option<Effect> {
        self.settlement
    }
    pub(crate) fn pending(&self) -> bool {
        pending(&self.record)
    }
    pub(super) fn original(&self) -> &serde_json::Value {
        self.record.original()
    }
}
pub(super) fn recover(store: &DraftStore, scope: &Scope, intent: &str) -> Result<Recovery, Error> {
    let tx = disk(adapter_atomic::begin(&store.0))?;
    disk(receipts::require(&tx, &scope.account_id))?;
    let (_, binding) = disk(bindings::exact(&tx, scope, intent))?;
    let record = disk(slash_journal::protected_inventory(&tx, &scope.account_id))?
        .into_iter()
        .find(|record| record.id() == intent)
        .ok_or(Error::Stale)?;
    let settlement =
        disk(receipts::read(&tx, &scope.account_id, intent))?.map(|saved| saved.effect);
    let view = current(&tx, scope)?;
    disk(tx.commit())?;
    Ok(Recovery {
        scope: scope.clone(),
        binding,
        record,
        current: view,
        settlement,
    })
}
/// Explicit settlement of already-persisted terminal evidence after restart.
/// Always preserves canonical input, even a previously acknowledged exact input.
/// DispatchPossible cannot be fabricated into Unknown or completion.
pub(super) fn preserve_recovered(
    store: &DraftStore,
    expected: &Recovery,
) -> Result<Settlement, Error> {
    if !store.0.is_autocommit() {
        return Err(Error::Storage);
    }
    let tx = disk(adapter_atomic::begin(&store.0))?;
    disk(receipts::require(&tx, &expected.scope.account_id))?;
    let (binding, raw) = disk(bindings::exact(&tx, &expected.scope, expected.record.id()))?;
    if raw != expected.binding
        || matches!(
            expected.record.phase(),
            Phase::Prepared | Phase::DispatchPossible
        )
    {
        return Err(Error::Pending);
    }
    let current_record = disk(slash_journal::protected_inventory(
        &tx,
        &expected.scope.account_id,
    ))?
    .into_iter()
    .find(|record| record.id() == expected.record.id())
    .ok_or(Error::Stale)?;
    if current_record != expected.record {
        return Err(Error::Stale);
    }
    if let Some(saved) = disk(receipts::read(
        &tx,
        &expected.scope.account_id,
        expected.record.id(),
    ))? {
        disk(tx.commit())?;
        return Ok(result(saved));
    }
    let view = current(&tx, &expected.scope)?;
    if !same_view(view.as_ref(), expected.current.as_ref()) {
        return Err(Error::Stale);
    }
    let effect = if pending(&expected.record) {
        Effect::PendingUnknown
    } else {
        Effect::RecoveredPreserved
    };
    let receipt = receipts::Receipt {
        version: 1,
        scope: expected.scope.clone(),
        intent: binding.intent,
        binding: raw,
        terminal: expected.record.clone(),
        effect,
        before: capture(view.as_ref())?,
        after: capture(view.as_ref())?,
    };
    disk(receipts::insert(&tx, &receipt))?;
    disk(tx.commit())?;
    Ok(result(receipt))
}
pub(super) fn audit(conn: &Connection, account: &str) -> Result<(), AppError> {
    receipts::audit(conn, account)
}
pub(super) fn settlement_raw(
    conn: &Connection,
    scope: &Scope,
    id: &str,
) -> Result<(SlashRecord, String), AppError> {
    let receipt = receipts::read(conn, &scope.account_id, id)?
        .ok_or_else(|| invalid("missing exact settlement"))?;
    if receipt.scope != *scope {
        return Err(invalid("settlement scope changed"));
    }
    let raw = conn.query_row(
        "SELECT content FROM composer_draft_slash_completions WHERE intent=?1",
        [id],
        |r| r.get(0),
    )?;
    Ok((receipt.terminal, raw))
}
fn same_view(a: Option<&Review>, b: Option<&Review>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => a.same(b),
        (None, None) => true,
        _ => false,
    }
}
