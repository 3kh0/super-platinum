//! Baseline command branches; called through a non-inlined heap boundary.
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
        Action::ReserveBaseline {
            stream,
            expected,
            now_ms,
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
                baseline::reserve(store, &stream, &expected, now_ms)
            })();
            let report = terminal::protected(&result, |_| terminal::accepted(&()).pending(true));
            let _ = reply.send(result);
            report
        }
        Action::ClaimBaseline {
            ticket,
            now_ms,
            reply,
        } => {
            let result = (|| {
                if !(ticket.review.scope().account_id == account) {
                    return Err(Error::Stale);
                }
                let scope: Option<&Scope> = Some(ticket.review.scope());
                if scope.is_some_and(|s| pending.contains_key(&s.key)) {
                    return Err(Error::Pending);
                }
                if !valid_uuid(account) {
                    return Err(Error::Stale);
                }
                let opened = open(account).map_err(|_| Error::Storage)?;
                let store = &opened;
                adapter_schema::require(&store.0).map_err(|_| Error::WrongFormat)?;
                baseline::claim(store, ticket, now_ms)
            })();
            let report = terminal::protected(&result, |value| match value {
                BaselineClaim::Ready { .. } => terminal::accepted(value).pending(true),
                BaselineClaim::Deferred { .. } => {
                    terminal::Report::new(terminal::Execution::Deferred)
                }
            });
            let _ = reply.send(result);
            report
        }
        Action::RetainBaseline {
            completion,
            response,
            known_until,
            reply,
        } => {
            let result = (|| {
                if !(completion.scope().account_id == account) {
                    return Err(Error::Stale);
                }
                if !valid_uuid(account) {
                    return Err(Error::Stale);
                }
                let opened = open(account).map_err(|_| Error::Storage)?;
                let store = &opened;
                adapter_schema::require(&store.0).map_err(|_| Error::WrongFormat)?;
                baseline::retain(store, &completion, response, known_until)
            })();
            let report = terminal::protected(&result, |value| match value {
                BaselineResult::Retained(_) | BaselineResult::Nonpositive => {
                    terminal::accepted(value)
                }
            });
            let _ = reply.send(result);
            report
        }
        Action::FinishBaseline {
            evidence,
            choice,
            reply,
        } => {
            let result = (|| {
                if !(evidence.scope().account_id == account) {
                    return Err(Error::Stale);
                }
                let scope: Option<&Scope> = Some(evidence.scope());
                if scope.is_some_and(|s| pending.contains_key(&s.key)) {
                    return Err(Error::Pending);
                }
                if !valid_uuid(account) {
                    return Err(Error::Stale);
                }
                let opened = open(account).map_err(|_| Error::Storage)?;
                let store = &opened;
                adapter_schema::require(&store.0).map_err(|_| Error::WrongFormat)?;
                baseline::finish(store, &evidence, &choice)
            })();
            let report = terminal::protected(&result, terminal::review);
            let _ = reply.send(result);
            report
        }
        Action::SupersedeBaseline {
            recovery,
            expected,
            admission,
            now_ms,
            reply,
        } => {
            let result = (|| {
                if !(recovery.account == account
                    && expected.scope().account_id == account
                    && admission.scope.account_id == account)
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
                checks::supersede_baseline(store, &recovery, &expected, &admission, now_ms)
            })();
            let report = terminal::protected(&result, terminal::accepted);
            let _ = reply.send(result);
            report
        }
        _ => unreachable!("internal protected command dispatch group mismatch"),
    }
}
