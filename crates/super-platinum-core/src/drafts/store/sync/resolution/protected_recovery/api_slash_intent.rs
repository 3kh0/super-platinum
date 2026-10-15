//! Inert private operation. No Client method, queued Action or HTTP Admission.
//! The caller must be the existing worker after its final pending-input fence.
use super::*;
use crate::slash_commands::{controller::Record as SlashRecord, journal as slash_journal};
#[path = "api_slash_archive.rs"]
mod archive;
#[path = "api_slash_archive_storage.rs"]
mod archive_storage;
#[path = "api_slash_storage.rs"]
mod bindings;
#[path = "api_slash_completion.rs"]
mod completion;
#[path = "api_slash_history.rs"]
mod history;
#[path = "api_slash_remote.rs"]
mod remote;
pub(super) fn validate_remote_read(
    conn: &Connection,
    id: &str,
    record: &ReadRecord,
) -> Result<(), AppError> {
    remote::validate_retained_read(conn, id, record)
}

/// Storage ownership only. Never Clone/Deserialize, never a transport Permit.
pub(crate) struct Claim {
    prepared: SlashRecord,
    record: SlashRecord,
    review: Review,
    stream: Stream,
}
impl Claim {
    pub(crate) fn into_target(self) -> completion::Target {
        completion::owned_target(self)
    }
    pub(crate) fn record(&self) -> &SlashRecord {
        &self.record
    }
    pub(super) fn review(&self) -> &Review {
        &self.review
    }
    pub(crate) fn stream_live(&self) -> bool {
        self.stream.is_live()
    }
}

fn disk<T>(result: Result<T, impl std::fmt::Display>) -> Result<T, Error> {
    result.map_err(|_| Error::Storage)
}

/// This is an internal storage envelope, not Slack's Delta/blocks wire format.
/// Its exact local caret/marks and desired raw rich snapshot must both match.
/// Native classification/argument conversion is still an unavailable boundary.
fn original(review: &Review) -> serde_json::Value {
    serde_json::json!({"format":1,"local":review.local(),
        "desired":review.row().state().desired()})
}

pub(super) fn install(store: &DraftStore, account: &str) -> Result<(), Error> {
    let tx = disk(adapter_atomic::begin(&store.0))?;
    disk(adapter_schema::require(&tx))?;
    disk(owner(&tx, account))?;
    disk(bindings::install(&tx, account))?;
    disk(tx.commit())
}

pub(super) fn guard(conn: &Connection, scope: &Scope) -> Result<(), AppError> {
    bindings::guard(conn, scope)
}

