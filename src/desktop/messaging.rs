use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

use dioxus::prelude::{ReadableExt, Signal, WritableExt};
use super_platinum_core::domain::{ComposerAttachment, PendingFileMessage};
use super_platinum_core::slack::api;

use crate::bootstrap::{credentials, persist_workspace, refresh_history};
use crate::state::{PendingSend, ShellState};

pub async fn send_composer(mut state: Signal<ShellState>) {
    if !state.read().core.composer_attachments.is_empty() {
        send_attachments(state).await;
        return;
    }
    let Some(pending) = state.write().queue_composer() else {
        return;
    };
    send_pending(state, pending, "Message").await;
}

pub async fn send_thread_composer(mut state: Signal<ShellState>) {
    if !state.read().core.thread_composer_attachments.is_empty() {
        send_thread_attachments(state).await;
        return;
    }
    let Some(pending) = state.write().queue_thread_composer() else {
        return;
    };
    send_pending(state, pending, "Thread reply").await;
}

async fn send_pending(mut state: Signal<ShellState>, pending: PendingSend, label: &str) {
    let Some((transport, client, workspaces)) = credentials(&state) else {
        state.write().toast = Some("Message is queued until Slack reconnects".into());
        return;
    };
    let Some(workspace_session) = workspaces
        .into_iter()
        .find(|workspace| workspace.team_id == pending.team)
    else {
        state.write().toast = Some("Active workspace session is unavailable".into());
        return;
    };
    match api::send_message(
        &transport,
        &client,
        &workspace_session,
        pending.channel.clone(),
        pending.text,
        pending.thread_ts.clone(),
    )
    .await
    {
        Ok(sent) => {
            let mut shell = state.write();
            let message = super_platinum_core::slack::models::Message {
                ts: Some(sent.ts),
                channel: Some(pending.channel.clone()),
                thread_ts: pending.thread_ts.clone(),
                ..sent.message
            };
            if let Some(root) = pending.thread_ts.as_ref() {
                if let Some(messages) = shell.core.threads.get_mut(&(
                    pending.team.clone(),
                    pending.channel.clone(),
                    root.clone(),
                )) {
                    messages.confirm(&pending.client_msg_id, message);
                }
                shell.refresh_thread_from_core();
            } else if let Some(messages) = shell
                .core
                .workspaces
                .get_mut(&pending.team)
                .and_then(|workspace| workspace.messages.get_mut(&pending.channel))
            {
                messages.confirm(&pending.client_msg_id, message);
            }
            shell.refresh_from_core();
            drop(shell);
            persist_workspace(&state, &pending.team);
        }
        Err(error) => {
            let mut shell = state.write();
            if let Some(root) = pending.thread_ts.as_ref() {
                if let Some(messages) = shell.core.threads.get_mut(&(
                    pending.team.clone(),
                    pending.channel.clone(),
                    root.clone(),
                )) {
                    messages.remove(&pending.optimistic_ts);
                }
                shell.refresh_thread_from_core();
            } else if let Some(messages) = shell
                .core
                .workspaces
                .get_mut(&pending.team)
                .and_then(|workspace| workspace.messages.get_mut(&pending.channel))
            {
                messages.remove(&pending.optimistic_ts);
            }
            shell.refresh_from_core();
            shell.toast = Some(format!("{label} failed: {error}"));
        }
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
        state.write().toast = Some("File upload unavailable while offline".into());
        return;
    };
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
            shell.toast = Some("File upload unavailable for this workspace".into());
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
        let files: Vec<(std::path::PathBuf, Arc<AtomicU64>)> = taken
            .iter_mut()
            .map(|attachment| {
                attachment.uploading = true;
                attachment.upload_started = Some(Instant::now());
                attachment.upload_cancel = Some(upload_cancel.clone());
                let progress = Arc::new(AtomicU64::new(0));
                attachment.upload_progress = Some(progress.clone());
                (attachment.path.clone(), progress)
            })
            .collect();

        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let client_msg_id = format!("super-platinum-file-{stamp}");
        let message_ts = format!("{}.000000", stamp / 1_000_000_000);
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

        PreparedUpload {
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

    let Some(workspace_session) = workspaces
        .into_iter()
        .find(|workspace| workspace.team_id == prepared.team)
    else {
        restore_failed_upload(&mut state, &prepared.client_msg_id, None);
        state.write().toast = Some("Active workspace session is unavailable".into());
        return;
    };

    let result = api::upload_files(
        &transport,
        &client,
        &workspace_session,
        api::UploadRequest {
            channel: prepared.channel.clone(),
            thread_ts: prepared.thread_ts.clone(),
            initial_comment: prepared.text.clone(),
            files: prepared.files,
            canceled: prepared.upload_cancel.clone(),
        },
    )
    .await;

    match result {
        Ok(()) => {
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
            refresh_history(
                &mut state,
                &transport,
                &client,
                &workspace_session,
                &prepared.team,
                prepared.channel.clone(),
            )
            .await;
            {
                let mut shell = state.write();
                shell
                    .core
                    .pending_file_messages
                    .retain(|pending| pending.client_msg_id != prepared.client_msg_id);
                // Drop the optimistic pending message; history refresh should have the real one.
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
        }
        Err(error) => {
            let canceled = matches!(error, super_platinum_core::slack::Error::UploadCanceled);
            restore_failed_upload(
                &mut state,
                &prepared.client_msg_id,
                prepared.thread_ts.as_ref(),
            );
            if !canceled {
                state.write().toast = Some(format!("Upload failed: {error}"));
            }
        }
    }
}

struct PreparedUpload {
    team: String,
    channel: String,
    thread_ts: Option<String>,
    message_ts: String,
    client_msg_id: String,
    text: String,
    files: Vec<(std::path::PathBuf, Arc<AtomicU64>)>,
    upload_cancel: Arc<AtomicBool>,
}

fn restore_failed_upload(
    state: &mut Signal<ShellState>,
    client_msg_id: &str,
    thread_ts: Option<&String>,
) {
    let mut shell = state.write();
    let pending = shell
        .core
        .pending_file_messages
        .iter()
        .position(|pending| pending.client_msg_id == client_msg_id)
        .map(|index| shell.core.pending_file_messages.remove(index));
    let Some(pending) = pending else {
        return;
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
    let mut attachments = pending.attachments;
    for attachment in &mut attachments {
        attachment.uploading = false;
        attachment.upload_started = None;
        attachment.upload_cancel = None;
        attachment.upload_progress = None;
    }
    if thread_ts.is_some() {
        shell.core.thread_composer_attachments.extend(attachments);
    } else {
        shell.core.composer_attachments.extend(attachments);
    }
    shell.refresh_from_core();
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
        if let Some(messages) = shell
            .core
            .workspaces
            .get_mut(&team)
            .and_then(|workspace| workspace.messages.get_mut(&channel))
        {
            messages.apply_reaction(&ts, &user, &name, !removing);
        }
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
    if let Err(error) = result {
        let mut shell = state.write();
        if let Some(messages) = shell
            .core
            .workspaces
            .get_mut(&team)
            .and_then(|workspace| workspace.messages.get_mut(&channel))
        {
            messages.apply_reaction(&ts, &user, &name, removing);
        }
        shell.refresh_from_core();
        shell.toast = Some(format!("Reaction failed: {error}"));
    } else {
        persist_workspace(&state, &team);
    }
}

pub async fn save_edit(mut state: Signal<ShellState>) {
    let (channel, ts, text) = {
        let shell = state.read();
        let Some((channel, ts)) = shell.core.editing.clone() else {
            return;
        };
        (channel, ts, shell.core.edit_composer.text.trim().to_owned())
    };
    if text.is_empty() {
        return;
    }
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
    match api::edit_message(
        &transport,
        &client,
        &workspace_session,
        channel.clone(),
        ts.clone(),
        text,
    )
    .await
    {
        Ok(sent) => {
            let mut shell = state.write();
            let updated = super_platinum_core::slack::models::Message {
                ts: Some(sent.ts),
                channel: Some(channel.clone()),
                ..sent.message
            };
            if let Some(messages) = shell
                .core
                .workspaces
                .get_mut(&team)
                .and_then(|workspace| workspace.messages.get_mut(&channel))
            {
                messages.merge_update(updated.clone());
            }
            for ((thread_team, thread_channel, _), messages) in &mut shell.core.threads {
                if thread_team == &team && thread_channel == &channel {
                    messages.merge_update(updated.clone());
                }
            }
            shell.cancel_edit();
            shell.refresh_from_core();
            shell.refresh_thread_from_core();
            drop(shell);
            persist_workspace(&state, &team);
        }
        Err(error) => state.write().toast = Some(format!("Edit failed: {error}")),
    }
}

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
        Err(error) => state.write().toast = Some(format!("Delete failed: {error}")),
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
