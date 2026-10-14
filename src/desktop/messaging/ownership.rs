//! A network result belongs to the account visit that prepared the request.
use std::sync::Arc;

use dioxus::prelude::{ReadableExt, Signal, WritableExt};
use super_platinum_core::slack::{Error, SlackClient, Transport, api, models::SentMessage};

use crate::state::{PendingSend, ShellState};

pub(super) mod upload_pages;

pub(super) struct SendVisit {
    account: Option<String>,
    epoch: u64,
    credentials: u64,
    transport: Arc<Transport>,
}

impl SendVisit {
    pub(super) fn queued_is_current(shell: &ShellState, pending: &PendingSend) -> bool {
        shell.core.active_account == pending.account
            && shell.core.account_epoch == pending.account_epoch
    }

    pub(super) fn capture(shell: &ShellState, transport: &Arc<Transport>) -> Self {
        Self {
            account: shell.core.active_account.clone(),
            epoch: shell.core.account_epoch,
            credentials: super::upload_journal::network::credentials(shell),
            transport: transport.clone(),
        }
    }

    pub(super) fn current(&self, shell: &ShellState) -> bool {
        shell.core.active_account == self.account
            && shell.core.account_epoch == self.epoch
            && super::upload_journal::network::credentials(shell) == self.credentials
            && shell
                .core
                .transport
                .as_ref()
                .is_some_and(|current| Arc::ptr_eq(current, &self.transport))
    }
}

// A timeout, interrupted response, decoding error, or server failure does not
// establish that Slack rejected a write. Retain the optimistic payload instead
// of presenting it as safe to retry. There is deliberately no automatic replay.
pub(super) fn outcome_uncertain(error: &Error) -> bool {
    matches!(error, Error::Offline(_) | Error::Transport(_))
        || matches!(error, Error::Api(code) if matches!(code.as_str(), "internal_error" | "fatal_error"))
        || matches!(error, Error::HttpStatus { status, .. } if *status >= 500 || *status == 408)
}

fn confirms_text(sent: &SentMessage, pending: &PendingSend) -> bool {
    let valid_ts = sent.ts.split_once('.').is_some_and(|(seconds, fraction)| {
        seconds.parse::<u64>().is_ok_and(|seconds| seconds > 0)
            && seconds.bytes().all(|byte| byte.is_ascii_digit())
            && fraction.len() == 6
            && fraction.bytes().all(|byte| byte.is_ascii_digit())
    });
    valid_ts
        && sent.channel == pending.channel
        && sent.message.ts.as_ref().is_none_or(|ts| ts == &sent.ts)
        && sent
            .message
            .channel
            .as_ref()
            .is_none_or(|channel| channel == &pending.channel)
        && sent.message.thread_ts.as_ref().is_none_or(|root| {
            pending
                .thread_ts
                .as_ref()
                .map_or(root == &sent.ts, |expected| root == expected)
        })
        && sent
            .message
            .client_msg_id
            .as_ref()
            .is_none_or(|id| id == &pending.client_msg_id)
        && pending.thread_ts.as_ref() != Some(&sent.ts)
}

