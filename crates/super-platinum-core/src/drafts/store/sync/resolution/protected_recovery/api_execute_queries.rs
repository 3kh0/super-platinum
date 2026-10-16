//! Queries command branches; called through a non-inlined heap boundary.
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
        #[cfg(any(test, feature = "protected-test-support"))]
        Action::Inventory { reply } => {
            let result = (|| {
                if !valid_uuid(account) {
                    return Err(Error::Stale);
                }
                let opened = open(account).map_err(|_| Error::Storage)?;
                adapter_schema::require(&opened.0).map_err(|_| Error::WrongFormat)?;
                loading::load(&opened, account, LoadMode::Current, &pending)
                    .map(std::sync::Arc::new)
            })();
            let report = terminal::protected(&result, |_| {
                terminal::Report::new(terminal::Execution::ReadObserved)
            });
            let _ = reply.send(result);
            report
        }
        Action::Load { mode, reply } => {
            let result = (|| {
                if !valid_uuid(account) {
                    return Err(Error::Stale);
                }
                if mode == LoadMode::AfterRestart {
                    lifecycle::revoke_account(account);
                }
                let opened = open(account).map_err(|_| Error::Storage)?;
                let store = &opened;
                adapter_schema::require(&store.0).map_err(|_| Error::WrongFormat)?;
                loading::load(store, account, mode, &pending)
            })();
            let report = terminal::protected(&result, |_| match mode {
                LoadMode::Current => terminal::Report::new(terminal::Execution::ReadObserved),
                LoadMode::AfterRestart => {
                    terminal::Report::new(terminal::Execution::StorageAccepted)
                }
            });
            let _ = reply.send(result);
            report
        }
        Action::Review { scope, reply } => {
            let result = (|| {
                if !(scope.account_id == account) {
                    return Err(Error::Stale);
                }
                if !valid_uuid(account) {
                    return Err(Error::Stale);
                }
                let opened = open(account).map_err(|_| Error::Storage)?;
                let store = &opened;
                adapter_schema::require(&store.0).map_err(|_| Error::WrongFormat)?;
                let mut result = views::review(&store.0, &scope)?;
                result.uncommitted = pending.get(&scope.key).cloned();
                Ok(result)
            })();
            let report = terminal::protected(&result, terminal::read);
            let _ = reply.send(result);
            report
        }
        Action::ReviewLocal { scope, reply } => {
            let result = (|| {
                if !(scope.account_id == account) {
                    return Err(Error::Stale);
                }
                if !valid_uuid(account) {
                    return Err(Error::Stale);
                }
                let opened = open(account).map_err(|_| Error::Storage)?;
                let store = &opened;
                adapter_schema::require(&store.0).map_err(|_| Error::WrongFormat)?;
                let mut result = views::local(&store.0, &scope).map_err(|_| Error::Storage)?;
                result.uncommitted = pending.get(&scope.key).cloned();
                Ok(result)
            })();
            let report = terminal::protected(&result, terminal::read);
            let _ = reply.send(result);
            report
        }
        Action::EnrollLocal {
            scope,
            expected,
            reply,
        } => {
            let result = (|| {
                if !(scope.account_id == account && expected.scope().account_id == account) {
                    return Err(Error::Stale);
                }
                if pending.contains_key(&scope.key) {
                    return Err(Error::Pending);
                }
                if !valid_uuid(account) {
                    return Err(Error::Stale);
                }
                let opened = open(account).map_err(|_| Error::Storage)?;
                let store = &opened;
                adapter_schema::require(&store.0).map_err(|_| Error::WrongFormat)?;
                operations::enroll(store, &scope, &expected)
            })();
            let report = terminal::protected(&result, terminal::review);
            let _ = reply.send(result);
            report
        }
        Action::RegisterWriter { expected, reply } => {
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
                writers::register(store, &expected)
            })();
            let report = terminal::protected(&result, terminal::accepted);
            let _ = reply.send(result);
            report
        }
        Action::Receipt {
            expected,
            operation,
            reply,
        } => {
            let result = (|| {
                if !(expected.scope().account_id == account
                    && operation.scope.account_id == account)
                {
                    return Err(Error::Stale);
                }
                if !valid_uuid(account) {
                    return Err(Error::Stale);
                }
                let opened = open(account).map_err(|_| Error::Storage)?;
                let store = &opened;
                adapter_schema::require(&store.0).map_err(|_| Error::WrongFormat)?;
                operations::receipt(store, &expected, &operation)
            })();
            let report = terminal::protected(&result, terminal::read);
            let _ = reply.send(result);
            report
        }
        _ => unreachable!("internal protected command dispatch group mismatch"),
    }
}
