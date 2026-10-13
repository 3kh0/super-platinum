use super::ShellState;
use crate::model::*;

/// Which composer a file is being attached to.
///
/// The channel and the thread pane each send their own attachments, so a file
/// picked, dropped, or pasted into the thread has to land in the thread's list
/// — filed into the channel's, it would post as a top-level message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComposerTarget {
    Channel,
    Thread,
}

impl ShellState {
    /// All optimistic message surfaces share one monotonically increasing local
    /// microsecond clock. Two sends in the same clock tick must not replace one
    /// another in ChannelMessages, which indexes rows by timestamp.
    pub(crate) fn optimistic_identity(prefix: &str) -> (String, String) {
        static LAST: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let micros = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| u64::try_from(duration.as_micros()).unwrap_or(u64::MAX - 1))
            .unwrap_or(0);
        allocate_optimistic_identity(&LAST, micros, prefix)
    }

    pub fn composer_unavailable(&self) -> Option<&'static str> {
        let team = self.core.active_team.as_ref()?;
        let channel = self.core.active_channel.as_ref()?;
        let info = self.core.workspaces.get(team)?.channels.get(channel)?;
        if info.is_archived {
            Some("This channel is archived. Unarchive it in channel details to send messages.")
        } else if !info.is_im
            && !info.is_mpim
            && info.extra.get("is_member").and_then(|v| v.as_bool()) == Some(false)
        {
            Some(if info.is_private || info.is_group {
                "You are not a member of this private channel. An invitation is required to rejoin."
            } else {
                "Join this channel to send messages."
            })
        } else {
            None
        }
    }

    pub fn queue_composer(&mut self) -> Option<PendingSend> {
        if self.composer_unavailable().is_some() {
            return None;
        }
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
            let (client_msg_id, optimistic_ts) = Self::optimistic_identity("super-platinum");
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
                self.checkpoint_composers();
                self.refresh_from_core();
                return Some(PendingSend {
                    account: self.core.active_account.clone(),
                    account_epoch: self.core.account_epoch,
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
        let id = format!("local-{}", self.channel_rows.len());
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
        let mut messages = self
            .fixture_channel_messages
            .take()
            .unwrap_or_else(|| self.messages.clone());
        messages.push(message);
        self.install_fixture_messages(messages);
        self.core.composer = super_platinum_core::ComposerState::default();
        self.checkpoint_composers();
        self.show_toast("Message queued by the fixture dispatcher");
        None
    }

    pub fn queue_thread_composer(&mut self) -> Option<PendingSend> {
        if self.composer_unavailable().is_some() {
            return None;
        }
        let text = self.core.thread_composer.text.trim().to_owned();
        if text.is_empty() {
            return None;
        }
        let (team, channel, root) = (
            self.core.active_team.clone()?,
            self.core.active_channel.clone()?,
            self.thread_root.clone()?,
        );
        let (client_msg_id, optimistic_ts) = Self::optimistic_identity("super-platinum-thread");
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
        self.checkpoint_composers();
        self.refresh_thread_from_core();
        Some(PendingSend {
            account: self.core.active_account.clone(),
            account_epoch: self.core.account_epoch,
            team,
            channel,
            thread_ts: Some(root),
            text,
            client_msg_id,
            optimistic_ts,
        })
    }

    pub fn refresh_thread_from_core(&mut self) {
        self.prune_pending_delivery();
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
        let scope = self.thread_scope();
        let bag = self
            .core
            .threads
            .get(&(team.clone(), channel, root.clone()));
        self.thread_messages = self
            .core
            .workspaces
            .get(&team)
            .zip(bag)
            .map(|(workspace, bag)| {
                self.thread_window.sync(
                    &scope,
                    bag,
                    &self.thread_history,
                    &root,
                    self.thread_target.as_deref(),
                    self.thread_at_bottom,
                );
                crate::thread_vm::project_window(
                    workspace,
                    bag,
                    &self.thread_history,
                    &root,
                    &self.media,
                    &self.thread_window,
                )
            })
            .unwrap_or_default();
    }

    pub fn start_edit(&mut self, channel: String, ts: String, text: String) {
        self.edit_generation = self.edit_generation.wrapping_add(1);
        self.core.editing = Some((channel, ts));
        self.core.edit_composer = super_platinum_core::ComposerState::with_text(text);
    }

    pub fn cancel_edit(&mut self) {
        self.edit_generation = self.edit_generation.wrapping_add(1);
        self.core.editing = None;
        self.core.edit_composer = super_platinum_core::ComposerState::default();
    }

    pub fn add_attachments(
        &mut self,
        paths: impl IntoIterator<Item = std::path::PathBuf>,
        target: ComposerTarget,
    ) {
        // With no thread open there is no thread composer to hold the files.
        let target = if self.thread_root.is_none() {
            ComposerTarget::Channel
        } else {
            target
        };
        self.sync_composer_drafts();
        for path in paths {
            let attachments = match target {
                ComposerTarget::Channel => &self.core.composer_attachments,
                ComposerTarget::Thread => &self.core.thread_composer_attachments,
            };
            if !path.is_file() || attachments.iter().any(|attachment| attachment.path == path) {
                continue;
            }
            let Ok(metadata) = std::fs::metadata(&path) else {
                continue;
            };
            self.core.attachment_seq = self.core.attachment_seq.wrapping_add(1);
            let attachments = match target {
                ComposerTarget::Channel => &mut self.core.composer_attachments,
                ComposerTarget::Thread => &mut self.core.thread_composer_attachments,
            };
            attachments.push(super_platinum_core::domain::ComposerAttachment {
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
        self.checkpoint_composers();
    }

    pub fn remove_attachment(&mut self, id: u64) {
        self.sync_composer_drafts();
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
        self.checkpoint_composers();
    }

    /// Freeze virtualization while the user is drag-selecting message text so
    /// multi-row native selection cannot unmount mid-gesture. Keep the existing
    /// overscan: inserting estimated-height rows above the pointer during
    /// selectstart moves the clicked text before the native selection begins.
    pub fn pin_selection(&mut self) {
        if self.selection_pinned {
            return;
        }
        self.selection_pinned = true;
    }

    pub fn unpin_selection(&mut self) {
        self.selection_pinned = false;
        self.reproject_channel_window();
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

fn allocate_optimistic_identity(
    last: &std::sync::atomic::AtomicU64,
    now_micros: u64,
    prefix: &str,
) -> (String, String) {
    use std::sync::atomic::Ordering;
    let previous = last
        .try_update(Ordering::Relaxed, Ordering::Relaxed, |previous| {
            Some(
                now_micros.max(
                    previous
                        .checked_add(1)
                        .expect("optimistic timestamp exhausted"),
                ),
            )
        })
        .expect("optimistic timestamp update always succeeds");
    let stamp = now_micros.max(previous + 1);
    (
        format!("{prefix}-{stamp}"),
        format!("{}.{:06}", stamp / 1_000_000, stamp % 1_000_000),
    )
}

#[cfg(test)]
mod optimistic_identity_tests {
    use super::*;
    use std::sync::atomic::AtomicU64;
    use super_platinum_core::{slack::models::Message, state::ChannelMessages};

    #[test]
    fn same_microsecond_channel_thread_and_upload_rows_keep_separate_identities() {
        for prefix in [
            "super-platinum",
            "super-platinum-thread",
            "super-platinum-file",
        ] {
            for confirm_first in [false, true] {
                let clock = AtomicU64::new(0);
                let now = 1_800_000_000_123_456;
                let (first_id, first_ts) = allocate_optimistic_identity(&clock, now, prefix);
                let (second_id, second_ts) = allocate_optimistic_identity(&clock, now, prefix);
                assert_ne!(first_id, second_id);
                assert_ne!(first_ts, second_ts);
                assert_eq!(first_ts, "1800000000.123456");
                assert_eq!(second_ts, "1800000000.123457");
                let mut messages = ChannelMessages::default();
                for (id, ts, text) in [
                    (&first_id, &first_ts, "First"),
                    (&second_id, &second_ts, "Second"),
                ] {
                    messages.upsert(Message {
                        ts: Some(ts.clone()),
                        client_msg_id: Some(id.clone()),
                        text: Some(text.into()),
                        ..Default::default()
                    });
                    messages.pending.push(ts.clone());
                }
                assert_eq!(messages.messages.len(), 2);
                if confirm_first {
                    assert!(messages.confirm(
                        &first_id,
                        Message {
                            ts: Some("1800000001.000000".into()),
                            text: Some("First confirmed".into()),
                            ..Default::default()
                        }
                    ));
                } else {
                    assert!(messages.remove(&first_ts));
                }
                assert_eq!(messages.pending, vec![second_ts.clone()]);
                assert!(
                    messages
                        .messages
                        .iter()
                        .any(|message| message.ts.as_ref() == Some(&second_ts)
                            && message.client_msg_id.as_ref() == Some(&second_id)
                            && message.text.as_deref() == Some("Second"))
                );
            }
        }
    }

    #[test]
    fn optimistic_clock_is_shared_across_surfaces_and_survives_clock_rollback() {
        let clock = AtomicU64::new(0);
        let first = allocate_optimistic_identity(&clock, 1_800_000_000_999_999, "super-platinum");
        let second =
            allocate_optimistic_identity(&clock, 1_800_000_000_999_999, "super-platinum-file");
        let third =
            allocate_optimistic_identity(&clock, 1_700_000_000_000_000, "super-platinum-thread");
        assert_eq!(first.1, "1800000000.999999");
        assert_eq!(second.1, "1800000001.000000");
        assert_eq!(third.1, "1800000001.000001");
    }
}