/// Return true only when this visit confirmed a send and may persist its cache.
pub(super) fn apply_text_result(
    shell: &mut ShellState,
    visit: &SendVisit,
    pending: &PendingSend,
    label: &str,
    result: Result<SentMessage, Error>,
) -> bool {
    if !visit.current(shell) || !SendVisit::queued_is_current(shell, pending) {
        return false;
    }
    // A decoded object alone is not a receipt for this destination. Preserve
    // the original payload when a successful response contradicts the request.
    // The client id is sent unchanged; a contradictory echoed identity is not
    // a receipt for this pending operation.
    let result = result.and_then(|sent| {
        if confirms_text(&sent, pending) {
            Ok(sent)
        } else {
            Err(Error::Transport(
                "Slack did not confirm this message's timestamp and destination".into(),
            ))
        }
    });
    let message = match result {
        Ok(sent) => Some(super_platinum_core::slack::models::Message {
            ts: Some(sent.ts),
            channel: Some(pending.channel.clone()),
            thread_ts: pending.thread_ts.clone(),
            subtype: if pending.reply_broadcast {
                Some("thread_broadcast".into())
            } else {
                sent.message.subtype.clone()
            },
            ..sent.message
        }),
        Err(error) => {
            if outcome_uncertain(&error) {
                shell.pending_delivery.insert(
                    pending.optimistic_ts.clone(),
                    crate::state::PendingDeliveryStatus::Unconfirmed,
                );
                shell.show_toast(format!(
                    "{label} delivery is unconfirmed; check the conversation before sending again"
                ));
                return false;
            }
            shell.pending_delivery.insert(
                pending.optimistic_ts.clone(),
                crate::state::PendingDeliveryStatus::Failed,
            );
            shell.report_failure(
                &error,
                &format!("{label} not sent; its text remains in the conversation"),
                || format!("{label} not sent; its text remains in the conversation ({error})"),
            );
            return false;
        }
    };
    let confirmed = message.is_some();
    if pending.reply_broadcast && pending.thread_ts.is_some() {
        if let Some(message) = message.as_ref() {
            if let Some(workspace) = shell.core.workspaces.get_mut(&pending.team) {
                let messages = workspace
                    .messages
                    .entry(pending.channel.clone())
                    .or_default();
                if let Some(ts) = &message.ts {
                    if !messages.was_deleted(ts)
                        && !messages.messages.iter().any(|m| m.ts.as_ref() == Some(ts))
                    {
                        messages.upsert(message.clone());
                    }
                }
            }
        }
    }
    let messages = if let Some(root) = pending.thread_ts.as_ref() {
        shell
            .core
            .threads
            .get_mut(&(pending.team.clone(), pending.channel.clone(), root.clone()))
    } else {
        shell
            .core
            .workspaces
            .get_mut(&pending.team)
            .and_then(|workspace| workspace.messages.get_mut(&pending.channel))
    };
    if let Some(messages) = messages {
        if let Some(message) = message {
            messages.confirm(&pending.client_msg_id, message);
        } else {
            messages.remove(&pending.optimistic_ts);
        }
    }
    shell.pending_delivery.remove(&pending.optimistic_ts);
    if pending.thread_ts.is_some() {
        shell.refresh_thread_from_core();
    }
    shell.refresh_from_core();
    confirmed
}

#[derive(Clone, Debug)]
pub(super) struct UploadReadback {
    pub team: String,
    pub channel: String,
    pub thread_ts: Option<String>,
    pub reply_broadcast: bool,
    pub sender: String,
    pub file_ids: Vec<String>,
}

impl UploadReadback {
    fn in_destination(&self, message: &super_platinum_core::slack::models::Message) -> bool {
        if message
            .channel
            .as_deref()
            .is_some_and(|channel| channel != self.channel)
        {
            return false;
        }
        match &self.thread_ts {
            Some(root) => {
                message.ts.as_ref() == Some(root) || message.thread_ts.as_ref() == Some(root)
            }
            None => super_platinum_core::state::is_channel_timeline_visible(message),
        }
    }
    pub(super) fn confirms(&self, message: &super_platinum_core::slack::models::Message) -> bool {
        let valid_ts = message.ts.as_deref().is_some_and(|ts| {
            ts.split_once('.').is_some_and(|(seconds, fraction)| {
                seconds.parse::<u64>().is_ok_and(|seconds| seconds > 0)
                    && seconds.bytes().all(|byte| byte.is_ascii_digit())
                    && fraction.len() == 6
                    && fraction.bytes().all(|byte| byte.is_ascii_digit())
            })
        });
        let ids: std::collections::BTreeSet<_> = message
            .files
            .iter()
            .filter_map(|file| file.id.as_deref())
            .collect();
        let expected: std::collections::BTreeSet<_> =
            self.file_ids.iter().map(String::as_str).collect();
        valid_ts
            && (!self.reply_broadcast
                || matches!(
                    message.subtype.as_deref(),
                    Some("thread_broadcast" | "reply_broadcast")
                ))
            && self.in_destination(message)
            && message.user.as_ref() == Some(&self.sender)
            && match &self.thread_ts {
                Some(root) => {
                    message.thread_ts.as_ref() == Some(root) && message.ts.as_ref() != Some(root)
                }
                None => message.thread_ts.is_none() || message.thread_ts == message.ts,
            }
            && !expected.is_empty()
            && expected.len() == self.file_ids.len()
            && ids.len() == message.files.len()
            && ids == expected
    }
}

