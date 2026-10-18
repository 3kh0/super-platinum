//! Explicit CSV initiation. Receipts outlive views; acknowledgment is not delivery.
use super::*;
use super_platinum_core::{
    block_actions::workflow_management::{ActivityExport, ActivityExportInitiated},
    slack::PreparedRequest,
};
mod book;
pub(crate) mod fixture;
mod native;
#[cfg(test)]
mod tests;
mod ticket;
pub(in crate::app_home::about::management) use book::Attempt;
use book::*;
use ticket::{Preparation, Ticket};

pub(super) fn start(mut state: Signal<ShellState>, visit: u64) {
    let Some(ticket) = Ticket::capture(&state.peek(), visit) else {
        return;
    };
    dioxus::core::spawn_forever(async move {
        let Some((native, initial, dates)) = native::capture(&ticket.expected()).await else {
            return;
        };
        let lease = {
            if !ticket.current(&state.peek()) {
                return;
            }
            let mut s = state.write();
            let Some(lease) = reserve(&mut s, &ticket) else {
                return;
            };
            lease
        };
        let activated = native.activate(lease.id).await;
        drop(initial);
        let prep = Preparation {
            ticket,
            lease,
            native,
        };
        let policy = if activated {
            tokio::select! { biased;
                _ = prep.retired(state) => Err(unavailable()),
                value = policy::acquire(state, &prep) => value,
            }
        } else {
            Err(unavailable())
        };
        let guard = prep.native.lock().await;
        let request = if guard.is_some() && prep.preparing(&state.peek()) {
            let mut s = state.write();
            match policy {
                Ok(fresh) if fresh.current(&s) => {
                    let allowed =
                        fresh.can_export_activity() && fresh.same(&prep.ticket.permission);
                    // A newly observed denial must not leave the old enabled action.
                    s.app_home.about.management.surface.as_mut().unwrap().policy =
                        Some(fresh.clone());
                    if allowed {
                        let query = query(&prep.ticket.workflow, &prep.ticket.filters, dates);
                        query
                            .ok()
                            .and_then(|q| {
                                ActivityExport::new(&q, fresh.activity_export_assignment()).ok()
                            })
                            .and_then(|holder| claim(&mut s, prep.lease.id, holder))
                    } else {
                        None
                    }
                }
                _ => None,
            }
        } else {
            None
        };
        let Some(request) = request else {
            finish_not_sent(state, &prep, guard).await;
            return;
        };
        // No await between the guarded claim and this single invocation. The
        // short lease unlocks during I/O; the retained evaluator still binds all
        // original nodes when the receipt is later published.
        drop(guard);
        let synthetic = fixture::enabled(&state.peek());
        let result = if synthetic {
            fixture::request(state, prep.lease.id, &request).await
        } else if crate::fixture::is_fixture() {
            Err(Error::Api("workflow_management_not_sent_fixture".into()))
        } else {
            prep.ticket
                .owner
                .ctx
                .transport
                .execute_workflow_management(request)
                .await
        };
        // Retain the actual reply even after navigation/account replacement.
        // Internal receipt storage cannot render over an uncommitted buffer.
        #[allow(deprecated)]
        {
            let mut s = state.write_silent();
            settle(&mut s, prep.lease.id, result.clone());
        }
        let guard = prep.native.lock().await;
        if guard.is_some() && prep.cleanup_current(&state.peek()) {
            let mut s = state.write();
            if prep.cleanup_current(&s) {
                retain_limit(&mut s, &prep.ticket.owner, &result);
                prep.lease.finish(&mut s, None);
            }
        } else {
            #[allow(deprecated)]
            {
                let mut s = state.write_silent();
                retain_limit(&mut s, &prep.ticket.owner, &result);
                prep.lease.finish(&mut s, None);
            }
        }
        if let Some(guard) = guard {
            let _ = guard.close().await;
        }
    });
}
async fn finish_not_sent(
    mut state: Signal<ShellState>,
    prep: &Preparation,
    guard: Option<native::Release>,
) {
    if guard.is_some() && prep.cleanup_current(&state.peek()) {
        let mut s = state.write();
        stop(&mut s, prep.lease.id);
        prep.lease.finish(&mut s, None);
    } else {
        #[allow(deprecated)]
        {
            let mut s = state.write_silent();
            stop(&mut s, prep.lease.id);
            prep.lease.finish(&mut s, None);
        }
    }
    if let Some(guard) = guard {
        let _ = guard.close().await;
    }
}
fn retain_limit(s: &mut ShellState, owner: &Owner, result: &Result<Value, Error>) {
    if let Err(Error::RateLimited { retry_after_secs }) = result {
        navigation::retain_limit(s, &owner.ctx, *retry_after_secs);
    }
}
pub(super) fn busy(s: &ShellState, visit: u64) -> bool {
    document(s, visit).is_some_and(|d| {
        s.app_home
            .about
            .management
            .exports
            .iter()
            .any(|a| a.matches(d) && matches!(a.phase, Phase::Preparing | Phase::Sent))
    })
}
pub(super) fn status(s: &ShellState, visit: u64) -> (bool, bool, Option<&'static str>) {
    let Some(d) = document(s, visit).filter(|d| d.activity.active) else {
        return (false, true, None);
    };
    let h = s.app_home.about.management.surface.as_ref().unwrap();
    let shown = h
        .policy
        .as_ref()
        .is_some_and(|p| p.current(s) && p.can_export_activity());
    let disabled = !permitted(s, visit)
        || h.read.is_some()
        || blocked(s, d)
        || s.app_home.about.management.exports.len() >= MAX_EXPORTS;
    let notice = s
        .app_home
        .about
        .management
        .exports
        .iter()
        .rev()
        .find(|a| {
            a.matches(d)
                && (matches!(a.phase, Phase::Preparing | Phase::Sent)
                    || same_filters(&a.ticket.filters, &d.activity.filters))
        })
        .map(|a| a.phase.notice());
    (shown, disabled, notice)
}
pub(crate) fn fixture_plan(s: &ShellState, id: u64, request: &PreparedRequest) -> bool {
    s.app_home
        .about
        .management
        .exports
        .iter()
        .find(|a| a.id == id)
        .is_some_and(|a| {
            a.phase == Phase::Sent && a.receipt.is_none() && a.request.as_ref() == Some(request)
        })
}
pub(crate) fn snapshot(s: &ShellState) -> Value {
    let book = &s.app_home.about.management;
    let (shown, disabled, notice) = book
        .surface
        .as_ref()
        .map(|h| status(s, h.id))
        .unwrap_or((false, true, None));
    json!({"attempts":book.exports.iter().map(|a|json!({"id":a.id,"document":a.ticket.document,"visit":a.ticket.visit,"phase":a.phase.name(),
        "arguments":a.holder.as_ref().map(|h|h.arguments()),"receipt":a.receipt.as_ref().map(|r|match r {Ok(raw)=>raw.clone(),Err(error)=>json!({"failure":match error {Error::Offline(_)=>"offline",Error::Transport(_)=>"transport",Error::Api(_)=>"api",Error::RateLimited{..}=>"rate-limited",_=>"other"}})}),"initiated":a.initiated.is_some()})).collect::<Vec<_>>(),
        "current":{"shown":shown,"disabled":disabled,"notice":notice},"retained":book.exports.len()})
}
