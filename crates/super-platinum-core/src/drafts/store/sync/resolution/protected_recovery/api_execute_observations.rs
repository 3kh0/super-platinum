//! Observations command branches; called through a non-inlined heap boundary.
use super::*;
use crate::drafts::store::terminal;
#[inline(never)]
pub(super) fn execute(
    command: Box<Command>,
    open: &dyn Fn(&str) -> Result<DraftStore, AppError>,
    pending: BTreeMap<DraftKey, LocalDraft>,
    isolated: bool,
) -> terminal::Report {
    let _ = (&pending, isolated, &open);
    let account = command.account.as_str();
    match command.action {
        Action::ReserveObservation {
            stream,
            expected,
            event,
            reply,
        } => {
            let result = (|| {
                if !(stream.account() == account
                    && expected.scope().account_id == account
                    && event.scope.account_id == account)
                {
                    return Err(Error::Stale);
                }
                let scope: Option<&Scope> = Some(expected.scope());
                if scope.is_some_and(|s| pending.contains_key(&s.key)) {
                    return Err(Error::Pending);
                }
                if !valid_uuid(account) {
                    return Err(Error::Stale);
                }
                let opened = open(account).map_err(|_| Error::Storage)?;
                let store = &opened;
                adapter_schema::require(&store.0).map_err(|_| Error::WrongFormat)?;
                observation::reserve(store, &stream, &expected, &event)
            })();
            let report = terminal::protected(&result, terminal::accepted);
            let _ = reply.send(result);
            report
        }
        Action::FinishObservation { ticket, reply } => {
            let result = finish_open(account, &ticket, &pending, open)
                .and_then(|store| observation::finish(&store, ticket));
            let report = terminal::protected(&result, |value| terminal::review(value.after()));
            let _ = reply.send(result);
            report
        }
        Action::FinishRetainedObservation { ticket, reply } => {
            let result = finish_open(account, &ticket, &pending, open)
                .and_then(|store| observation::finish_retained(&store, &ticket));
            let report = terminal::protected(&result, |value| terminal::review(value.after()));
            let _ = reply.send(result);
            report
        }
        _ => unreachable!("internal protected command dispatch group mismatch"),
    }
}

fn finish_open(
    account: &str,
    ticket: &ObservationTicket,
    pending: &BTreeMap<DraftKey, LocalDraft>,
    open: &dyn Fn(&str) -> Result<DraftStore, AppError>,
) -> Result<DraftStore, Error> {
    if ticket.review.scope().account_id != account {
        return Err(Error::Stale);
    }
    if pending.contains_key(&ticket.review.scope().key) {
        return Err(Error::Pending);
    }
    if !valid_uuid(account) {
        return Err(Error::Stale);
    }
    let opened = open(account).map_err(|_| Error::Storage)?;
    adapter_schema::require(&opened.0).map_err(|_| Error::WrongFormat)?;
    Ok(opened)
}
