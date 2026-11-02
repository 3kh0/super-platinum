//! Coherent current native shape only; never a Root eligibility/namespace grant.
use crate::state::ShellState;
use super_platinum_core::slack::models::Message;

fn copies(s: &ShellState, team: &str, channel: &str, ts: &str, mut visit: impl FnMut(&Message)) {
    if let Some(bag) = s
        .core
        .workspaces
        .get(team)
        .and_then(|w| w.messages.get(channel))
    {
        for m in &bag.messages {
            if m.ts.as_deref() == Some(ts) {
                visit(m);
            }
        }
    }
    for ((t, c, _), bag) in &s.core.threads {
        if t == team && c == channel {
            for m in &bag.messages {
                if m.ts.as_deref() == Some(ts) {
                    visit(m);
                }
            }
        }
    }
    if s.core
        .threads_view
        .matches_scope(s.core.account_epoch, team)
    {
        for item in &s.core.threads_view.items {
            if item.channel().is_some_and(|c| c == channel) {
                for m in std::iter::once(&item.root_msg)
                    .chain(&item.unread_replies)
                    .chain(&item.latest_replies)
                {
                    if m.ts.as_deref() == Some(ts) {
                        visit(m);
                    }
                }
            }
        }
    }
}
pub(crate) fn known_reply(s: &ShellState, team: &str, channel: &str, ts: &str) -> bool {
    let mut found = false;
    copies(s, team, channel, ts, |m| {
        found |= m
            .thread_ts
            .as_deref()
            .is_some_and(|root| !root.is_empty() && root != ts);
    });
    found
}
pub(super) fn explicit_reply(s: &ShellState, team: &str, channel: &str, ts: &str) -> bool {
    let mut any = false;
    let mut coherent = true;
    // The owned string avoids retaining a reference into a callback borrow.
    let mut original = String::new();
    copies(s, team, channel, ts, |m| {
        any = true;
        let Some(thread) = m.thread_ts.as_deref().filter(|r| !r.is_empty() && *r != ts) else {
            coherent = false;
            return;
        };
        if m.channel.as_deref().is_some_and(|c| c != channel) {
            coherent = false;
        }
        if original.is_empty() {
            original = thread.into();
        } else if original != thread {
            coherent = false;
        }
    });
    any && coherent
}
