use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

use dioxus::prelude::{ReadableExt, Signal, WritableExt};
use super_platinum_core::domain::{ComposerAttachment, PendingFileMessage};
use super_platinum_core::slack::api;

use crate::bootstrap::{credentials, persist_workspace};
use crate::state::{PendingSend, ShellState};

pub(crate) mod delivery_check;
mod outcomes;
mod ownership;
pub(crate) mod thread_broadcast_check;
pub(crate) mod upload_journal;
pub(crate) mod upload_recovery;
pub(crate) use outcomes::fixture as send_outcomes_fixture;
use ownership::SendVisit;

pub async fn send_composer(mut state: Signal<ShellState>) {
    let unavailable = state.peek().composer_unavailable();
    if let Some(reason) = unavailable {
        state.write().show_toast(reason);
        return;
    }
    if crate::drafts::send::intercept(state) {
        return;
    }
    if !state.read().core.composer_attachments.is_empty() {
        send_attachments(state).await;
        return;
    }
    let pending = state.write().queue_composer();
    // The box renders `initial_value`, so clearing it is our job.
    crate::view::composer::set_field_text("channel-composer", &state.read().core.composer.text);
    let Some(pending) = pending else {
        return;
    };
    send_pending(state, pending, "Message").await;
}

pub async fn send_thread_composer(mut state: Signal<ShellState>) {
    let unavailable = state.peek().composer_unavailable();
    if let Some(reason) = unavailable {
        state.write().show_toast(reason);
        return;
    }
    if crate::drafts::send::intercept_target(state, crate::state::ComposerTarget::Thread) {
        return;
    }
    if !state.read().core.thread_composer_attachments.is_empty() {
        send_thread_attachments(state).await;
        return;
    }
    let pending = state.write().queue_thread_composer();
    crate::view::composer::set_field_text(
        "thread-composer",
        &state.read().core.thread_composer.text,
    );
    let Some(pending) = pending else {
        return;
    };
    send_pending(state, pending, "Thread reply").await;
}

async fn send_pending(mut state: Signal<ShellState>, pending: PendingSend, label: &str) {
    if !SendVisit::queued_is_current(&state.peek(), &pending) {
        return;
    }
    let Some((transport, client, workspaces)) = credentials(&state) else {
        state
            .write()
            .show_toast("Message is queued until Slack reconnects");
        return;
    };
    let visit = SendVisit::capture(&state.peek(), &transport);
    let Some(workspace_session) = workspaces
        .into_iter()
        .find(|workspace| workspace.team_id == pending.team)
    else {
        state
            .write()
            .show_toast("Active workspace session is unavailable");
        return;
    };
    let result = api::send_message_with_broadcast(
        &transport,
        &client,
        &workspace_session,
        pending.channel.clone(),
        pending.text.clone(),
        pending.thread_ts.clone(),
        pending.client_msg_id.clone(),
        pending.reply_broadcast,
    )
    .await;
    let persist = ownership::apply_text_result(&mut state.write(), &visit, &pending, label, result);
    if persist {
        persist_workspace(&state, &pending.team);
    }
}

async fn send_attachments(state: Signal<ShellState>) {
    start_file_upload(state, false).await;
}

async fn send_thread_attachments(state: Signal<ShellState>) {
    start_file_upload(state, true).await;
}