// Completion proves which files were finalized, not their visible message ts.
// Keep the optimistic payload until the exact sender/destination/files appear.
pub(super) async fn refresh_upload_history(
    state: &mut Signal<ShellState>,
    visit: &SendVisit,
    client: &SlackClient,
    session: &super_platinum_core::config::WorkspaceSession,
    expected: &UploadReadback,
) -> bool {
    if !visit.current(&state.peek()) {
        return false;
    }
    let read_state = *state;
    let scan = upload_pages::find(
        expected,
        |cursor| {
            Box::pin(async move {
                if let Some(root) = &expected.thread_ts {
                    api::fetch_replies(
                        &visit.transport,
                        client,
                        session,
                        api::RepliesArgs {
                            channel: expected.channel.clone(),
                            ts: root.clone(),
                            cursor,
                            limit: Some(100),
                            ..Default::default()
                        },
                    )
                    .await
                } else {
                    api::fetch_history(
                        &visit.transport,
                        client,
                        session,
                        api::HistoryArgs {
                            channel: expected.channel.clone(),
                            cursor,
                            limit: Some(100),
                            ..Default::default()
                        },
                    )
                    .await
                }
            })
        },
        || {
            let shell = read_state.peek();
            visit.current(&shell)
                && shell
                    .core
                    .workspaces
                    .get(&expected.team)
                    .is_some_and(|workspace| workspace.self_user_id == expected.sender)
        },
    );
    // Bound the whole scan, including transport offline holds/backoff. Cancelling
    // these reads never cancels or replays the already-finished upload mutation.
    let result = tokio::time::timeout(std::time::Duration::from_secs(30), scan)
        .await
        .unwrap_or_else(|_| {
            Err(Error::Transport(
                "Upload readback exceeded its time budget".into(),
            ))
        });
    apply_upload_history(&mut state.write(), visit, expected, result)
}

fn apply_upload_history(
    shell: &mut ShellState,
    visit: &SendVisit,
    expected: &UploadReadback,
    result: Result<super_platinum_core::slack::models::HistoryPage, Error>,
) -> bool {
    if !visit.current(shell)
        || shell
            .core
            .workspaces
            .get(&expected.team)
            .is_none_or(|workspace| workspace.self_user_id != expected.sender)
    {
        return false;
    }
    match result {
        Ok(page) => {
            let confirmed = ingest_upload_page(shell, expected, page);
            shell.refresh_from_core();
            if !confirmed {
                shell.show_toast(
                    "Upload delivery is unconfirmed; the exact file message has not appeared yet",
                );
            }
            confirmed
        }
        Err(error) => {
            shell.report_failure(
                &error,
                "Upload delivery is unconfirmed; checking it needs a connection",
                || "Upload delivery is unconfirmed; conversation refresh is unavailable".into(),
            );
            false
        }
    }
}

#[cfg(test)]
#[path = "ownership_tests.rs"]
mod tests;

/// Only exact canonical matches are merged; existing rows and tombstones win.
/// The caller must validate its immutable account/read ownership first.
pub(super) fn ingest_upload_page(
    shell: &mut ShellState,
    expected: &UploadReadback,
    page: super_platinum_core::slack::models::HistoryPage,
) -> bool {
    // Deletions are retained in the original channel bag even for a
    // never-loaded row/thread. Filter before declaring success as well
    // as before inserting, so a stale page cannot retire the payload.
    let channel_messages = shell.core.workspaces[&expected.team]
        .messages
        .get(&expected.channel);
    let thread_messages = expected.thread_ts.as_ref().and_then(|root| {
        shell.core.threads.get(&(
            expected.team.clone(),
            expected.channel.clone(),
            root.clone(),
        ))
    });
    let matching = page
        .messages
        .into_iter()
        .filter(|message| {
            expected.confirms(message)
                && !thread_messages.is_some_and(|messages| {
                    message
                        .ts
                        .as_deref()
                        .is_some_and(|ts| messages.was_deleted(ts))
                })
                && !channel_messages.is_some_and(|messages| {
                    message
                        .ts
                        .as_deref()
                        .is_some_and(|ts| messages.was_deleted(ts))
                })
        })
        .collect::<Vec<_>>();
    let confirmed = !matching.is_empty();
    if expected.reply_broadcast && expected.thread_ts.is_some() {
        let channel = shell
            .core
            .workspaces
            .get_mut(&expected.team)
            .unwrap()
            .messages
            .entry(expected.channel.clone())
            .or_default();
        for message in &matching {
            if !channel
                .messages
                .iter()
                .any(|existing| existing.ts == message.ts)
            {
                channel.upsert(super_platinum_core::state::visible_message(message.clone()));
            }
        }
    }
    let messages = if let Some(root) = &expected.thread_ts {
        shell
            .core
            .threads
            .entry((
                expected.team.clone(),
                expected.channel.clone(),
                root.clone(),
            ))
            .or_default()
    } else {
        shell
            .core
            .workspaces
            .get_mut(&expected.team)
            .unwrap()
            .messages
            .entry(expected.channel.clone())
            .or_default()
    };
    for message in matching {
        // This page only verifies this upload. Unrelated rows may be
        // older than intervening realtime edits or deletion events;
        // existing upload rows may have been edited in the meantime.
        if expected.confirms(&message)
            && !messages
                .messages
                .iter()
                .any(|existing| existing.ts == message.ts)
        {
            messages.upsert(super_platinum_core::state::visible_message(message));
        }
    }
    confirmed
}
