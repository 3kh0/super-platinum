//! Protected composer batch custody preparation. No Client command, configured
//! migration activation, HTTP permit, native clear, or arbitrary SQL closure.
//! The future owned worker must call this after its final pending-input fence.
use super::*;
#[path = "api_hosted_files_storage.rs"]
mod journal;
#[path = "api_hosted_files_model.rs"]
mod model;
pub use journal::{Record, Returned};
pub use model::Input;
#[path = "api_hosted_selection.rs"]
mod selection;
pub use selection::Selection;
#[path = "api_hosted_runtime.rs"]
pub(crate) mod runtime;

/// Non-cloneable process custody; loading a record never reconstructs a Claim.
pub(crate) struct Claim {
    record: Record,
}
impl Claim {
    pub(crate) fn record(&self) -> &Record {
        &self.record
    }
}
fn disk<T>(v: Result<T, impl std::fmt::Display>) -> Result<T, Error> {
    v.map_err(|_| Error::Storage)
}

/// Only the trusted owned initialization transaction may install this schema.
/// Deliberately not called by ordinary load/open or the legacy Client.
pub(crate) fn initialize_transaction(conn: &Connection, account: &str) -> Result<(), AppError> {
    if conn.is_autocommit() {
        return Err(invalid(
            "hosted-file initialization requires owned transaction",
        ));
    }
    adapter_schema::require(conn)?;
    owner(conn, account)?;
    journal::install(conn, account)?;
    editor::install(conn, account)?;
    cleanup::install(conn, account)
}
pub(super) fn guard(conn: &Connection, scope: &Scope) -> Result<(), AppError> {
    journal::guard(conn, scope)?;
    editor::guard(conn, scope)
}

/// One commit binds original full protected inventory, writer receipt, stream,
/// caption and EVERY ordered source to one possible network attempt. Repeating
/// this call cannot grant another attempt, including after an unobserved reply.
#[cfg(test)]
pub(crate) fn claim(
    store: &DraftStore,
    stream: &Stream,
    expected: &Review,
    writer: &Writer,
    serial: u64,
    input: Input,
) -> Result<Claim, Error> {
    claim_inner(store, stream, expected, writer, serial, input, false)
}
pub(crate) fn claim_saved(store: &DraftStore, input: &runtime::Prepared) -> Result<Claim, Error> {
    claim_inner(
        store,
        &input.stream,
        &input.expected,
        &input.writer,
        input.serial,
        input.input.clone(),
        true,
    )
}
fn claim_inner(
    store: &DraftStore,
    stream: &Stream,
    expected: &Review,
    writer: &Writer,
    serial: u64,
    input: Input,
    required_editor: bool,
) -> Result<Claim, Error> {
    if !store.0.is_autocommit() {
        return Err(Error::Storage);
    }
    let _live = stream.0.live_guard().map_err(|_| Error::Gap)?;
    let tx = disk(adapter_atomic::begin(&store.0))?;
    let (record, after) = prepare_record(&tx, stream, expected, writer, serial, input)?;
    disk(editor::check_claim(&tx, &record, required_editor))?;
    disk(journal::insert(&tx, &record))?;
    disk(storage::write_row(&tx, &after))?;
    disk(tx.commit())?;
    // A revoked stream never turns a committed possible attempt into permission.
    if !stream.is_live() {
        return Err(Error::Gap);
    }
    Ok(Claim { record })
}

/// Original Claim custody is enough to retain a reply after source/navigation/
/// account selection changes. No fresh UI, stream or source is consulted here.
/// Repeated storage settlement must use the exact same returned value.
pub(crate) fn settle(store: &DraftStore, claim: &Claim, result: Returned) -> Result<Record, Error> {
    if !store.0.is_autocommit() {
        return Err(Error::Storage);
    }
    let tx = disk(adapter_atomic::begin(&store.0))?;
    let value = disk(journal::settle(&tx, &claim.record, result))?;
    disk(faults::before_commit(faults::FaultPoint::MutationReceipt))?;
    disk(tx.commit())?;
    Ok(value)
}
/// Read-only restart inventory. It never grants a Claim, dispatch or cleanup.
pub(crate) fn retained(store: &DraftStore, account: &str) -> Result<Vec<Record>, Error> {
    disk(journal::all(&store.0, account))
}

#[cfg(test)]
#[path = "api_hosted_files_tests.rs"]
mod tests;

fn prepare_record(
    conn: &Connection,
    stream: &Stream,
    expected: &Review,
    writer: &Writer,
    serial: u64,
    input: Input,
) -> Result<(Record, ProtectedRow), Error> {
    let current = views::exact(conn, expected)?;
    let scope = current.scope();
    disk(journal::require(conn, &scope.account_id))?;
    disk(facade::admitted_hosted(conn, &stream.0, &current.inner))?;
    input.paired(&current)?;
    let state = current.row().state();
    if stream.account() != scope.account_id
        || stream.team() != scope.key.team
        || state.pending().is_some()
        || state.consumption().is_some()
        || state.blocked_observation().is_some()
        || !matches!(state.workflow(), Workflow::Idle)
        || writer.scope() != scope
        || writer.enrollment() != current.row().lineage().enrollment()
        || serial == 0
    {
        return Err(Error::Stale);
    }
    let receipt = writers::matching(conn, writer)?;
    if receipt.processed != serial {
        return Err(Error::Stale);
    }
    let writers::Retry::Applied(accepted) = writers::retry(
        conn,
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
    let after = disk(successor(&current.row().inner, state.clone()))?;
    let record = journal::Record::new(
        &current,
        writer,
        serial,
        &receipt,
        disk(storage::team(conn, stream.team()))?,
        &after,
        input,
    )?;
    Ok((record, after))
}

#[path = "api_hosted_editor.rs"]
pub(crate) mod editor;

pub(in super::super) fn dispatch_guard(conn: &Connection, scope: &Scope) -> Result<(), AppError> {
    journal::guard(conn, scope)
}

pub(super) fn upgrade_reference(
    conn: &Connection,
    account: &str,
    has_editor: bool,
    has_cleanup: bool,
) -> Result<(), AppError> {
    journal::install(conn, account)?;
    if has_editor {
        editor::install(conn, account)?;
    }
    if has_cleanup {
        cleanup::install(conn, account)?;
    }
    Ok(())
}

#[path = "api_hosted_cleanup.rs"]
pub(crate) mod cleanup;
#[path = "api_hosted_inventory.rs"]
mod inventory;
pub use inventory::Inventory;
pub(super) fn inventory(conn: &Connection, account: &str) -> Result<Inventory, AppError> {
    inventory::read(conn, account)
}
