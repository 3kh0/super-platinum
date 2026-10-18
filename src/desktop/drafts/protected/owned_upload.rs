//! Storage-only adapter for the existing upload journal. Call off-renderer.
//! Every holder belongs to the actual configured Account/Operations; no worker
//! creation, HTTP, publication, retry queue, or caller-created ticket authority.
use super::*;
use std::sync::Mutex;
use super_platinum_core::{
    drafts::store::{
        protected::owned_store as o,
        uploads::{self, Draft, Reservation, Stored, Ticket},
    },
    error::AppError,
};

#[derive(Default)]
pub(super) struct Book(Mutex<BTreeMap<String, Arc<Mutex<Entry>>>>);
struct Entry {
    reservation: Result<o::UploadJob<Draft, Reservation>, o::AdmissionRefused<Draft>>,
    child: Option<Child>,
}
enum Child {
    Finalize {
        files: Vec<String>,
        job: Result<o::UploadFinalization, o::AdmissionRefused<Vec<String>>>,
    },
    Cancel(Result<o::UploadCancellation, o::AdmissionError>),
}
fn failure() -> AppError {
    std::io::Error::other("Owned upload storage is unavailable; recovery remains retained").into()
}
fn account(id: &str) -> Result<Arc<Account>, AppError> {
    super::production::account(id).map_err(|_| failure())
}
fn route(account: &Account) -> Result<&Arc<super::owned_route::Route>, AppError> {
    account.owned.as_ref().ok_or_else(failure)
}
fn remove(
    route: &super::owned_route::Route,
    client: &str,
    entry: &Arc<Mutex<Entry>>,
) -> Result<(), AppError> {
    let mut book = route.uploads.0.lock().map_err(|_| failure())?;
    if !book.get(client).is_some_and(|old| Arc::ptr_eq(old, entry)) {
        return Err(failure());
    }
    book.remove(client);
    Ok(())
}
pub(crate) fn reserve(draft: Draft) -> Result<Reservation, AppError> {
    reserve_in(&account(&draft.account_id)?, draft)
}
fn reserve_in(account: &Arc<Account>, draft: Draft) -> Result<Reservation, AppError> {
    let route = route(account)?;
    if route.operations.account() != draft.account_id {
        return Err(failure());
    }
    let client = draft.client_msg_id.clone();
    let entry = {
        let mut book = route.uploads.0.lock().map_err(|_| failure())?;
        // A repeat caller cannot convert a retained created=true result into
        // another network permission. The original holder remains inspectable.
        if book.contains_key(&client) || book.len() >= uploads::MAX_ACTIVE {
            return Err(failure());
        }
        let entry = Arc::new(Mutex::new(Entry {
            reservation: route.operations.reserve_upload(draft),
            child: None,
        }));
        book.insert(client.clone(), entry.clone());
        entry
    };
    let held = entry.lock().map_err(|_| failure())?;
    let job = held.reservation.as_ref().map_err(|_| failure())?;
    job.submit().map_err(|_| failure())?;
    let result = job.wait().map_err(|_| failure())?;
    let value = result.as_ref().as_ref().map_err(|_| failure())?.clone();
    if !value.created {
        let (observed, ack) = job.observe_existing().map_err(|_| failure())?;
        if observed != value {
            return Err(failure());
        }
        ack.acknowledge().map_err(|_| failure())?;
        drop(held);
        remove(route, &client, &entry)?;
    }
    Ok(value)
}
pub(crate) fn finalize(ticket: Ticket, files: Vec<String>) -> Result<Option<Stored>, AppError> {
    finalize_in(&account(ticket.account_id())?, ticket, files)
}
fn finalize_in(
    account: &Arc<Account>,
    ticket: Ticket,
    files: Vec<String>,
) -> Result<Option<Stored>, AppError> {
    let route = route(account)?;
    let entry = route
        .uploads
        .0
        .lock()
        .map_err(|_| failure())?
        .get(ticket.client_msg_id())
        .cloned()
        .ok_or_else(failure)?;
    let mut held = entry.lock().map_err(|_| failure())?;
    let parent = held.reservation.as_ref().map_err(|_| failure())?.clone();
    let reserved = parent.wait().map_err(|_| failure())?;
    if !reserved
        .as_ref()
        .as_ref()
        .is_ok_and(|r| r.created && r.stored.ticket == ticket)
    {
        return Err(failure());
    }
    if held.child.is_none() {
        // Install the actual returned holder/refusal before wait. Cancellation
        // of this caller cannot lose returned file IDs or repeat submission.
        held.child = Some(Child::Finalize {
            files: files.clone(),
            job: parent.finalize(files.clone()),
        });
    }
    let Some(Child::Finalize {
        files: original,
        job,
    }) = &held.child
    else {
        return Err(failure());
    };
    if original != &files {
        return Err(failure());
    }
    let job = job.as_ref().map_err(|_| failure())?;
    job.submit().map_err(|_| failure())?;
    let result = job.wait().map_err(|_| failure())?;
    let value = result.as_ref().as_ref().map_err(|_| failure())?.clone();
    if let Some(row) = &value {
        let (observed, ack) = job.observe_finalized().map_err(|_| failure())?;
        if &observed != row {
            return Err(failure());
        }
        ack.acknowledge().map_err(|_| failure())?;
        drop(held);
        remove(route, ticket.client_msg_id(), &entry)?;
    }
    Ok(value)
}
pub(crate) fn retire(row: Stored) -> Result<bool, AppError> {
    retire_in(&account(row.ticket.account_id())?, row)
}
fn retire_in(account: &Arc<Account>, row: Stored) -> Result<bool, AppError> {
    let route = route(account)?;
    let entry = route
        .uploads
        .0
        .lock()
        .map_err(|_| failure())?
        .get(row.ticket.client_msg_id())
        .cloned();
    if let Some(entry) = entry {
        let mut held = entry.lock().map_err(|_| failure())?;
        let parent = held.reservation.as_ref().map_err(|_| failure())?.clone();
        let reserved = parent.wait().map_err(|_| failure())?;
        if !reserved
            .as_ref()
            .as_ref()
            .is_ok_and(|r| r.created && r.stored == row)
        {
            return Err(failure());
        }
        if held.child.is_none() {
            held.child = Some(Child::Cancel(parent.cancel_reservation()));
        }
        let Some(Child::Cancel(job)) = &held.child else {
            return Err(failure());
        };
        let job = job.as_ref().map_err(|_| failure())?;
        job.submit().map_err(|_| failure())?;
        let result = job.wait().map_err(|_| failure())?;
        let value = *result.as_ref().as_ref().map_err(|_| failure())?;
        if value {
            job.observe_canceled()
                .map_err(|_| failure())?
                .acknowledge()
                .map_err(|_| failure())?;
            drop(held);
            remove(route, row.ticket.client_msg_id(), &entry)?;
        }
        return Ok(value);
    }
    let job = route.operations.retire_upload(row).map_err(|_| failure())?;
    job.submit().map_err(|_| failure())?;
    let result = job.wait().map_err(|_| failure())?;
    let value = *result.as_ref().as_ref().map_err(|_| failure())?;
    if value {
        job.observe_retired()
            .map_err(|_| failure())?
            .acknowledge()
            .map_err(|_| failure())?;
    }
    Ok(value)
}
pub(crate) fn load(id: &str) -> Result<Vec<Stored>, AppError> {
    load_in(&account(id)?)
}
fn load_in(account: &Arc<Account>) -> Result<Vec<Stored>, AppError> {
    let route = route(account)?;
    let job = route.operations.upload_inventory().map_err(|_| failure())?;
    job.submit().map_err(|_| failure())?;
    job.wait().map_err(|_| failure())?;
    let (rows, ack) = job.observe_loaded().map_err(|_| failure())?;
    ack.acknowledge().map_err(|_| failure())?;
    Ok(rows)
}

#[cfg(test)]
#[path = "owned_upload_tests.rs"]
mod tests;
