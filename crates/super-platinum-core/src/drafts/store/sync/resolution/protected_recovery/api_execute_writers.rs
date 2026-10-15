//! Writers command branches; called through a non-inlined heap boundary.
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
        Action::Checkpoint {
            writer,
            serial,
            expected,
            local,
            direct,
            reply,
        } => {
            let result = (|| {
                if !(writer.scope().account_id == account && expected.scope().account_id == account)
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
                writers::checkpoint(
                    store,
                    &writer,
                    serial,
                    Some(&expected),
                    &local,
                    direct.as_ref(),
                )
            })();
            let report = terminal::protected(&result, terminal::writer);
            let _ = reply.send(result);
            report
        }
        Action::PreserveVersion {
            writer,
            serial,
            local,
            reply,
        } => {
            let result = (|| {
                if !(writer.scope().account_id == account) {
                    return Err(Error::Stale);
                }
                if !valid_uuid(account) {
                    return Err(Error::Stale);
                }
                let opened = open(account).map_err(|_| Error::Storage)?;
                let store = &opened;
                adapter_schema::require(&store.0).map_err(|_| Error::WrongFormat)?;
                writers::checkpoint(store, &writer, serial, None, &local, None)
            })();
            let report = terminal::protected(&result, terminal::writer);
            let _ = reply.send(result);
            report
        }
        Action::Clear { expected, reply } => {
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
                operations::control(store, &expected, Control::Clear)
            })();
            let report = terminal::protected(&result, terminal::review);
            let _ = reply.send(result);
            report
        }
        Action::SettleLocal {
            expected,
            choice,
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
                operations::settle(store, &expected, &choice)
            })();
            let report = terminal::protected(&result, terminal::review);
            let _ = reply.send(result);
            report
        }
        _ => unreachable!("internal protected command dispatch group mismatch"),
    }
}