/// One commit joins the protected ownership fence with the exact journal CAS.
/// No separate consume, clear, claim_dispatch, or journal::compare commit.
pub(super) fn claim(
    store: &DraftStore,
    stream: &Stream,
    expected: &Review,
    writer: &Writer,
    serial: u64,
    prior: &SlashRecord,
    next: &SlashRecord,
) -> Result<Claim, Error> {
    // Never return authority from a savepoint while an outer transaction is open.
    if !store.0.is_autocommit() {
        return Err(Error::Storage);
    }
    let _live = stream.0.live_guard().map_err(|_| Error::Gap)?;
    let tx = disk(adapter_atomic::begin(&store.0))?;
    let current = views::exact(&tx, expected)?;
    let scope = current.scope();
    disk(bindings::require(&tx, &scope.account_id))?;
    disk(facade::admitted(&tx, &stream.0, &current.inner))?;
    disk(facade::original_pair(&current.inner))?;
    let state = current.row().state();
    if stream.account() != scope.account_id
        || stream.team() != scope.key.team
        || state.pending().is_some()
        || state.consumption().is_some()
        || state.blocked_observation().is_some()
        || !matches!(state.workflow(), Workflow::Idle)
        || !current.local().attachments.is_empty()
        || writer.scope() != scope
        || writer.enrollment() != current.row().lineage().enrollment()
        || serial == 0
        || prior.owner().account != scope.account_id
        || prior.owner().team != scope.key.team
        || prior.owner().channel != scope.key.channel
        || prior.owner().thread != scope.key.thread
        || prior.original() != &original(&current)
        || prior.protected_input_revision() != current.row().revision()
    {
        return Err(Error::Stale);
    }

    let receipt = writers::matching(&tx, writer)?;
    if receipt.processed != serial {
        return Err(Error::Stale);
    }
    let writers::Retry::Applied(accepted) = writers::retry(
        &tx,
        &receipt,
        &writer.writer,
        serial,
        current.local(),
        None,
        None,
    )?
    else {
        return Err(Error::Stale);
    };
    if !views::managed(accepted)?.same(&current) {
        return Err(Error::Stale);
    }

    let inventory = disk(views::InventoryRecord::capture(&views::local_from_review(
        &current,
    )))?;
    let before = current.row().inner.clone();
    let after = disk(successor(&before, state.clone()))?;
    let team = disk(storage::team(&tx, stream.team()))?;
    let binding = bindings::Binding {
        version: 1,
        scope: scope.clone(),
        intent: next.id().into(),
        enrollment: before.ownership.enrollment_id.clone(),
        inventory,
        writer: writer.id().into(),
        serial,
        writer_receipt: disk(serde_json::to_string(&receipt))?,
        stream: team,
        before_revision: before.stored.revision,
        after_revision: after.stored.revision,
        after_row: disk(wire::encode(&after))?,
        dispatch: next.clone(),
    };
    // A RateLimited result may have conservatively rebased the monotonic gate.
    // Commit that alone; no row or binding has been changed at this point.
    match slash_journal::protected_dispatch(&tx, &scope.account_id, prior, next) {
        Err(crate::slash_commands::Failure::RateLimited) => {
            disk(tx.commit())?;
            return Err(Error::Pending);
        }
        result => disk(result)?,
    }
    disk(bindings::insert(&tx, &binding))?;
    disk(storage::write_row(&tx, &after))?;
    let review = views::review(&tx, scope)?;
    disk(tx.commit())?;
    // Revocation never undoes a durable DispatchPossible; recovery stays inert.
    if !stream.is_live() {
        return Err(Error::Gap);
    }
    Ok(Claim {
        prepared: prior.clone(),
        record: next.clone(),
        review,
        stream: stream.clone(),
    })
}

#[cfg(test)]
#[path = "api_slash_tests.rs"]
mod tests;

// Private test-build-only schema/audit bridge for inactive store inspection.
// Operates on a trusted in-memory reference or shadow, never the inspected file.
#[cfg(any(test, feature = "protected-test-support"))]
pub(super) fn upgrade_reference(
    store: &mut DraftStore,
    account: &str,
    version: i64,
    completed: bool,
) -> Result<(), AppError> {
    slash_journal::install(&mut store.0, account)
        .map_err(|_| invalid("slash reference install"))?;
    if version == 0 {
        return Ok(());
    }
    install(store, account).map_err(|_| invalid("binding reference install"))?;
    if completed {
        completion::install(store, account).map_err(|_| invalid("completion reference install"))?;
    }
    if version >= 2 {
        history::migrate(store, account).map_err(|_| invalid("history reference install"))?;
    }
    if version >= 3 {
        remote::migrate(store, account)?;
    }
    Ok(())
}
#[cfg(any(test, feature = "protected-test-support"))]
pub(super) fn upgrade_audit(
    conn: &Connection,
    account: &str,
    binding: bool,
    completed: bool,
) -> Result<(), AppError> {
    slash_journal::protected_inventory(conn, account)
        .map_err(|_| invalid("invalid slash journal"))?;
    if binding {
        bindings::require(conn, account)?;
        bindings::all(conn, account)?;
        if completed {
            completion::audit(conn, account)?;
        }
        if history::v2(conn)? {
            archive_storage::audit(conn, account)?;
        }
        if remote::enabled(conn)? {
            remote::audit(conn, account)?;
        }
    }
    Ok(())
}

#[path = "api_slash_runtime.rs"]
pub(crate) mod runtime;
pub(crate) use completion::{FreshClear, Settlement as SlashSettlement, Target as SlashTarget};
