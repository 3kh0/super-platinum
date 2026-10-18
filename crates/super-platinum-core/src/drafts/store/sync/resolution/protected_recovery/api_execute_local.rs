//! Local command branches; called through a non-inlined heap boundary.
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
        Action::Release { expected, reply } => {
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
                operations::release(store, &expected)
            })();
            let report = terminal::protected(&result, terminal::accepted);
            let _ = reply.send(result);
            report
        }
        Action::StartLocal {
            release,
            writer,
            reply,
        } => {
            let result = (|| {
                if !(release.0.scope.account_id == account && writer.scope().account_id == account)
                {
                    return Err(Error::Stale);
                }
                let scope: Option<&Scope> = Some(writer.scope());
                if scope.is_some_and(|s| pending.contains_key(&s.key)) {
                    return Err(Error::Pending);
                }
                if !valid_uuid(account) {
                    return Err(Error::Stale);
                }
                let opened = open(account).map_err(|_| Error::Storage)?;
                let store = &opened;
                adapter_schema::require(&store.0).map_err(|_| Error::WrongFormat)?;
                local::start(store, &release, &writer)
            })();
            let report = terminal::protected(&result, terminal::accepted);
            let _ = reply.send(result);
            report
        }
        Action::StartRetainedLocal {
            release,
            writer,
            reply,
        } => {
            let result = (|| {
                if !(release.0.scope.account_id == account && writer.scope().account_id == account)
                {
                    return Err(Error::Stale);
                }
                let scope: Option<&Scope> = Some(writer.scope());
                if scope.is_some_and(|s| pending.contains_key(&s.key)) {
                    return Err(Error::Pending);
                }
                if !valid_uuid(account) {
                    return Err(Error::Stale);
                }
                let opened = open(account).map_err(|_| Error::Storage)?;
                let store = &opened;
                adapter_schema::require(&store.0).map_err(|_| Error::WrongFormat)?;
                local::start(store, &release, &writer)
            })();
            let report = terminal::protected(&result, terminal::accepted);
            let _ = reply.send(result);
            report
        }
        Action::RenewLocal { expected, reply } => {
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
                local::renew(store, &expected)
            })();
            let report = terminal::protected(&result, terminal::accepted);
            let _ = reply.send(result);
            report
        }
        Action::WriteLocal {
            lease,
            predecessor_serial,
            serial,
            expected_local,
            local,
            reply,
        } => {
            let result = (|| {
                if !(lease.scope().account_id == account) {
                    return Err(Error::Stale);
                }
                let scope: Option<&Scope> = Some(lease.scope());
                if scope.is_some_and(|s| pending.contains_key(&s.key)) {
                    return Err(Error::Pending);
                }
                if !valid_uuid(account) {
                    return Err(Error::Stale);
                }
                let opened = open(account).map_err(|_| Error::Storage)?;
                let store = &opened;
                adapter_schema::require(&store.0).map_err(|_| Error::WrongFormat)?;
                local::save(
                    store,
                    &lease,
                    predecessor_serial,
                    serial,
                    &expected_local,
                    &local,
                )
            })();
            let report = terminal::protected(&result, terminal::local_writer);
            let _ = reply.send(result);
            report
        }
        Action::KeepEditing {
            expected,
            intent,
            reply,
        } => {
            let result = (|| {
                if !(expected.scope().account_id == account && intent.scope.account_id == account) {
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
                operations::keep_editing(store, &expected, &intent)
            })();
            let report = terminal::protected(&result, terminal::accepted);
            let _ = reply.send(result);
            report
        }
        _ => unreachable!("internal protected command dispatch group mismatch"),
    }
}