async fn start_file_upload(mut state: Signal<ShellState>, in_thread: bool) {
    let Some((transport, client, workspaces)) = credentials(&state) else {
        state
            .write()
            .show_toast("File upload unavailable while offline");
        return;
    };
    let visit = SendVisit::capture(&state.peek(), &transport);
    let prepared = {
        let mut shell = state.write();
        let (Some(team), Some(channel)) = (
            shell.core.active_team.clone(),
            shell.core.active_channel.clone(),
        ) else {
            return;
        };
        let thread_ts = if in_thread {
            shell.thread_root.clone()
        } else {
            None
        };
        let reply_broadcast = in_thread && shell.core.thread_composer.reply_broadcast;
        let text = if in_thread {
            shell.core.thread_composer.text.trim().to_owned()
        } else {
            shell.core.composer.text.trim().to_owned()
        };
        let Some(self_user) = shell
            .core
            .workspaces
            .get(&team)
            .map(|workspace| workspace.self_user_id.clone())
        else {
            shell.show_toast("File upload unavailable for this workspace");
            return;
        };
        let attachments_empty = if in_thread {
            shell.core.thread_composer_attachments.is_empty()
                || shell
                    .core
                    .thread_composer_attachments
                    .iter()
                    .any(|attachment| attachment.uploading)
        } else {
            shell.core.composer_attachments.is_empty()
                || shell
                    .core
                    .composer_attachments
                    .iter()
                    .any(|attachment| attachment.uploading)
        };
        if attachments_empty {
            return;
        }

        let upload_cancel = Arc::new(AtomicBool::new(false));
        let mut taken = if in_thread {
            std::mem::take(&mut shell.core.thread_composer_attachments)
        } else {
            std::mem::take(&mut shell.core.composer_attachments)
        };
        let files: Vec<api::UploadFile> = taken
            .iter_mut()
            .map(|attachment| {
                attachment.uploading = true;
                attachment.upload_started = Some(Instant::now());
                attachment.upload_cancel = Some(upload_cancel.clone());
                let progress = Arc::new(AtomicU64::new(0));
                attachment.upload_progress = Some(progress.clone());
                api::UploadFile {
                    path: attachment.path.clone(),
                    filename: attachment.name.clone(),
                    progress,
                }
            })
            .collect();

        let (client_msg_id, message_ts) = ShellState::optimistic_identity("super-platinum-file");
        let pending_message = super_platinum_core::slack::models::Message {
            user: Some(self_user),
            kind: Some("message".into()),
            ts: Some(message_ts.clone()),
            client_msg_id: Some(client_msg_id.clone()),
            text: Some(text.clone()),
            channel: Some(channel.clone()),
            thread_ts: thread_ts.clone(),
            ..Default::default()
        };

        match thread_ts.as_ref() {
            Some(root) => {
                let messages = shell
                    .core
                    .threads
                    .entry((team.clone(), channel.clone(), root.clone()))
                    .or_default();
                messages.upsert(pending_message);
                messages.pending.push(message_ts.clone());
                shell
                    .message_arrivals
                    .insert(message_ts.clone(), Instant::now());
                shell.core.thread_composer = super_platinum_core::ComposerState::default();
                shell.refresh_thread_from_core();
            }
            None => {
                if let Some(workspace) = shell.core.workspaces.get_mut(&team) {
                    let messages = workspace.messages.entry(channel.clone()).or_default();
                    messages.upsert(pending_message);
                    messages.pending.push(message_ts.clone());
                }
                shell
                    .message_arrivals
                    .insert(message_ts.clone(), Instant::now());
                shell.core.composer = super_platinum_core::ComposerState::default();
                shell.refresh_from_core();
            }
        }

        shell.core.pending_file_messages.push(PendingFileMessage {
            finalized: None,
            reply_broadcast,
            team: team.clone(),
            channel: channel.clone(),
            thread_ts: thread_ts.clone(),
            message_ts: message_ts.clone(),
            client_msg_id: client_msg_id.clone(),
            text: text.clone(),
            attachments: taken,
        });
        shell.core.pending_scroll_to = Some((
            channel.clone(),
            super_platinum_core::domain::PendingScrollTarget::Latest,
        ));
        shell.stick_to_bottom = true;

        shell.checkpoint_composers();
        PreparedUpload {
            reply_broadcast,
            team,
            channel,
            thread_ts,
            message_ts,
            client_msg_id,
            text,
            files,
            upload_cancel,
        }
    };
    // An upload consumes the caption the same way a send does.
    crate::view::composer::set_field_text(
        if in_thread {
            "thread-composer"
        } else {
            "channel-composer"
        },
        "",
    );

    let Some(workspace_session) = workspaces
        .into_iter()
        .find(|workspace| workspace.team_id == prepared.team)
    else {
        restore_failed_upload(
            &mut state,
            &visit,
            &prepared.client_msg_id,
            prepared.thread_ts.as_ref(),
        );
        state
            .write()
            .show_toast("Active workspace session is unavailable");
        return;
    };

    // The send outlives the originating composer component. Navigation cannot
    // cancel durable receipt settlement after a delivery-capable request.
    let Some(owner) = upload_journal::Owner::capture(&state.peek(), Some(&prepared.team)) else {
        restore_failed_upload(
            &mut state,
            &visit,
            &prepared.client_msg_id,
            prepared.thread_ts.as_ref(),
        );
        return;
    };
    dioxus::core::spawn_forever(async move {
        let journal = match upload_journal::reserve(state, &owner, &prepared.client_msg_id).await {
            Ok(journal) => journal,
            Err(failure) => {
                if owner.current(&state.peek()) {
                    if failure.may_restore {
                        restore_failed_upload(
                            &mut state,
                            &visit,
                            &prepared.client_msg_id,
                            prepared.thread_ts.as_ref(),
                        );
                    } else {
                        outcomes::mark_upload_status(
                            &mut state.write(),
                            &visit,
                            &prepared.client_msg_id,
                            crate::state::PendingDeliveryStatus::Unconfirmed,
                        );
                    }
                    state.write().show_toast(failure.message);
                }
                return;
            }
        };
        let result = api::upload_files_with_receipt(
            &transport,
            &client,
            &workspace_session,
            api::UploadRequest {
                reply_broadcast: prepared.reply_broadcast,
                client_msg_id: prepared.client_msg_id.clone(),
                channel: prepared.channel.clone(),
                thread_ts: prepared.thread_ts.clone(),
                initial_comment: prepared.text.clone(),
                files: prepared.files,
                canceled: prepared.upload_cancel.clone(),
            },
            |receipt| journal.finalize(state, &owner, receipt.file_ids),
        )
        .await;
        // A definite pre-delivery failure retires its own exact ticket even if the
        // user has switched accounts; it never edits that replacement account UI.
        if result
            .as_ref()
            .is_err_and(|error| !error.delivery_uncertain())
        {
            journal.retire(state, &owner).await;
        }
        if !owner.current(&state.peek()) || !visit.current(&state.peek()) {
            return;
        }
        // A lost sharing receipt is not permission to resend. Private finalization
        // still identifies the exact files; only canonical history can retire the row.
        let readback_files = upload_readback_files(&result);
        if let Some(file_ids) = readback_files {
            let already_confirmed = upload_recovery::attach(
                &mut state.write(),
                &prepared.client_msg_id,
                &workspace_session.user_id,
                file_ids.clone(),
            );
            if already_confirmed {
                state.write().refresh_from_core();
                persist_workspace(&state, &prepared.team);
                return;
            }
            {
                let mut shell = state.write();
                if let Some(pending) = shell
                    .core
                    .pending_file_messages
                    .iter_mut()
                    .find(|pending| pending.client_msg_id == prepared.client_msg_id)
                {
                    for attachment in &mut pending.attachments {
                        attachment.uploading = false;
                        attachment.upload_started = None;
                        if let Some(progress) = &attachment.upload_progress {
                            progress.store(attachment.bytes, Ordering::Relaxed);
                        }
                    }
                }
            }
            let refreshed = ownership::refresh_upload_history(
                &mut state,
                &visit,
                &client,
                &workspace_session,
                &ownership::UploadReadback {
                    team: prepared.team.clone(),
                    channel: prepared.channel.clone(),
                    thread_ts: prepared.thread_ts.clone(),
                    reply_broadcast: prepared.reply_broadcast,
                    sender: workspace_session.user_id.clone(),
                    file_ids,
                },
            )
            .await;
            if !visit.current(&state.peek()) {
                return;
            }
            if !refreshed {
                outcomes::mark_upload_status(
                    &mut state.write(),
                    &visit,
                    &prepared.client_msg_id,
                    crate::state::PendingDeliveryStatus::Unconfirmed,
                );
                return;
            }
            {
                let mut shell = state.write();
                shell
                    .core
                    .pending_file_messages
                    .retain(|pending| pending.client_msg_id != prepared.client_msg_id);
                shell.pending_delivery.remove(&prepared.message_ts);
                // Readback found the exact files under the original sender and destination.
                upload_journal::confirmed(&mut shell, &prepared.client_msg_id);
                if let Some(root) = prepared.thread_ts.as_ref() {
                    if let Some(messages) = shell.core.threads.get_mut(&(
                        prepared.team.clone(),
                        prepared.channel.clone(),
                        root.clone(),
                    )) {
                        messages.remove(&prepared.message_ts);
                    }
                    shell.refresh_thread_from_core();
                } else if let Some(messages) = shell
                    .core
                    .workspaces
                    .get_mut(&prepared.team)
                    .and_then(|workspace| workspace.messages.get_mut(&prepared.channel))
                {
                    messages.remove(&prepared.message_ts);
                }
                shell.refresh_from_core();
            }
            persist_workspace(&state, &prepared.team);
        } else if let Err(error) = result {
            if error.delivery_uncertain() {
                outcomes::mark_upload_status(
                    &mut state.write(),
                    &visit,
                    &prepared.client_msg_id,
                    crate::state::PendingDeliveryStatus::Unconfirmed,
                );
                state.write().show_toast(
                    "Upload delivery is unconfirmed; check the conversation before uploading again",
                );
                return;
            }
            let canceled = matches!(
                error.error(),
                super_platinum_core::slack::Error::UploadCanceled
            );
            let retained = restore_failed_upload(
                &mut state,
                &visit,
                &prepared.client_msg_id,
                prepared.thread_ts.as_ref(),
            );
            if !canceled {
                state
                    .write()
                    .report_failure(error.error(), if retained { "Upload not sent; its caption and files remain in the conversation. Your newer draft is unchanged." } else { "Upload not sent; its caption and files were restored" }, || {
                        if retained { "Upload not sent; its caption and files remain in the conversation. Your newer draft is unchanged.".into() } else { format!("Upload failed: {error}") }
                    });
            }
        }
    });
}

