use super::eligibility::Target;
use crate::{
    realtime::activity_counts::CursorOwner,
    state::{MainView, ShellState},
};
use dioxus::prelude::*;
use std::sync::Arc;
use super_platinum_core::{
    config::WorkspaceSession,
    domain::{ReadTarget, ThreadKey},
    slack::{SlackClient, Transport, api},
    state::{
        ChannelEscape, LoadedReadTimestamp, ThreadEscape, channel_escape,
        latest_channel_read_target, thread_escape,
    },
};

#[derive(Clone, Debug, PartialEq, Eq)]
enum Decision {
    Channel {
        team: String,
        channel: String,
        ts: String,
        previous: Option<String>,
        revision: u64,
    },
    Thread {
        key: ThreadKey,
        ts: String,
        revision: u64,
    },
    ClearChannel {
        team: String,
        channel: String,
        cursor: String,
        repair: bool,
        revision: u64,
    },
    ClearThread,
    CloseThread,
    ClosePrimary,
}

struct Authority {
    owner: CursorOwner,
    transport: Arc<Transport>,
    client: SlackClient,
    session: WorkspaceSession,
}

pub(super) struct Prepared {
    epoch: u64,
    view_key: String,
    target: Target,
    decision: Decision,
    authority: Option<Authority>,
}

fn decision(shell: &ShellState, target: &Target) -> Option<Decision> {
    if super::bulk::blocks_reads(shell) || super::bulk::dialog_open(shell) {
        return None;
    }
    let team = shell.core.active_team.as_ref()?;
    let channel = shell.core.active_channel.as_ref()?;
    let workspace = shell.core.workspaces.get(team)?;
    match target {
        Target::Primary(scope) => {
            if scope != &shell.timeline_scope()
                || !shell.transcript_visible()
                || super::busy_channel(shell)
                || crate::unread::busy(shell, channel)
            {
                return None;
            }
            let read_target = ReadTarget::Conversation {
                team: team.clone(),
                channel: channel.clone(),
            };
            if shell
                .core
                .pending_marks
                .iter()
                .any(|(target, _)| target == &read_target)
            {
                return None;
            }
            let meta = workspace.channels.get(channel)?;
            if !(meta.is_im
                || meta.is_mpim
                || meta
                    .extra
                    .get("is_member")
                    .and_then(serde_json::Value::as_bool)
                    == Some(true))
            {
                return None;
            }
            let bag = workspace.messages.get(channel);
            let metadata = meta.extra.get("latest").and_then(|v| {
                v.as_str()
                    .or_else(|| v.get("ts").and_then(serde_json::Value::as_str))
            });
            let metadata = latest_channel_read_target(
                metadata,
                bag.and_then(|bag| bag.latest.as_deref())
                    .into_iter()
                    .map(|ts| LoadedReadTimestamp { ts, pending: false }),
            );
            let latest = latest_channel_read_target(
                metadata,
                bag.into_iter().flat_map(|bag| {
                    bag.messages.iter().filter_map(move |m| {
                        Some(LoadedReadTimestamp {
                            ts: m.ts.as_deref()?,
                            pending: bag.is_pending(m.ts.as_deref()?),
                        })
                    })
                }),
            );
            let count = if meta.unread_count.is_some()
                || meta.unread_count_display.is_some()
                || meta.has_unreads
            {
                Some(u64::from(bag.map_or_else(
                    || meta.unread_count.or(meta.unread_count_display).unwrap_or(1),
                    |bag| bag.unread_count,
                )))
            } else {
                None
            };
            let previous = workspace.read_cursor(channel);
            let revision = workspace.read_revision(channel);
            // An eligible unread conversation with missing latest metadata is
            // unresolved, not the already-read close fallback.
            if latest.is_none()
                && (count.is_some_and(|n| n > 0) || shell.divider_at(channel).is_some())
            {
                return None;
            }
            match channel_escape(count, shell.divider_at(channel).is_some(), previous, latest) {
                ChannelEscape::Mark { ts } => Some(Decision::Channel {
                    team: team.clone(),
                    channel: channel.clone(),
                    ts: ts.into(),
                    previous: previous.map(str::to_owned),
                    revision,
                }),
                ChannelEscape::AlreadyAtLatest {
                    repair_unread_count,
                } => Some(Decision::ClearChannel {
                    team: team.clone(),
                    channel: channel.clone(),
                    cursor: previous?.into(),
                    repair: repair_unread_count,
                    revision,
                }),
                ChannelEscape::NoRead => matches!(
                    shell.main_view,
                    MainView::Dms | MainView::Activity | MainView::Later
                )
                .then_some(Decision::ClosePrimary),
            }
        }
        Target::Thread(scope) => {
            if scope != &shell.thread_scope()
                || shell.thread_loading
                || shell.thread_error.is_some()
                || super::busy_thread(shell)
                || crate::thread_unread::busy(shell)
            {
                return None;
            }
            let key = crate::thread_unread::key(shell)?;
            let target = ReadTarget::Thread {
                team: key.0.clone(),
                channel: key.1.clone(),
                root_ts: key.2.clone(),
            };
            if shell
                .core
                .pending_marks
                .iter()
                .any(|(pending, _)| pending == &target)
            {
                return None;
            }
            let bag = shell.core.threads.get(&key)?;
            // B2 dbCg2518757 -> S2 xmpJ3856198/dUDN3856314 rejects temp ts;
            // S2 iub5.aw3838502 -> w9uO3861208 rejects is_ephemeral.
            let maximum = latest_channel_read_target(
                None,
                bag.messages
                    .iter()
                    .filter(|m| {
                        m.extra.get("is_ephemeral").is_none_or(|v| {
                            matches!(v, serde_json::Value::Null | serde_json::Value::Bool(false))
                        })
                    })
                    .filter_map(|m| {
                        Some(LoadedReadTimestamp {
                            ts: m.ts.as_deref()?,
                            pending: bag.is_pending(m.ts.as_deref()?),
                        })
                    }),
            );
            let subscribed = crate::thread_subscription::active(shell);
            let initial = crate::thread_unread::shortcut_initial_cursor(shell);
            let cursor = shell.core.thread_cursor(&key);
            if subscribed == Some(true)
                && (maximum.is_none()
                    || cursor.is_some_and(|ts| latest_channel_read_target(Some(ts), []).is_none())
                    || initial.is_some_and(|ts| latest_channel_read_target(Some(ts), []).is_none()))
            {
                return None;
            }
            match thread_escape(subscribed, initial, cursor, maximum) {
                ThreadEscape::Mark { ts } => Some(Decision::Thread {
                    key: key.clone(),
                    ts: ts.into(),
                    revision: shell.core.thread_read_revision(&key),
                }),
                ThreadEscape::ClearDivider => Some(Decision::ClearThread),
                ThreadEscape::NoRead => Some(Decision::CloseThread),
            }
        }
        Target::Workspace => None,
    }
}

