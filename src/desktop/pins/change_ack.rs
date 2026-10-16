//! A successful pin acknowledgment owns membership, not an old message snapshot.
use super::{Context, Message, PinnedItem, ShellState};

pub(super) fn apply(
    shell: &mut ShellState,
    ctx: &Context,
    channel: &str,
    ts: &str,
    mut item: PinnedItem,
    pinned: bool,
    version: Option<&str>,
) -> bool {
    // A replaced owner or newer websocket membership change wins as before.
    if !ctx.current(shell) || shell.core.pins.version(channel, &format!("message:{ts}")) != version
    {
        return false;
    }
    // Resolve content only at the admitted acknowledgment boundary. Accepted
    // file edits and later file-bearing messages already reached retained copies.
    // When those copies vanished, the existing ts-only shape still records the
    // successful membership; the adapter's ordinary post-success load hydrates it.
    item.message = Some(
        pinned
            .then(|| super::message(shell, channel, ts))
            .flatten()
            .unwrap_or_else(|| Message {
                ts: Some(ts.into()),
                ..Default::default()
            }),
    );
    shell
        .core
        .set_pin_item(&ctx.session.team_id, channel, item, pinned);
    shell
        .core
        .update_pin_copies(&ctx.session.team_id, channel, ts, pinned);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use super_platinum_core::{
        pins::{PinUpdate, PinsPage},
        slack::{
            Transport,
            events::RtEvent,
            models::{Attachment, File},
        },
    };

    const CHANNEL: &str = "C2";
    const TS: &str = "989.0";

    fn file() -> File {
        serde_json::from_value(serde_json::json!({"id":"FACK","title":"Before","name":"before.txt","size":12,"pretty_type":"Text","filetype":"text","mimetype":"text/plain","url_private":"https://files.slack.com/before","opaque":{"keep":true}})).unwrap()
    }
    fn original() -> Message {
        Message {
            ts: Some(TS.into()),
            text: Some("Before acknowledgment".into()),
            files: vec![file()],
            attachments: vec![Attachment {
                files: vec![file()],
                ..Default::default()
            }],
            ..Default::default()
        }
    }
    fn setup() -> (ShellState, Context, PinnedItem) {
        let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
        let transport = Arc::new(Transport::new("synthetic-pin-ack-cookie").unwrap());
        shell.core.transport = Some(transport.clone());
        shell.core.pins.ensure_scope(shell.core.account_epoch, "T1");
        let generation = shell.core.pins.begin(CHANNEL);
        shell.core.pins.apply(generation, PinsPage::default());
        shell
            .core
            .workspaces
            .get_mut("T1")
            .unwrap()
            .messages
            .entry(CHANNEL.into())
            .or_default()
            .upsert(original());
        let ctx = Context {
            transport,
            client: shell.core.client.clone(),
            epoch: shell.core.account_epoch,
            session: super::super::WorkspaceSession {
                team_id: "T1".into(),
                enterprise_id: None,
                user_id: "U0".into(),
                name: "Fixture".into(),
                url: "https://example.invalid".into(),
                token: String::new(),
            },
        };
        // Only membership metadata crosses the network await in production.
        let item = PinnedItem {
            kind: "message".into(),
            channel: Some(CHANNEL.into()),
            created: Some(123),
            created_by: Some("U0".into()),
            ..Default::default()
        };
        (shell, ctx, item)
    }
    fn actual(shell: &ShellState) -> &Message {
        shell.core.pins.items[&format!("message:{TS}")]
            .message
            .as_ref()
            .unwrap()
    }
    fn assert_membership(shell: &ShellState) {
        assert!(shell.core.pins.is_pinned(CHANNEL, TS, false));
        let item = &shell.core.pins.items[&format!("message:{TS}")];
        assert_eq!(item.created, Some(123));
        assert_eq!(item.created_by.as_deref(), Some("U0"));
    }

    #[test]
    fn successful_ack_uses_later_rename_and_display_fields_from_retained_message() {
        let (mut shell, ctx, item) = setup();
        shell
            .core
            .rename_file("T1", "FACK", "After rename", Some("after.txt"));
        shell.core.merge_file_display_metadata(
            "T1",
            &File {
                id: Some("FACK".into()),
                size: Some(0),
                pretty_type: Some("Plain text".into()),
                ..Default::default()
            },
        );
        let expected = super::super::message(&shell, CHANNEL, TS).unwrap();
        assert!(apply(&mut shell, &ctx, CHANNEL, TS, item, true, None));
        assert_membership(&shell);
        assert_eq!(
            actual(&shell).files[0].title.as_deref(),
            Some("After rename")
        );
        assert_eq!(actual(&shell).files[0].size, Some(0));
        assert_eq!(
            serde_json::to_value(actual(&shell)).unwrap(),
            serde_json::to_value(expected).unwrap()
        );
    }

    #[test]
    fn successful_ack_keeps_later_file_bearing_message_instead_of_replaying_capture() {
        let (mut shell, ctx, item) = setup();
        shell
            .core
            .rename_file("T1", "FACK", "Intermediate rename", None);
        let later = File {
            title: Some("Later message file".into()),
            url_private: Some("https://files.slack.com/later".into()),
            ..file()
        };
        let changed = Message {
            text: Some("Later message body".into()),
            files: vec![later.clone()],
            attachments: vec![Attachment {
                files: vec![later],
                ..Default::default()
            }],
            ..original()
        };
        let socket = shell.core.workspaces["T1"].rt_generation;
        crate::realtime::apply_in_view(
            &mut shell.core,
            "T1",
            socket,
            RtEvent::MessageChanged {
                channel: CHANNEL.into(),
                message: changed.clone(),
            },
            true,
        );
        assert!(apply(&mut shell, &ctx, CHANNEL, TS, item, true, None));
        assert_membership(&shell);
        assert_eq!(
            serde_json::to_value(actual(&shell)).unwrap(),
            serde_json::to_value(changed).unwrap()
        );
    }

    #[test]
    fn vanished_sources_still_record_membership_with_existing_unhydrated_shape() {
        let (mut shell, ctx, item) = setup();
        shell
            .core
            .workspaces
            .get_mut("T1")
            .unwrap()
            .messages
            .remove(CHANNEL);
        shell.core.threads.clear();
        shell.core.pins.items.clear();
        assert!(super::super::message(&shell, CHANNEL, TS).is_none());
        assert!(apply(&mut shell, &ctx, CHANNEL, TS, item, true, None));
        assert_membership(&shell);
        assert_eq!(
            serde_json::to_value(actual(&shell)).unwrap(),
            serde_json::to_value(Message {
                ts: Some(TS.into()),
                ..Default::default()
            })
            .unwrap()
        );
    }

    #[test]
    fn display_fence_pressure_cannot_suppress_successful_membership() {
        let (mut shell, ctx, item) = setup();
        let held = (0..32)
            .map(|_| shell.core.begin_file_display_read("T1"))
            .collect::<Vec<_>>();
        let refused = shell.core.begin_file_display_read("T1");
        assert!(shell.core.finish_file_display_read("T1", refused).is_err());
        shell
            .core
            .rename_file("T1", "FACK", "Renamed under pressure", None);
        assert!(apply(&mut shell, &ctx, CHANNEL, TS, item, true, None));
        assert_membership(&shell);
        assert_eq!(
            actual(&shell).files[0].title.as_deref(),
            Some("Renamed under pressure")
        );
        drop(held);
    }

    #[test]
    fn current_thread_fallback_and_known_deletion_mask_remain_available() {
        let (mut shell, ctx, item) = setup();
        shell.core.threads.insert(
            ("T1".into(), CHANNEL.into(), TS.into()),
            super_platinum_core::state::ChannelMessages {
                messages: vec![original()],
                ..Default::default()
            },
        );
        shell
            .core
            .workspaces
            .get_mut("T1")
            .unwrap()
            .messages
            .remove(CHANNEL);
        shell.core.rename_file("T1", "FACK", "Thread rename", None);
        let expected = super::super::message(&shell, CHANNEL, TS).unwrap();
        assert!(apply(
            &mut shell,
            &ctx,
            CHANNEL,
            TS,
            item.clone(),
            true,
            None
        ));
        assert_eq!(
            serde_json::to_value(actual(&shell)).unwrap(),
            serde_json::to_value(expected).unwrap()
        );
        shell.core.remove_file("T1", "FACK");
        assert!(apply(&mut shell, &ctx, CHANNEL, TS, item, true, None));
        assert!(super_platinum_core::file_actions::deleted(
            &actual(&shell).files[0]
        ));
        assert!(actual(&shell).files[0].full_url().is_none());
    }

    #[test]
    fn stale_owner_or_newer_membership_version_refuses_ack_without_mutation() {
        for change in 0..4 {
            let (mut shell, ctx, item) = setup();
            match change {
                0 => shell.core.account_epoch += 1,
                1 => shell.core.active_team = Some("T2".into()),
                2 => shell.core.pins.ensure_scope(ctx.epoch, "T2"),
                _ => {
                    shell.core.transport = Some(Arc::new(
                        Transport::new("synthetic-pin-ack-cookie").unwrap(),
                    ))
                }
            }
            let generation = shell.core.pins.generation;
            assert!(!apply(&mut shell, &ctx, CHANNEL, TS, item, true, None));
            assert!(shell.core.pins.items.is_empty());
            assert_eq!(shell.core.pins.generation, generation);
        }
        let (mut shell, ctx, item) = setup();
        let remote = PinUpdate {
            channel: CHANNEL.into(),
            item: PinnedItem {
                message: Some(original()),
                ..Default::default()
            },
            pinned: false,
            event_ts: "20.0".into(),
        };
        assert!(shell.core.apply_pin_event("T1", &remote));
        let generation = shell.core.pins.generation;
        assert!(!apply(
            &mut shell,
            &ctx,
            CHANNEL,
            TS,
            item.clone(),
            true,
            None
        ));
        assert!(!shell.core.pins.is_pinned(CHANNEL, TS, true));
        assert_eq!(shell.core.pins.generation, generation);
        assert!(apply(
            &mut shell,
            &ctx,
            CHANNEL,
            TS,
            item,
            false,
            Some("20.0")
        ));
        assert!(!shell.core.pins.is_pinned(CHANNEL, TS, true));
    }
}
