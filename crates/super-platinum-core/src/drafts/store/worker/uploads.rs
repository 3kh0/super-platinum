//! Ordered, acknowledged journal operations. No operation is automatically retried.
use super::{AppError, DraftStore, Reply, Request, mpsc, receive, worker};
use crate::drafts::store::uploads::{Draft, Reservation, Stored, Ticket};

pub(super) enum Command {
    Reserve(Draft, Reply<Reservation>),
    Finalize(Ticket, Vec<String>, Reply<Option<Stored>>),
    Retire(Stored, Reply<bool>),
    Load(String, Reply<Vec<Stored>>),
}
pub(super) fn execute(command: Command, open: &impl Fn(&str) -> Result<DraftStore, AppError>) {
    match command {
        Command::Reserve(draft, reply) => {
            let result = open(&draft.account_id).and_then(|mut store| store.reserve_upload(&draft));
            let _ = reply.send(result);
        }
        Command::Finalize(ticket, files, reply) => {
            let result = open(ticket.account_id())
                .and_then(|mut store| store.finalize_upload(&ticket, files));
            let _ = reply.send(result);
        }
        Command::Retire(stored, reply) => {
            let result =
                open(stored.ticket.account_id()).and_then(|mut store| store.retire_upload(&stored));
            let _ = reply.send(result);
        }
        Command::Load(account, reply) => {
            let result = open(&account).and_then(|mut store| store.load_uploads(&account));
            let _ = reply.send(result);
        }
    }
}
/// Background only. A durable reservation precedes all network work. An existing
/// reservation (`created=false`) never authorizes publication or an upload retry.
pub fn reserve(draft: Draft) -> Result<Reservation, AppError> {
    let (tx, rx) = mpsc::sync_channel(1);
    worker().submit(Request::Upload(Command::Reserve(draft, tx)));
    receive(rx)
}
/// Background only. Publication must await this commit acknowledgement and use
/// its returned exact file IDs. None means retired: do not publish.
pub fn finalize(ticket: Ticket, files: Vec<String>) -> Result<Option<Stored>, AppError> {
    let (tx, rx) = mpsc::sync_channel(1);
    worker().submit(Request::Upload(Command::Finalize(ticket, files, tx)));
    receive(rx)
}
/// Background only. Exact retirement; an old snapshot cannot delete a newer one.
pub fn retire(stored: Stored) -> Result<bool, AppError> {
    let (tx, rx) = mpsc::sync_channel(1);
    worker().submit(Request::Upload(Command::Retire(stored, tx)));
    receive(rx)
}
/// Background only. Returns recovery state, never permission to resend.
pub fn load(account: &str) -> Result<Vec<Stored>, AppError> {
    let (tx, rx) = mpsc::sync_channel(1);
    worker().submit(Request::Upload(Command::Load(account.into(), tx)));
    receive(rx)
}

#[cfg(test)]
mod tests {
    use super::super::Worker;
    use super::*;
    use crate::drafts::store::{sync::tests::Database, uploads::tests::draft};

    #[test]
    fn worker_ack_follows_commit_and_late_finalize_cannot_reinsert() {
        let db = Database::new();
        let worker = Worker::default();
        let (tx, rx) = mpsc::sync_channel(1);
        worker.submit(Request::Upload(Command::Reserve(draft(), tx)));
        assert!(rx.try_recv().is_err());
        assert!(!worker.cycle(|_| Ok(db.open())));
        let stored = rx.recv().unwrap().unwrap().stored;
        assert_eq!(
            db.open().load_uploads(stored.ticket.account_id()).unwrap(),
            vec![stored.clone()]
        );
        let (retire_tx, retire_rx) = mpsc::sync_channel(1);
        let (late_tx, late_rx) = mpsc::sync_channel(1);
        worker.submit(Request::Upload(Command::Retire(stored.clone(), retire_tx)));
        worker.submit(Request::Upload(Command::Finalize(
            stored.ticket.clone(),
            vec!["F1".into()],
            late_tx,
        )));
        assert!(!worker.cycle(|_| Ok(db.open())));
        assert!(retire_rx.recv().unwrap().unwrap());
        assert!(late_rx.recv().unwrap().unwrap().is_none());
        assert!(
            db.open()
                .load_uploads(stored.ticket.account_id())
                .unwrap()
                .is_empty()
        );
    }
    #[test]
    fn dropped_ack_is_recoverable_and_storage_failure_is_not_retried() {
        let db = Database::new();
        let worker = Worker::default();
        let (tx, rx) = mpsc::sync_channel(1);
        worker.submit(Request::Upload(Command::Reserve(draft(), tx)));
        drop(rx);
        assert!(!worker.cycle(|_| Ok(db.open())));
        assert_eq!(
            db.open().load_uploads(&draft().account_id).unwrap().len(),
            1
        );
        assert!(!db.open().reserve_upload(&draft()).unwrap().created);
        let (tx, rx) = mpsc::sync_channel(1);
        worker.submit(Request::Upload(Command::Load(draft().account_id, tx)));
        assert!(!worker.cycle(|_| Err(std::io::Error::other("synthetic disk failure").into())));
        assert!(rx.recv().unwrap().is_err());
        assert!(!worker.cycle(|_| panic!("explicit failed journal request must not retry")));
    }
}