fn upload_readback_files(
    result: &Result<api::CompletedUpload, api::UploadMessageError>,
) -> Option<Vec<String>> {
    match result {
        Ok(completed) => Some(completed.file_ids.clone()),
        Err(error) => error.finalized_files().map(<[String]>::to_vec),
    }
}

struct PreparedUpload {
    reply_broadcast: bool,
    team: String,
    channel: String,
    thread_ts: Option<String>,
    message_ts: String,
    client_msg_id: String,
    text: String,
    files: Vec<api::UploadFile>,
    upload_cancel: Arc<AtomicBool>,
}

fn restore_failed_upload(
    state: &mut Signal<ShellState>,
    visit: &SendVisit,
    client_msg_id: &str,
    thread_ts: Option<&String>,
) -> bool {
    restore_upload_to_shell(&mut state.write(), visit, client_msg_id, thread_ts)
}

fn restore_upload_to_shell(
    shell: &mut ShellState,
    visit: &SendVisit,
    client_msg_id: &str,
    thread_ts: Option<&String>,
) -> bool {
    if !visit.current(shell) {
        return false;
    }
    shell.checkpoint_composers();
    if let Some(pending) = shell
        .core
        .pending_file_messages
        .iter()
        .find(|pending| pending.client_msg_id == client_msg_id)
    {
        let key = super_platinum_core::drafts::DraftKey {
            team: pending.team.clone(),
            channel: pending.channel.clone(),
            thread: pending.thread_ts.clone(),
        };
        if shell
            .core
            .drafts
            .items
            .get(&key)
            .is_some_and(|draft| !draft.empty())
        {
            let pending = shell
                .core
                .pending_file_messages
                .iter_mut()
                .find(|pending| pending.client_msg_id == client_msg_id)
                .unwrap();
            for attachment in &mut pending.attachments {
                attachment.uploading = false;
                attachment.upload_started = None;
                attachment.upload_cancel = None;
                attachment.upload_progress = None;
            }
            shell.pending_delivery.insert(
                pending.message_ts.clone(),
                crate::state::PendingDeliveryStatus::Failed,
            );
            shell.show_toast("Upload not sent; its caption and files remain in the conversation. Your newer draft is unchanged.");
            shell.refresh_from_core();
            if thread_ts.is_some() {
                shell.refresh_thread_from_core();
            }
            return true;
        }
    }
    let pending = shell
        .core
        .pending_file_messages
        .iter()
        .position(|pending| pending.client_msg_id == client_msg_id)
        .map(|index| shell.core.pending_file_messages.remove(index));
    let Some(pending) = pending else {
        return false;
    };
    if let Some(root) = thread_ts {
        if let Some(messages) = shell.core.threads.get_mut(&(
            pending.team.clone(),
            pending.channel.clone(),
            root.clone(),
        )) {
            messages.remove(&pending.message_ts);
        }
        shell.refresh_thread_from_core();
    } else if let Some(messages) = shell
        .core
        .workspaces
        .get_mut(&pending.team)
        .and_then(|workspace| workspace.messages.get_mut(&pending.channel))
    {
        messages.remove(&pending.message_ts);
    }
    shell.pending_delivery.remove(&pending.message_ts);
    let mut attachments = pending.attachments;
    for attachment in &mut attachments {
        attachment.uploading = false;
        attachment.upload_started = None;
        attachment.upload_cancel = None;
        attachment.upload_progress = None;
    }
    shell.checkpoint_composers();
    let key = super_platinum_core::drafts::DraftKey {
        team: pending.team,
        channel: pending.channel,
        thread: pending.thread_ts,
    };
    let target = if key.thread.is_some() {
        crate::state::ComposerTarget::Thread
    } else {
        crate::state::ComposerTarget::Channel
    };
    if shell.composer_key(target).as_ref() == Some(&key) {
        if target == crate::state::ComposerTarget::Thread {
            if shell.core.thread_composer.text.is_empty()
                && !shell.core.thread_composer.reply_broadcast
            {
                shell.core.thread_composer =
                    super_platinum_core::ComposerState::with_text(&pending.text);
                shell.core.thread_composer.reply_broadcast = pending.reply_broadcast;
                crate::view::composer::set_field_text("thread-composer", &pending.text);
            }
            shell.core.thread_composer_attachments.extend(attachments);
        } else {
            if shell.core.composer.text.is_empty() {
                shell.core.composer = super_platinum_core::ComposerState::with_text(&pending.text);
                crate::view::composer::set_field_text("channel-composer", &pending.text);
            }
            shell.core.composer_attachments.extend(attachments);
        }
    } else {
        shell.core.drafts.return_upload_with_broadcast(
            key,
            pending.text,
            attachments,
            pending.reply_broadcast,
        );
    }
    shell.checkpoint_composers();
    shell.refresh_from_core();
    false
}

