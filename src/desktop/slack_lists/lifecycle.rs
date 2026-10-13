//! Files viewing leases drive temporary List socket subscriptions. HTTP work is
//! serialized locally per file. A timed-out HTTP close can still have an unknown
//! server outcome; local serialization does not prove remote completion order.
use super::*;
use super_platinum_core::{
    slack::{api, realtime},
    state::RealtimeStatus,
};

const PING_MS: u64 = 30_000;
const UNSUBSCRIBE_MS: u64 = 60_000;
const ENQUEUE_RETRY_MS: u64 = 1_000;
type Key = (String, String);
#[derive(Clone, Default)]
pub(super) struct State {
    entries: BTreeMap<Key, Lease>,
}
#[derive(Clone)]
struct Lease {
    scope: Scope,
    id: String,
    generation: u64,
    desired: Option<u64>,
    opened: bool,
    renew: bool,
    failed_open: bool,
    open_done: bool,
    eligible: bool,
    in_flight: Option<(u64, Kind)>,
    next_ping: u64,
    close_due: Option<u64>,
    queued_generation: Option<u64>,
    next_enqueue: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Open,
    Ping { focused: bool },
    Close,
}
pub(crate) struct Request {
    key: Key,
    lease: u64,
    serial: u64,
    kind: Kind,
    visit: Option<u64>,
    prior_opened: bool,
    scope: Scope,
    at: u64,
}
pub(crate) fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| u64::try_from(duration.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}
fn diagnostic(event: &str, lease: &Lease, queued: Option<bool>) {
    if std::env::var_os("SUPER_PLATINUM_LIST_VIEWING_TRACE").is_some() {
        eprintln!(
            "{}",
            serde_json::json!({"list_viewing":event,"team":lease.scope.team,"file_id":lease.id,"lease":lease.generation,"eligible":lease.eligible,"enqueue_accepted":queued})
        );
    }
}
/// Called by the existing runtime tick. With no visible or retiring reader this
/// immediately returns; it creates no timer and no background HTTP poll loop.
pub(crate) fn plan(shell: &mut ShellState, focused: bool, now: u64) -> Vec<Request> {
    let mut state = std::mem::take(&mut shell.slack_lists.viewing);
    state
        .entries
        .retain(|_, lease| lease.scope.account_current(shell));
    let desired = shell
        .slack_lists
        .scope
        .as_ref()
        .filter(|scope| scope.request_ready(shell))
        .zip(shell.slack_lists.view.as_ref())
        .filter(|(_, view)| shell.overlay == Some(Overlay::SlackList) && !view.unavailable)
        .map(|(scope, view)| (scope, view.id.as_str(), view.generation));
    let mut desired_exists = false;
    for (key, lease) in &mut state.entries {
        let next = desired
            .filter(|(scope, id, _)| scope.team == key.0 && *id == key.1)
            .map(|(_, _, visit)| visit);
        desired_exists |= next.is_some();
        if next.is_some() && next != lease.desired && lease.opened {
            lease.renew = true;
        }
        lease.desired = next;
        if lease.desired.is_some() {
            lease.close_due = None;
        }
    }
    if !desired_exists && let Some((scope, id, visit)) = desired {
        state.entries.insert(
            (scope.team.clone(), id.to_owned()),
            Lease {
                scope: scope.clone(),
                id: id.to_owned(),
                generation: next_id(),
                desired: Some(visit),
                opened: false,
                renew: false,
                failed_open: false,
                open_done: false,
                eligible: false,
                in_flight: None,
                next_ping: u64::MAX,
                close_due: None,
                queued_generation: None,
                next_enqueue: 0,
            },
        );
    }
    let mut requests = Vec::new();
    state.entries.retain(|key, lease| {
        let socket = shell
            .core
            .workspaces
            .get(&lease.scope.team)
            .and_then(|workspace| match &workspace.rt {
                RealtimeStatus::Connected(connection) => {
                    Some((workspace.rt_generation, connection))
                }
                _ => None,
            });
        let socket_generation = socket.map(|(generation, _)| generation);
        // A replaced/disconnected socket owns none of its predecessor's frames.
        if lease.queued_generation.is_some() && lease.queued_generation != socket_generation {
            lease.queued_generation = None;
            lease.next_enqueue = 0;
        }
        let subscribe = lease.desired.is_some()
            && !lease.renew
            && lease.opened
            && lease.open_done
            && lease.eligible;
        let release = (!lease.eligible && lease.open_done && lease.desired.is_some())
            || (lease.desired.is_none() && lease.close_due.is_some_and(|due| due <= now));
        if now >= lease.next_enqueue
            && let Some((generation, connection)) = socket
        {
            if subscribe && lease.queued_generation != Some(generation) {
                let accepted = connection
                    .try_send(realtime::temp_channel_sub_frame(&lease.id))
                    .is_ok();
                if accepted {
                    lease.queued_generation = Some(generation);
                } else {
                    lease.next_enqueue = now.saturating_add(ENQUEUE_RETRY_MS);
                }
                diagnostic("subscribe_enqueue", lease, Some(accepted));
            } else if release && lease.queued_generation == Some(generation) {
                let accepted = connection
                    .try_send(realtime::temp_channel_unsub_frame(&lease.id))
                    .is_ok();
                if accepted {
                    lease.queued_generation = None;
                } else {
                    lease.next_enqueue = now.saturating_add(ENQUEUE_RETRY_MS);
                }
                diagnostic("unsubscribe_enqueue", lease, Some(accepted));
            }
        }
        if lease.in_flight.is_none() {
            let kind = if lease.desired.is_some() {
                if lease.renew && lease.opened {
                    Some(Kind::Close)
                } else if !lease.opened || (lease.failed_open && now >= lease.next_ping) {
                    Some(Kind::Open)
                } else if lease.open_done && lease.eligible && now >= lease.next_ping {
                    Some(Kind::Ping { focused })
                } else {
                    None
                }
            } else if lease.opened {
                Some(Kind::Close)
            } else {
                None
            };
            if let Some(kind) = kind {
                let serial = next_id();
                let prior_opened = lease.opened;
                lease.in_flight = Some((serial, kind));
                if kind == Kind::Open {
                    lease.opened = true;
                    lease.open_done = false;
                }
                requests.push(Request {
                    key: key.clone(),
                    lease: lease.generation,
                    serial,
                    kind,
                    visit: lease.desired,
                    prior_opened,
                    scope: lease.scope.clone(),
                    at: now,
                });
            }
        }
        !(lease.desired.is_none()
            && !lease.opened
            && lease.in_flight.is_none()
            && lease.queued_generation.is_none())
    });
    shell.slack_lists.viewing = state;
    requests
}
impl Request {
    fn current(&self, shell: &ShellState) -> bool {
        self.scope.account_current(shell)
            && shell
                .slack_lists
                .viewing
                .entries
                .get(&self.key)
                .is_some_and(|lease| {
                    lease.generation == self.lease
                        && lease.in_flight == Some((self.serial, self.kind))
                })
    }
    /// Sending open/ping requires the exact still-visible visit. Completion
    /// ownership is deliberately broader so already-sent opens can be cleaned up.
    fn dispatchable(&self, shell: &ShellState) -> bool {
        self.current(shell)
            && (self.kind == Kind::Close
                || (self.scope.current(shell)
                    && shell.overlay == Some(Overlay::SlackList)
                    && shell.slack_lists.view.as_ref().is_some_and(|view| {
                        !view.unavailable
                            && view.id == self.key.1
                            && Some(view.generation) == self.visit
                    })))
    }
    fn cancel_unsent(&self, shell: &mut ShellState) {
        if !self.current(shell) {
            return;
        }
        let lease = shell
            .slack_lists
            .viewing
            .entries
            .get_mut(&self.key)
            .unwrap();
        lease.in_flight = None;
        if self.kind == Kind::Open {
            lease.opened = self.prior_opened;
            lease.open_done = self.prior_opened;
        }
    }
    pub(crate) fn spawn(self, state: Signal<ShellState>) {
        dioxus::core::spawn_forever(async move {
            if !self.dispatchable(&state.peek()) {
                #[allow(deprecated)]
                self.cancel_unsent(&mut state.write_silent());
                return;
            }
            let Some(context) = self.scope.request_in_account(&state.peek()) else {
                return;
            };
            let request = match self.kind {
                Kind::Open => {
                    api::files_open(&context.client, &context.session, &self.key.1, self.at)
                }
                Kind::Ping { focused } => {
                    api::files_ping(&context.client, &context.session, &self.key.1, focused)
                }
                Kind::Close => {
                    api::files_close(&context.client, &context.session, &self.key.1, self.at)
                }
            };
            let response = context.transport.execute_file_viewing(request).await;
            let result = match self.kind {
                Kind::Close => response.map(|_| false),
                _ => response.and_then(|value| {
                    api::parse_file_viewing(&value)
                        .map(|response| response.should_subscribe_and_ping)
                }),
            };
            #[allow(deprecated)]
            finish(&mut state.write_silent(), &self, result, now_ms());
        });
    }
}
fn finish(shell: &mut ShellState, request: &Request, result: Result<bool, Error>, now: u64) {
    if !request.current(shell) {
        return;
    }
    let lease = shell
        .slack_lists
        .viewing
        .entries
        .get_mut(&request.key)
        .unwrap();
    lease.in_flight = None;
    match request.kind {
        Kind::Close => {
            lease.opened = false;
            lease.renew = false;
            lease.failed_open = false;
            lease.open_done = false;
            lease.eligible = false;
            lease.next_ping = u64::MAX;
            // Visibility may have changed since the last planner tick. Always
            // arm cleanup here; the next tick cancels it for a live visit.
            lease.close_due = Some(now.saturating_add(UNSUBSCRIBE_MS));
            diagnostic("closed", lease, None);
        }
        Kind::Open => {
            lease.open_done = true;
            lease.eligible = result.as_ref().is_ok_and(|value| *value);
            lease.failed_open = result.as_ref().is_err_and(transient_open_error);
            lease.next_ping = if lease.eligible || lease.failed_open {
                retry_deadline(&result, now)
            } else {
                u64::MAX
            };
            diagnostic(
                if result.is_ok() {
                    "opened"
                } else {
                    "open_failed"
                },
                lease,
                None,
            );
        }
        Kind::Ping { .. } => {
            if matches!(&result, Ok(false))
                || matches!(&result,Err(Error::Api(code)) if code=="file_not_found")
            {
                lease.eligible = false;
            }
            if let Ok(eligible) = result.as_ref() {
                lease.eligible = *eligible;
            }
            lease.next_ping = if lease.eligible {
                retry_deadline(&result, now)
            } else {
                u64::MAX
            };
            diagnostic(
                if result.is_ok() {
                    "pinged"
                } else {
                    "ping_failed"
                },
                lease,
                None,
            );
        }
    }
}
fn transient_open_error(error: &Error) -> bool {
    matches!(
        error,
        Error::Offline(_)
            | Error::Transport(_)
            | Error::RateLimited { .. }
            | Error::HttpStatus {
                status: 408 | 429 | 500..=599,
                ..
            }
    )
}
fn retry_deadline(result: &Result<bool, Error>, now: u64) -> u64 {
    let retry = match result {
        Err(Error::RateLimited { retry_after_secs })
        | Err(Error::HttpStatus {
            retry_after_secs, ..
        }) => retry_after_secs.unwrap_or(30).saturating_mul(1000),
        _ => 0,
    };
    now.saturating_add(PING_MS.max(retry))
}
#[cfg(test)]
mod tests;
