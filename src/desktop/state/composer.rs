use super_platinum_core::FormatMark;

use super::{ShellState, message_vm};
use crate::model::*;

impl ShellState {
    pub fn queue_composer(&mut self) -> Option<PendingSend> {
        let text = self.core.composer.text.trim().to_owned();
        if text.is_empty() {
            return None;
        }
        let domain_target = self
            .core
            .active_team
            .clone()
            .zip(self.core.active_channel.clone());
        if let Some((team, channel)) = domain_target {
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or(0);
            let client_msg_id = format!("super-platinum-{stamp}");
            let optimistic_ts = format!("{}.000000", stamp / 1_000_000_000);
            if let Some(workspace) = self.core.workspaces.get_mut(&team) {
                let message = super_platinum_core::slack::models::Message {
                    user: Some(workspace.self_user_id.clone()),
                    kind: Some("message".into()),
                    ts: Some(optimistic_ts.clone()),
                    client_msg_id: Some(client_msg_id.clone()),
                    text: Some(text.clone()),
                    channel: Some(channel.clone()),
                    ..Default::default()
                };
                let messages = workspace.messages.entry(channel.clone()).or_default();
                messages.upsert(message);
                messages.pending.push(optimistic_ts.clone());
                self.message_arrivals
                    .insert(optimistic_ts.clone(), std::time::Instant::now());
                self.core.composer = super_platinum_core::ComposerState::default();
                self.refresh_from_core();
                return Some(PendingSend {
                    team,
                    channel,
                    thread_ts: None,
                    text,
                    client_msg_id,
                    optimistic_ts,
                });
            }
        }

        // Offline visual fixtures have no authenticated workspace domain.
        let id = format!("local-{}", self.messages.len());
        let message = MessageVm {
            id: id.clone(),
            ts: id,
            author: "You".into(),
            timestamp: "now".into(),
            avatar_initials: "YO".into(),
            body: vec![RichNode::Text(text)],
            is_own: true,
            pending: true,
            ..Default::default()
        };
        self.messages.push(message.clone());
        if let Some(channel) = self.channels.get(self.active_channel) {
            self.messages_by_channel
                .entry(channel.id.clone())
                .or_default()
                .push(message);
        }
        self.core.composer = super_platinum_core::ComposerState::default();
        self.show_toast("Message queued by the fixture dispatcher");
        None
    }

    pub fn queue_thread_composer(&mut self) -> Option<PendingSend> {
        let text = self.core.thread_composer.text.trim().to_owned();
        if text.is_empty() {
            return None;
        }
        let (team, channel, root) = (
            self.core.active_team.clone()?,
            self.core.active_channel.clone()?,
            self.thread_root.clone()?,
        );
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let client_msg_id = format!("super-platinum-thread-{stamp}");
        let optimistic_ts = format!("{}.000000", stamp / 1_000_000_000);
        let user = self.core.workspaces.get(&team)?.self_user_id.clone();
        let message = super_platinum_core::slack::models::Message {
            user: Some(user),
            kind: Some("message".into()),
            ts: Some(optimistic_ts.clone()),
            client_msg_id: Some(client_msg_id.clone()),
            text: Some(text.clone()),
            channel: Some(channel.clone()),
            thread_ts: Some(root.clone()),
            ..Default::default()
        };
        let messages = self
            .core
            .threads
            .entry((team.clone(), channel.clone(), root.clone()))
            .or_default();
        messages.upsert(message);
        messages.pending.push(optimistic_ts.clone());
        self.core.thread_composer = super_platinum_core::ComposerState::default();
        self.refresh_thread_from_core();
        Some(PendingSend {
            team,
            channel,
            thread_ts: Some(root),
            text,
            client_msg_id,
            optimistic_ts,
        })
    }