pub async fn toggle_reaction(
    mut state: Signal<ShellState>,
    channel: String,
    ts: String,
    name: String,
    removing: bool,
) {
    let Some((transport, client, workspaces)) = credentials(&state) else {
        return;
    };
    let Some(team) = state.read().core.active_team.clone() else {
        return;
    };
    let Some(workspace_session) = workspaces
        .into_iter()
        .find(|workspace| workspace.team_id == team)
    else {
        return;
    };
    let user = workspace_session.user_id.clone();
    {
        let mut shell = state.write();
        crate::realtime::apply_reaction(
            &mut shell.core,
            &team,
            &channel,
            &ts,
            &user,
            &name,
            !removing,
        );
        shell.refresh_from_core();
    }
    let result = if removing {
        api::remove_reaction(
            &transport,
            &client,
            &workspace_session,
            channel.clone(),
            ts.clone(),
            name.clone(),
        )
        .await
    } else {
        api::add_reaction(
            &transport,
            &client,
            &workspace_session,
            channel.clone(),
            ts.clone(),
            name.clone(),
        )
        .await
    };
    // Account reloads replace the transport. Never roll a late response into
    // another account's workspace, even when the team id is the same.
    if state
        .peek()
        .core
        .transport
        .as_ref()
        .is_none_or(|current| !std::sync::Arc::ptr_eq(current, &transport))
    {
        return;
    }
    if let Err(error) = result {
        let mut shell = state.write();
        crate::realtime::apply_reaction(
            &mut shell.core,
            &team,
            &channel,
            &ts,
            &user,
            &name,
            removing,
        );
        shell.refresh_from_core();
        shell.report_failure(&error, "Reaction not saved — you are offline", || {
            format!("Reaction failed: {error}")
        });
    } else {
        persist_workspace(&state, &team);
    }
}

