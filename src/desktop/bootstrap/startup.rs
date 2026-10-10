//! Disk and Keychain work stays off the serial UI dispatcher.

use dioxus::prelude::{ReadableExt, Signal, WritableExt};
use super_platinum_core::state::ChannelMessages;

use crate::{media::MediaRegistry, state::ShellState};

pub(crate) async fn load_environment(media: MediaRegistry) -> ShellState {
    let fallback = media.clone();
    match tokio::task::spawn_blocking(move || ShellState::from_environment(media)).await {
        Ok(shell) => shell,
        Err(error) => {
            eprintln!("super-platinum: startup worker failed: {error}");
            let mut shell = ShellState::opening(fallback);
            shell.loading = false;
            shell.signed_in = false;
            shell
        }
    }
}

pub(crate) async fn initialize(mut state: Signal<ShellState>) {
    if std::env::var_os("SUPER_PLATINUM_FIXTURE").is_none() {
        let media = state.read().media.clone();
        let loaded = load_environment(media).await;
        *state.write() = loaded;
        crate::performance::record(crate::performance::StartupStage::CacheLoaded);
    }
    // Restore hidden transcripts before starting realtime. This prevents a
    // cached message from resurrecting a deletion delivered by the socket.
    // The selected conversation is already usable while this worker runs.
    restore_transcripts(state).await;
    super::refresh(state).await;
}

pub(super) async fn restore_transcripts(mut state: Signal<ShellState>) {
    let Some((account, sessions)) = ({
        let shell = state.read();
        shell.core.active_account.clone().zip(
            shell
                .core
                .session
                .as_ref()
                .map(|session| session.workspaces.values().cloned().collect::<Vec<_>>()),
        )
    }) else {
        return;
    };
    let expected_account = account.clone();
    let expected_transport = state.read().core.transport.clone();
    let restored = tokio::task::spawn_blocking(move || {
        let cache = super_platinum_core::cache::Cache::open_default(&account, false)?;
        sessions
            .into_iter()
            .map(|session| {
                let messages = cache
                    .load_workspace(&session)?
                    .map(|ws| ws.messages)
                    .unwrap_or_default();
                Ok((session.team_id, messages))
            })
            .collect::<Result<Vec<_>, super_platinum_core::error::AppError>>()
    })
    .await;
    let Ok(Ok(restored)) = restored else {
        eprintln!("super-platinum: deferred transcript restore failed");
        return;
    };
    let mut shell = state.write();
    if shell.core.active_account.as_ref() != Some(&expected_account)
        || !match (&shell.core.transport, &expected_transport) {
            (Some(current), Some(expected)) => std::sync::Arc::ptr_eq(current, expected),
            (None, None) => true,
            _ => false,
        }
    {
        return;
    }
    for (team, messages) in restored {
        if let Some(workspace) = shell.core.workspaces.get_mut(&team) {
            for (channel, cached) in messages {
                restore_transcript(workspace.messages.entry(channel).or_default(), cached);
            }
        }
    }
    shell.refresh_from_core();
}

fn restore_transcript(current: &mut ChannelMessages, cached: ChannelMessages) {
    // A navigation fetch or send may have changed this bag while disk was read.
    // Read state belongs to the live bag even when its transcript is untouched.
    if current.loaded || !current.messages.is_empty() || !current.pending.is_empty() {
        return;
    }
    current.messages = cached.messages;
    current.pending = cached.pending;
    current.loaded = cached.loaded;
    current.has_more_older = cached.has_more_older;
}

#[cfg(test)]
mod tests {
    use super::*;
    use super_platinum_core::slack::models::Message;

    fn cached() -> ChannelMessages {
        ChannelMessages {
            loaded: true,
            messages: vec![Message {
                ts: Some("1.0".into()),
                text: Some("cached".into()),
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    #[test]
    fn restore_preserves_current_read_state() {
        let mut current = ChannelMessages {
            unread_count: 7,
            last_read: Some("0.5".into()),
            ..Default::default()
        };
        restore_transcript(&mut current, cached());
        assert!(current.loaded);
        assert_eq!(current.messages.len(), 1);
        assert_eq!(current.unread_count, 7);
        assert_eq!(current.last_read.as_deref(), Some("0.5"));
    }

    #[test]
    fn restore_does_not_replace_fetched_empty_or_changed_transcripts() {
        for mut current in [
            ChannelMessages {
                loaded: true,
                ..Default::default()
            },
            ChannelMessages {
                pending: vec!["pending".into()],
                ..Default::default()
            },
            ChannelMessages {
                messages: vec![Message {
                    text: Some("new".into()),
                    ..Default::default()
                }],
                ..Default::default()
            },
        ] {
            let before = serde_json::to_value(&current.messages).unwrap();
            restore_transcript(&mut current, cached());
            assert_eq!(serde_json::to_value(&current.messages).unwrap(), before);
        }
    }
}
