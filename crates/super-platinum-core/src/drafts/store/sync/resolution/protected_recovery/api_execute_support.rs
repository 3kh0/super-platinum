//! Support command branches; called through a non-inlined heap boundary.
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
        #[cfg(test)]
        Action::PausePrepared {
            entered,
            resume,
            reply,
        } => {
            let result = if isolated {
                faults::arm_prepared(entered, resume)
            } else {
                Err(Error::Stale)
            };
            let report = terminal::protected(&result, terminal::unsupported);
            let _ = reply.send(result);
            report
        }
        #[cfg(any(test, feature = "protected-test-support"))]
        Action::ArmFault { point, reply } => {
            let result = if isolated {
                faults::arm(point)
            } else {
                Err(Error::Stale)
            };
            let report = terminal::protected(&result, terminal::unsupported);
            let _ = reply.send(result);
            report
        }
        #[cfg(any(test, feature = "protected-test-support"))]
        Action::Initialize { reply } => {
            let result = (|| {
                if !isolated {
                    return Err(Error::Stale);
                }
                if !valid_uuid(account) {
                    return Err(Error::Stale);
                }
                let opened = open(account).map_err(|_| Error::Storage)?;
                let store = &opened;
                let tx = adapter_atomic::begin(&store.0).map_err(|_| Error::Storage)?;
                storage::activate(store, account).map_err(|_| Error::Storage)?;
                adapter_schema::install(store, account).map_err(|_| Error::Storage)?;
                tx.commit().map_err(|_| Error::Storage)
            })();
            let report = terminal::protected(&result, terminal::accepted);
            let _ = reply.send(result);
            report
        }
        _ => unreachable!("internal protected command dispatch group mismatch"),
    }
}