type EditContent = (
    Option<String>,
    Vec<serde_json::Value>,
    Option<serde_json::Value>,
);
fn edit_content(message: &super_platinum_core::slack::models::Message) -> EditContent {
    (
        message.text.clone(),
        message.blocks.clone(),
        message.edited.clone(),
    )
}
fn edit_baseline(
    messages: &super_platinum_core::state::ChannelMessages,
    ts: &str,
) -> Option<EditContent> {
    messages
        .messages
        .iter()
        .find(|message| message.ts.as_deref() == Some(ts))
        .map(edit_content)
}
struct EditVisit {
    session: SendVisit,
    team: String,
    channel: String,
    ts: String,
    generation: u64,
    raw_text: String,
    channel_baseline: Option<EditContent>,
    thread_baselines: std::collections::HashMap<String, EditContent>,
}
impl EditVisit {
    fn capture(
        shell: &ShellState,
        transport: &Arc<super_platinum_core::slack::Transport>,
    ) -> Option<Self> {
        let (channel, ts) = shell.core.editing.clone()?;
        let team = shell.core.active_team.clone()?;
        let channel_baseline = shell
            .core
            .workspaces
            .get(&team)
            .and_then(|workspace| workspace.messages.get(&channel))
            .and_then(|messages| edit_baseline(messages, &ts));
        let thread_baselines = shell
            .core
            .threads
            .iter()
            .filter_map(|((thread_team, thread_channel, root), messages)| {
                (thread_team == &team && thread_channel == &channel)
                    .then(|| edit_baseline(messages, &ts).map(|baseline| (root.clone(), baseline)))
                    .flatten()
            })
            .collect();
        Some(Self {
            session: SendVisit::capture(shell, transport),
            team,
            channel_baseline,
            thread_baselines,
            channel,
            ts,
            generation: shell.edit_generation,
            raw_text: shell.core.edit_composer.text.clone(),
        })
    }
    fn editing(&self, shell: &ShellState) -> bool {
        self.session.current(shell)
            && shell.core.active_team.as_ref() == Some(&self.team)
            && shell.edit_generation == self.generation
            && shell.core.editing.as_ref() == Some(&(self.channel.clone(), self.ts.clone()))
            && shell.core.edit_composer.text == self.raw_text
    }
}