    pub fn refresh_thread_from_core(&mut self) {
        let Some((team, channel, root)) = self
            .core
            .active_team
            .clone()
            .zip(self.core.active_channel.clone())
            .zip(self.thread_root.clone())
            .map(|((team, channel), root)| (team, channel, root))
        else {
            self.thread_messages.clear();
            return;
        };
        let messages = self
            .core
            .threads
            .get(&(team.clone(), channel, root))
            .map(|messages| messages.messages.clone())
            .unwrap_or_default();
        if let Some(workspace) = self.core.workspaces.get(&team) {
            self.thread_messages = messages
                .iter()
                .map(|message| message_vm(workspace, message, &self.media))
                .collect();
        }
    }

    #[allow(dead_code)]
    pub fn format_composer(&mut self, mark: FormatMark) {
        self.core.composer.apply_format(mark);
    }

    pub fn start_edit(&mut self, channel: String, ts: String, text: String) {
        self.core.editing = Some((channel, ts));
        self.core.edit_composer = super_platinum_core::ComposerState::with_text(text);
    }

    pub fn cancel_edit(&mut self) {
        self.core.editing = None;
        self.core.edit_composer = super_platinum_core::ComposerState::default();
    }

    pub fn add_attachments(&mut self, paths: impl IntoIterator<Item = std::path::PathBuf>) {
        for path in paths {
            if !path.is_file()
                || self
                    .core
                    .composer_attachments
                    .iter()
                    .any(|attachment| attachment.path == path)
            {
                continue;
            }
            let Ok(metadata) = std::fs::metadata(&path) else {
                continue;
            };
            self.core.attachment_seq = self.core.attachment_seq.wrapping_add(1);
            self.core
                .composer_attachments
                .push(super_platinum_core::domain::ComposerAttachment {
                    id: self.core.attachment_seq,
                    name: path
                        .file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or("attachment")
                        .to_owned(),
                    path,
                    bytes: metadata.len(),
                    uploading: false,
                    upload_started: None,
                    upload_cancel: None,
                    upload_progress: None,
                    preview_path: None,
                });
        }
    }

    pub fn remove_attachment(&mut self, id: u64) {
        for attachment in self
            .core
            .composer_attachments
            .iter()
            .chain(self.core.thread_composer_attachments.iter())
            .filter(|attachment| attachment.id == id)
        {
            if let Some(cancel) = &attachment.upload_cancel {
                cancel.store(true, std::sync::atomic::Ordering::Relaxed);
            }
        }
        self.core
            .composer_attachments
            .retain(|attachment| attachment.id != id);
        self.core
            .thread_composer_attachments
            .retain(|attachment| attachment.id != id);
    }

    /// Freeze virtualization while the user is drag-selecting message text so
    /// multi-row native selection cannot unmount mid-gesture. Expand the window
    /// so the drag can span many adjacent rows.
    pub fn pin_selection(&mut self) {
        if self.selection_pinned {
            return;
        }
        self.selection_pinned = true;
        self.timeline_start = self.timeline_start.saturating_sub(80);
        self.timeline_end = (self.timeline_end + 80).min(self.messages.len());
        if self.timeline_end <= self.timeline_start {
            self.reset_timeline_window();
        }
    }

    pub fn unpin_selection(&mut self) {
        self.selection_pinned = false;
    }

    pub fn pending_attachments_for_message(
        &self,
        message_ts: &str,
    ) -> Option<&[super_platinum_core::domain::ComposerAttachment]> {
        self.core
            .pending_file_messages
            .iter()
            .find(|pending| pending.message_ts == message_ts)
            .map(|pending| pending.attachments.as_slice())
    }

    pub fn load_managed_background(&mut self) {
        let Some(background) = self.core.settings.background.as_ref() else {
            self.background_uri = None;
            return;
        };
        let Some(path) = super_platinum_core::config::background_path(background) else {
            return;
        };
        let Ok(bytes) = std::fs::read(&path) else {
            return;
        };
        let mime = match path.extension().and_then(|extension| extension.to_str()) {
            Some("png") => "image/png",
            Some("webp") => "image/webp",
            _ => "image/jpeg",
        };
        let id =
            super_platinum_core::MediaAssetId::new(super_platinum_core::MediaAssetKind::Background);
        self.media.insert(id.clone(), mime, bytes);
        self.background_uri = Some(id.uri());
    }
}