pub(super) fn prepare(shell: &ShellState, focus: &str, scope: Option<&str>) -> Option<Prepared> {
    let target = if focus == "thread" {
        Target::Thread(scope?.into())
    } else {
        Target::Primary(shell.timeline_scope())
    };
    let decision = decision(shell, &target)?;
    if matches!(decision, Decision::ClosePrimary) && focus != "primary" {
        return None;
    }
    let authority = if matches!(decision, Decision::Channel { .. } | Decision::Thread { .. })
        && !super::fixture::owner(shell)
    {
        let transport = shell.core.transport.clone()?;
        let session = shell
            .core
            .session
            .as_ref()?
            .workspaces
            .get(shell.core.active_team.as_ref()?)?
            .clone();
        Some(Authority {
            owner: CursorOwner::capture(&shell.core, &transport, &session)?,
            transport,
            client: shell.core.client.clone(),
            session,
        })
    } else {
        None
    };
    Some(Prepared {
        epoch: shell.core.account_epoch,
        view_key: super::key(shell),
        target,
        decision,
        authority,
    })
}

pub(super) async fn execute(mut state: Signal<ShellState>, prepared: Prepared) {
    {
        let shell = state.peek();
        if prepared.view_key != super::key(&shell)
            || !crate::read_visibility::focused()
            || decision(&shell, &prepared.target).as_ref() != Some(&prepared.decision)
            || prepared
                .authority
                .as_ref()
                .is_some_and(|authority| !authority.owner.current(&shell.core))
        {
            return;
        }
    }
    let (target, ts) = match &prepared.decision {
        Decision::Channel {
            team, channel, ts, ..
        } => (
            ReadTarget::Conversation {
                team: team.clone(),
                channel: channel.clone(),
            },
            ts.clone(),
        ),
        Decision::Thread { key, ts, .. } => (
            ReadTarget::Thread {
                team: key.0.clone(),
                channel: key.1.clone(),
                root_ts: key.2.clone(),
            },
            ts.clone(),
        ),
        other => {
            let mut shell = state.write();
            match other {
                Decision::ClearChannel {
                    team,
                    channel,
                    cursor,
                    repair,
                    revision,
                } => {
                    if *repair {
                        crate::realtime::activity_counts::confirm_channel_read(
                            &mut shell.core,
                            team,
                            channel,
                            cursor,
                            Some(cursor),
                            *revision,
                        );
                    }
                    shell.unread_anchor = None;
                    crate::unread::release(&mut shell);
                    shell.refresh_from_core();
                }
                Decision::ClearThread => {
                    crate::thread_unread::clear_shortcut_divider(&mut shell);
                    shell.refresh_from_core();
                }
                Decision::CloseThread => {
                    if matches!(shell.main_view, MainView::Activity | MainView::Later) {
                        shell.forget_surface();
                    } else {
                        shell.close_thread();
                    }
                }
                Decision::ClosePrimary => shell.forget_surface(),
                _ => {}
            }
            return;
        }
    };
    // A disconnected fixture never dispatches a live request. Production
    // decisions stay reusable by the forthcoming fixture admission ledger.
    if crate::fixture::is_fixture() {
        let mut shell = state.write();
        if super::fixture::record_mark(&mut shell, &target, &ts) {
            apply_ack(
                &mut shell,
                &prepared.decision,
                &prepared.target,
                &prepared.view_key,
            );
        }
        return;
    }
    let Some(authority) = prepared.authority else {
        return;
    };
    let request = match &prepared.decision {
        Decision::Channel { channel, .. } => api::conversations_mark(
            &authority.client,
            &authority.session,
            channel.clone(),
            ts.clone(),
        ),
        Decision::Thread { key, .. } => api::subscriptions_thread_mark(
            &authority.client,
            &authority.session,
            key.1.clone(),
            key.2.clone(),
            ts.clone(),
        ),
        _ => return,
    };
    let pending = (prepared.epoch, target.clone());
    let account = state.peek().core.active_account.clone().unwrap_or_default();
    let user = authority.session.user_id.clone();
    static NEXT_TICKET: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let ticket = NEXT_TICKET.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    {
        let mut shell = state.write();
        if shell.read_shortcuts.pending.contains_key(&pending) {
            return;
        }
        if !super::receipts::begin(&mut shell, &target, &ts, ticket) {
            return;
        }
        shell.read_shortcuts.pending.insert(pending.clone(), ticket);
        shell
            .core
            .pending_marks
            .insert((target.clone(), ts.clone()));
    }
    let result = authority.transport.execute_read_shortcuts(request).await;
    let mut shell = state.write();
    let current =
        authority.owner.current(&shell.core) && shell.core.account_epoch == prepared.epoch;
    let resolved = (result.is_ok() && current)
        || result
            .as_ref()
            .err()
            .is_some_and(api::read_shortcut_definitely_rejected);
    if !super::receipts::finish(&mut shell, &account, &user, &target, ticket, resolved) {
        return;
    }
    if !current {
        return;
    }
    match result {
        Ok(_) => {
            apply_ack(
                &mut shell,
                &prepared.decision,
                &prepared.target,
                &prepared.view_key,
            );
        }
        Err(error) => {
            shell.report_failure(&error, "Read status cannot change while offline.", || {
                if api::read_shortcut_definitely_rejected(&error) {
                    "Could not mark as read. Please try again.".into()
                } else {
                    "Read status is unconfirmed. Refresh before trying again.".into()
                }
            })
        }
    }
    drop(shell);
    crate::bootstrap::persist_workspace(&state, &authority.session.team_id);
}

fn apply_ack(shell: &mut ShellState, decision: &Decision, target: &Target, view_key: &str) {
    let accepted = match decision {
        Decision::Channel {
            team,
            channel,
            ts,
            previous,
            revision,
        } => crate::realtime::activity_counts::confirm_channel_read(
            &mut shell.core,
            team,
            channel,
            ts,
            previous.as_deref(),
            *revision,
        ),
        Decision::Thread { key, ts, revision } => {
            crate::realtime::activity_counts::confirm_thread_read(
                &mut shell.core,
                key,
                ts,
                *revision,
            )
        }
        _ => false,
    };
    if accepted && view_key == super::key(shell) {
        match target {
            Target::Primary(_) => {
                shell.unread_anchor = None;
                crate::unread::release(shell);
            }
            Target::Thread(_) => crate::thread_unread::clear_shortcut_divider(shell),
            _ => {}
        }
    }
    shell.refresh_from_core();
}

#[cfg(test)]
mod tests;
