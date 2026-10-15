//! Mutations command branches; called through a non-inlined heap boundary.
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
        Action::Consume {
            stream,
            expected,
            kind,
            reply,
        } => {
            let result = (|| {
                if !(stream.account() == account && expected.scope().account_id == account) {
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
                operations::consume(store, &stream, &expected, kind)
            })();
            let report = terminal::protected(&result, terminal::review);
            let _ = reply.send(result);
            report
        }
        Action::ReviewedSchedule {
            stream,
            expected,
            reviewed,
            post_at,
            reply,
        } => {
            let result = (|| {
                if !(stream.account() == account && expected.scope().account_id == account) {
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
                operations::reviewed_schedule(store, &stream, &expected, &reviewed, post_at)
            })();
            let report = terminal::protected(&result, terminal::review);
            let _ = reply.send(result);
            report
        }
        Action::RejectedSchedule {
            stream,
            expected,
            choice,
            now_secs,
            reply,
        } => {
            let result = (|| {
                if !(stream.account() == account && expected.scope().account_id == account) {
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
                operations::rejected_schedule(store, &stream, &expected, &choice, now_secs)
            })();
            let report = terminal::protected(&result, terminal::review);
            let _ = reply.send(result);
            report
        }
        Action::Control {
            expected,
            action,
            reply,
        } => {
            let result = (|| {
                if !(expected.scope().account_id == account) {
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
                operations::control(store, &expected, action)
            })();
            let report = terminal::protected(&result, terminal::review);
            let _ = reply.send(result);
            report
        }
        Action::Prepare {
            stream,
            expected,
            reply,
        } => {
            let result = (|| {
                if !(stream.account() == account && expected.scope().account_id == account) {
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
                operations::prepare(store, &stream, &expected)
            })();
            let report = terminal::protected(&result, |_| terminal::accepted(&()).pending(true));
            let _ = reply.send(result);
            report
        }
        #[cfg(any(test, feature = "protected-test-support"))]
        Action::PrepareConsumedSend {
            stream,
            consumed,
            reply,
        } => {
            let result = (|| {
                if !(stream.account() == account && consumed.scope().account_id == account) {
                    return Err(Error::Stale);
                }
                let scope: Option<&Scope> = Some(consumed.scope());
                if scope.is_some_and(|s| pending.contains_key(&s.key)) {
                    return Err(Error::Pending);
                }
                if !valid_uuid(account) {
                    return Err(Error::Stale);
                }
                let opened = open(account).map_err(|_| Error::Storage)?;
                let store = &opened;
                adapter_schema::require(&store.0).map_err(|_| Error::WrongFormat)?;
                operations::prepare_consumed_send(store, &stream, &consumed)
            })();
            let report = terminal::protected(&result, |_| terminal::accepted(&()).pending(true));
            let _ = reply.send(result);
            report
        }
        Action::ClaimDispatch { ticket, reply } => {
            let result = (|| {
                if !(ticket.receipt.scope().account_id == account
                    && ticket.stream.account() == account)
                {
                    return Err(Error::Stale);
                }
                if !valid_uuid(account) {
                    return Err(Error::Stale);
                }
                let opened = open(account).map_err(|_| Error::Storage)?;
                let store = &opened;
                adapter_schema::require(&store.0).map_err(|_| Error::WrongFormat)?;
                operations::claim(store, ticket)
            })();
            let report = terminal::protected(&result, |_| terminal::accepted(&()).pending(true));
            let _ = reply.send(result);
            report
        }
        Action::Record {
            receipt,
            outcome,
            known_until,
            reply,
        } => {
            let result = (|| {
                if !(receipt.scope().account_id == account) {
                    return Err(Error::Stale);
                }
                if !valid_uuid(account) {
                    return Err(Error::Stale);
                }
                let opened = open(account).map_err(|_| Error::Storage)?;
                let store = &opened;
                adapter_schema::require(&store.0).map_err(|_| Error::WrongFormat)?;
                operations::record(store, &receipt, &outcome, known_until)
            })();
            let report = terminal::protected(&result, terminal::review);
            let _ = reply.send(result);
            report
        }
        _ => unreachable!("internal protected command dispatch group mismatch"),
    }
}
