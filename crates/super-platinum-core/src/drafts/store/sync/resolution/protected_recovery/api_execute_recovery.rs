//! Recovery command branches; called through a non-inlined heap boundary.
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
        Action::RecoveryState { team, reply } => {
            let result = (|| {
                if !valid_uuid(account) {
                    return Err(Error::Stale);
                }
                let opened = open(account).map_err(|_| Error::Storage)?;
                let store = &opened;
                adapter_schema::require(&store.0).map_err(|_| Error::WrongFormat)?;
                checks::state(store, account, &team)
            })();
            let report = terminal::protected(&result, terminal::read);
            let _ = reply.send(result);
            report
        }
        Action::TakeOver {
            team,
            expected,
            reply,
        } => {
            let result = (|| {
                if !(expected
                    .as_ref()
                    .is_none_or(|e| e.account() == account && e.team() == team))
                {
                    return Err(Error::Stale);
                }
                if !valid_uuid(account) {
                    return Err(Error::Stale);
                }
                lifecycle::revoke_team(account, &team);
                let opened = open(account).map_err(|_| Error::Storage)?;
                let store = &opened;
                adapter_schema::require(&store.0).map_err(|_| Error::WrongFormat)?;
                checks::takeover(store, account, &team, expected.as_ref())
            })();
            let report = terminal::protected(&result, terminal::accepted);
            let _ = reply.send(result);
            report
        }
        Action::ReserveCheck {
            recovery,
            expected,
            receipt,
            now_ms,
            reply,
        } => {
            let result = (|| {
                if !(recovery.account == account
                    && expected.scope().account_id == account
                    && receipt.scope().account_id == account)
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
                checks::reserve(store, &recovery, &expected, &receipt, now_ms)
            })();
            let report = terminal::protected(&result, |_| terminal::accepted(&()).pending(true));
            let _ = reply.send(result);
            report
        }
        Action::ClaimCheck {
            recovery,
            ticket,
            now_ms,
            reply,
        } => {
            let result = (|| {
                if !(recovery.account == account && ticket.receipt.scope().account_id == account) {
                    return Err(Error::Stale);
                }
                if !valid_uuid(account) {
                    return Err(Error::Stale);
                }
                let opened = open(account).map_err(|_| Error::Storage)?;
                let store = &opened;
                adapter_schema::require(&store.0).map_err(|_| Error::WrongFormat)?;
                checks::claim(store, &recovery, ticket, now_ms)
            })();
            let report = terminal::protected(&result, |value| match value {
                CheckClaim::Ready { .. } => terminal::accepted(value).pending(true),
                CheckClaim::Deferred { .. } => terminal::Report::new(terminal::Execution::Deferred),
            });
            let _ = reply.send(result);
            report
        }
        Action::CompleteCheck {
            completion,
            result,
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
                checks::complete(store, &completion, &result, known_until)
            })();
            let report = terminal::protected(&result, |value| {
                value
                    .review()
                    .map(terminal::review)
                    .unwrap_or_else(|| terminal::accepted(value))
            });
            let _ = reply.send(result);
            report
        }
        Action::SupersedeCheck {
            recovery,
            expected,
            admission,
            now_ms,
            reply,
        } => {
            let result = (|| {
                if !(recovery.account == account
                    && expected.scope().account_id == account
                    && admission.0.scope.account_id == account)
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
                checks::supersede(store, &recovery, &expected, &admission, now_ms)
            })();
            let report = terminal::protected(&result, terminal::accepted);
            let _ = reply.send(result);
            report
        }
        Action::AcceptLatePositive {
            expected,
            admission,
            reply,
        } => {
            let result = (|| {
                if !(expected.scope().account_id == account
                    && admission.0.scope.account_id == account)
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
                checks::accept_late(store, &expected, &admission)
            })();
            let report = terminal::protected(&result, terminal::review);
            let _ = reply.send(result);
            report
        }
        _ => unreachable!("internal protected command dispatch group mismatch"),
    }
}