fn merge_edit_if_unchanged(
    messages: &mut super_platinum_core::state::ChannelMessages,
    ts: &str,
    baseline: Option<&EditContent>,
    updated: &super_platinum_core::slack::models::Message,
) {
    // A response cannot reverse a newer HTTP/realtime edit or introduce this
    // message into an unrelated thread. Reactions are outside this comparison
    // and outside the edit-only patch, so independent reaction updates survive.
    if !messages.was_deleted(ts)
        && baseline.is_some()
        && edit_baseline(messages, ts).as_ref() == baseline
    {
        messages.merge_update(updated.clone());
    }
}

fn apply_edit_result(
    shell: &mut ShellState,
    visit: &EditVisit,
    result: Result<
        super_platinum_core::slack::models::SentMessage,
        super_platinum_core::slack::Error,
    >,
) -> bool {
    if !visit.session.current(shell) {
        return false;
    }
    match result {
        Ok(sent) => {
            if sent.channel != visit.channel
                || sent.ts != visit.ts
                || sent.message.ts.as_ref().is_some_and(|ts| ts != &visit.ts)
                || sent
                    .message
                    .channel
                    .as_ref()
                    .is_some_and(|channel| channel != &visit.channel)
            {
                if visit.editing(shell) {
                    shell.show_toast("Slack did not confirm this edit. Your text is still here.");
                }
                return false;
            }
            let updated = super_platinum_core::slack::models::Message {
                ts: Some(visit.ts.clone()),
                channel: Some(visit.channel.clone()),
                text: sent.message.text,
                blocks: sent.message.blocks,
                edited: sent.message.edited,
                ..Default::default()
            };
            if let Some(messages) = shell
                .core
                .workspaces
                .get_mut(&visit.team)
                .and_then(|workspace| workspace.messages.get_mut(&visit.channel))
            {
                merge_edit_if_unchanged(
                    messages,
                    &visit.ts,
                    visit.channel_baseline.as_ref(),
                    &updated,
                );
            }
            for ((team, channel, root), messages) in &mut shell.core.threads {
                if team == &visit.team && channel == &visit.channel {
                    merge_edit_if_unchanged(
                        messages,
                        &visit.ts,
                        visit.thread_baselines.get(root),
                        &updated,
                    );
                }
            }
            if visit.editing(shell) {
                shell.cancel_edit();
            }
            shell.refresh_from_core();
            shell.refresh_thread_from_core();
            true
        }
        Err(error) => {
            if visit.editing(shell) {
                shell.report_failure(&error, "Edit not saved — you are offline", || {
                    "Edit could not be confirmed. Your text is still here.".into()
                });
            }
            false
        }
    }
}

