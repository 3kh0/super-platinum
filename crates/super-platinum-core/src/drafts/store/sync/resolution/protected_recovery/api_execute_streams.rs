//! Streams command branches; called through a non-inlined heap boundary.
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
        Action::StreamState { team, reply } => {
            let result = (|| {
                if !valid_uuid(account) {
                    return Err(Error::Stale);
                }
                let opened = open(account).map_err(|_| Error::Storage)?;
                let store = &opened;
                adapter_schema::require(&store.0).map_err(|_| Error::WrongFormat)?;
                operations::stream_state(store, account, &team)
            })();
            let report = terminal::protected(&result, terminal::read);
            let _ = reply.send(result);
            report
        }
        Action::OpenStream {
            team,
            expected,
            reply,
        } => {
            let result = (|| {
                if !(expected.account() == account && expected.team() == team) {
                    return Err(Error::Stale);
                }
                if !valid_uuid(account) {
                    return Err(Error::Stale);
                }
                lifecycle::revoke_team(account, &team);
                let opened = open(account).map_err(|_| Error::Storage)?;
                let store = &opened;
                adapter_schema::require(&store.0).map_err(|_| Error::WrongFormat)?;
                operations::open_stream(store, account, &team, &expected)
            })();
            let report = terminal::protected(&result, terminal::accepted);
            let _ = reply.send(result);
            report
        }
        Action::MarkGap {
            stream,
            reason,
            reply,
        } => {
            let result = (|| {
                if !(stream.account() == account) {
                    return Err(Error::Stale);
                }
                if !valid_uuid(account) {
                    return Err(Error::Stale);
                }
                let opened = open(account).map_err(|_| Error::Storage)?;
                let store = &opened;
                adapter_schema::require(&store.0).map_err(|_| Error::WrongFormat)?;
                operations::mark_gap(store, &stream, reason)
            })();
            let report = terminal::protected(&result, terminal::accepted);
            let _ = reply.send(result);
            report
        }
        Action::VerifyLocal {
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
                operations::verify(store, &stream, &expected)
            })();
            let report = terminal::protected(&result, terminal::review);
            let _ = reply.send(result);
            report
        }
        _ => unreachable!("internal protected command dispatch group mismatch"),
    }
}
