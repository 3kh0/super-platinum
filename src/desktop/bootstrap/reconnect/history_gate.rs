//! Reconnect's optional history obligation and non-mutating unread join policy.
use super::Visit;
use crate::state::ShellState;
use serde_json::{Value, json};

pub(in crate::bootstrap) fn current(s: &ShellState, reconnect: Option<&Visit>) -> bool {
    // The caller retains transport/file or CursorOwner/scope/boundary guards.
    // Ordinary cache warming does not belong to a reconnect request serial.
    reconnect.is_none_or(|visit| visit.current(s))
}

#[derive(Debug, PartialEq, Eq)]
pub(in crate::bootstrap) enum JoinedOpening {
    Waiting,
    Finished,
    Available,
    Rejected,
    TimedOut,
}
pub(in crate::bootstrap) fn joined_opening(
    fetching: &Option<(String, String)>,
    loading: &Option<(String, String)>,
    ticket: &(String, String),
    owned: bool,
    expired: bool,
) -> JoinedOpening {
    if !owned
        || fetching.as_ref().is_some_and(|current| current != ticket)
        || loading.as_ref().is_some_and(|current| current != ticket)
    {
        JoinedOpening::Rejected
    } else if fetching.as_ref() == Some(ticket) {
        if expired {
            JoinedOpening::TimedOut
        } else {
            JoinedOpening::Waiting
        }
    } else if loading.as_ref() != Some(ticket) {
        JoinedOpening::Finished
    } else if expired {
        JoinedOpening::TimedOut
    } else {
        JoinedOpening::Available
    }
}

/// Executed by the disconnected native checker. Uses the production predicates;
/// no fabricated credentials, history responses or timers, and no shell writes.
pub(super) fn fixture_checks(s: &ShellState) -> Result<Value, String> {
    if s.core.session.is_some() || s.core.transport.is_some() {
        return Err("History guard checks require disconnected state".into());
    }
    let mut old = Visit::capture(s);
    old.request = old.request.wrapping_sub(1);
    let live = Visit::capture(s);
    if !current(s, None) || current(s, Some(&old)) || !current(s, Some(&live)) {
        return Err("History optional owner policy failed".into());
    }
    let ticket = ("same-scope".to_owned(), "10.000001".to_owned());
    let another = ("new-scope".to_owned(), "10.000001".to_owned());
    let mut fetching = Some(ticket.clone());
    let mut loading = Some(ticket.clone());
    let mut results = Vec::new();
    let mut check = |name: &str, actual: JoinedOpening, expected: JoinedOpening| {
        if actual != expected {
            return Err(format!("History join failed: {name}"));
        }
        results.push(json!({"case":name,"decision":format!("{actual:?}")}));
        Ok(())
    };
    check(
        "exact-busy",
        joined_opening(&fetching, &loading, &ticket, true, false),
        JoinedOpening::Waiting,
    )?;
    check(
        "retired-owner",
        joined_opening(&fetching, &loading, &ticket, false, false),
        JoinedOpening::Rejected,
    )?;
    check(
        "new-ticket",
        joined_opening(&Some(another), &loading, &ticket, true, false),
        JoinedOpening::Rejected,
    )?;
    check(
        "expired-busy",
        joined_opening(&fetching, &loading, &ticket, true, true),
        JoinedOpening::TimedOut,
    )?;
    if fetching.as_ref() != Some(&ticket) || loading.as_ref() != Some(&ticket) {
        return Err("Join stole old ticket".into());
    }
    fetching = None;
    check(
        "released-old",
        joined_opening(&fetching, &loading, &ticket, true, false),
        JoinedOpening::Available,
    )?;
    fetching = Some(ticket.clone());
    check(
        "replacement-busy",
        joined_opening(&fetching, &loading, &ticket, true, false),
        JoinedOpening::Waiting,
    )?;
    fetching = None;
    loading = None;
    check(
        "old-success",
        joined_opening(&fetching, &loading, &ticket, true, false),
        JoinedOpening::Finished,
    )?;
    check(
        "expired-released",
        joined_opening(&fetching, &Some(ticket.clone()), &ticket, true, true),
        JoinedOpening::TimedOut,
    )?;
    Ok(
        json!({"ok":true,"ordinarySurvivesSerial":true,"oldReconnectRejected":true,"currentReconnectAccepted":true,"ticketUntouchedOnTimeout":true,"cases":results,"scope":"Shared production optional-owner and unread-join predicates; synthetic tickets, no history/network/ACK execution"}),
    )
}