pub async fn save_edit(mut state: Signal<ShellState>) {
    let Some((transport, client, workspaces)) = credentials(&state) else {
        return;
    };
    let Some(visit) = EditVisit::capture(&state.read(), &transport) else {
        return;
    };
    let text = visit.raw_text.trim().to_owned();
    if text.is_empty() || !visit.session.current(&state.peek()) {
        return;
    }
    let Some(workspace_session) = workspaces
        .into_iter()
        .find(|workspace| workspace.team_id == visit.team)
    else {
        return;
    };
    let result = api::edit_message(
        &transport,
        &client,
        &workspace_session,
        visit.channel.clone(),
        visit.ts.clone(),
        text,
    )
    .await;
    let changed = apply_edit_result(&mut state.write(), &visit, result);
    if changed {
        persist_workspace(&state, &visit.team);
    }
}

#[cfg(test)]
#[path = "messaging/edit_tests.rs"]
mod edit_tests;

pub async fn delete_message(mut state: Signal<ShellState>, channel: String, ts: String) {
    let Some((transport, client, workspaces)) = credentials(&state) else {
        return;
    };
    let Some(team) = state.read().core.active_team.clone() else {
        return;
    };
    let Some(workspace_session) = workspaces
        .into_iter()
        .find(|workspace| workspace.team_id == team)
    else {
        return;
    };
    match api::delete_message(
        &transport,
        &client,
        &workspace_session,
        channel.clone(),
        ts.clone(),
    )
    .await
    {
        Ok(()) => {
            let mut shell = state.write();
            if let Some(messages) = shell
                .core
                .workspaces
                .get_mut(&team)
                .and_then(|workspace| workspace.messages.get_mut(&channel))
            {
                messages.remove(&ts);
            }
            for ((thread_team, thread_channel, _), messages) in &mut shell.core.threads {
                if thread_team == &team && thread_channel == &channel {
                    messages.remove(&ts);
                }
            }
            shell.cancel_edit();
            shell.refresh_from_core();
            shell.refresh_thread_from_core();
            drop(shell);
            persist_workspace(&state, &team);
        }
        Err(error) => {
            state
                .write()
                .report_failure(&error, "Delete not saved — you are offline", || {
                    format!("Delete failed: {error}")
                })
        }
    }
}

/// Display helpers for attachment chips / pending strips.
pub fn format_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{} KB", bytes.div_ceil(1024))
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

pub fn truncate_filename(name: &str) -> String {
    const MAX: usize = 22;
    let count = name.chars().count();
    if count <= MAX {
        return name.to_owned();
    }
    let head: String = name.chars().take(MAX).collect();
    format!("{head}…")
}

pub fn attachment_kind_label(path: &std::path::Path) -> &'static str {
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase())
        .as_deref()
    {
        Some("png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp") => "IMG",
        Some("mp4" | "mov" | "m4v" | "webm") => "VIDEO",
        _ => "FILE",
    }
}

pub fn attachment_progress_ratio(attachment: &ComposerAttachment) -> f32 {
    if !attachment.uploading {
        return if attachment.bytes == 0 { 0.0 } else { 1.0 };
    }
    let uploaded = attachment
        .upload_progress
        .as_ref()
        .map(|progress| progress.load(Ordering::Relaxed))
        .unwrap_or(0);
    (uploaded as f32 / attachment.bytes.max(1) as f32).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_validated_finalization_enables_uncertain_upload_readback() {
        use super_platinum_core::slack::Error;
        for error in [
            api::UploadMessageError::BeforeDelivery(Error::Transport("local file".into())),
            api::UploadMessageError::Completion(Error::Transport("no known files".into())),
            api::UploadMessageError::CompletionWithFiles {
                error: Error::Api("not_in_channel".into()),
                finalized: api::CompletedUpload {
                    file_ids: vec!["F1".into()],
                },
            },
        ] {
            assert!(upload_readback_files(&Err(error)).is_none());
        }
        let lost = api::UploadMessageError::CompletionWithFiles {
            error: Error::Transport("lost response".into()),
            finalized: api::CompletedUpload {
                file_ids: vec!["F1".into()],
            },
        };
        assert_eq!(upload_readback_files(&Err(lost)), Some(vec!["F1".into()]));
    }

    #[test]
    fn format_bytes_uses_kb_and_mb() {
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(2048), "2 KB");
        assert_eq!(format_bytes(1_572_864), "1.5 MB");
    }

    #[test]
    fn truncate_filename_ellipsis() {
        assert_eq!(truncate_filename("short.png"), "short.png");
        let long = "very-long-attachment-name-here.png";
        let truncated = truncate_filename(long);
        assert!(truncated.ends_with('…'));
        assert!(truncated.chars().count() <= 23);
    }
}
