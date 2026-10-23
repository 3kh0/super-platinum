//! Thread projection shares the channel's author-grouping rule while keeping
//! the parent, first reply and unloaded page boundaries visually distinct.
use crate::{media::MediaRegistry, model::MessageVm};
use super_platinum_core::{
    slack_lists::comments::edit_seed::prepare::StandardEmoji,
    state::{ChannelMessages, Workspace},
    thread_history::ThreadHistory,
};

pub(crate) fn project(
    workspace: &Workspace,
    bag: &ChannelMessages,
    history: &ThreadHistory,
    root: &str,
    media: &MediaRegistry,
) -> Vec<MessageVm> {
    let mut window = crate::thread_window::ThreadWindow::default();
    window.sync("projection", bag, history, root, None, true);
    project_window(workspace, bag, history, root, media, &window)
}

pub(crate) fn project_window(
    workspace: &Workspace,
    bag: &ChannelMessages,
    history: &ThreadHistory,
    root: &str,
    media: &MediaRegistry,
    window: &crate::thread_window::ThreadWindow,
) -> Vec<MessageVm> {
    project_window_with_standard(workspace, bag, history, root, media, window, None)
}

pub(crate) fn project_window_with_standard(
    workspace: &Workspace,
    bag: &ChannelMessages,
    history: &ThreadHistory,
    root: &str,
    media: &MediaRegistry,
    window: &crate::thread_window::ThreadWindow,
    standard: Option<&StandardEmoji>,
) -> Vec<MessageVm> {
    if !window.source_shape_matches(bag) {
        return Vec::new();
    }
    let mut previous: Option<(usize, &super_platinum_core::slack::models::Message)> = None;
    window
        .selected_indices()
        .iter()
        .map(|&index| {
            let row = window.rows.get(index)?;
            let message = bag.messages.get(row.source_index)?;
            let ts = message.ts.as_deref()?;
            // Source positions are valid only for the synchronized canonical bag.
            // Reject the entire projection on drift so VM positions never shift.
            if ts != row.ts
                || bag.is_pending(ts) != row.pending
                || history.contains(root, ts) != row.history
                || !(history.visible(root, ts) || row.pending)
            {
                return None;
            }
            let mut vm = crate::message_vm::message_vm_with_standard(
                workspace,
                message,
                media,
                row.pending,
                standard,
            );
            vm.compact = previous.is_some_and(|(previous_index, previous)| {
                row.previous_source_index == Some(previous_index)
                    && previous.ts.as_deref() != Some(root)
                    && previous
                        .ts
                        .as_deref()
                        .is_some_and(|ts| history.contains(root, ts) == row.history)
                    && super_platinum_core::state::same_message_group(previous, message)
            });
            previous = Some((row.source_index, message));
            Some(vm)
        })
        .collect::<Option<Vec<_>>>()
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use super_platinum_core::slack::models::Message;
    fn message(ts: &str, user: &str) -> Message {
        Message {
            ts: Some(ts.into()),
            user: Some(user.into()),
            text: Some("Synthetic reply".into()),
            ..Default::default()
        }
    }
    #[test]
    fn parent_first_reply_author_change_and_time_gap_keep_headers() {
        let core = crate::fixture::fixture_core();
        let mut bag = ChannelMessages::default();
        for (ts, user) in [
            ("100.0", "U1"),
            ("101.0", "U1"),
            ("102.0", "U1"),
            ("103.0", "U2"),
            ("104.0", "U2"),
            ("500.0", "U2"),
        ] {
            bag.upsert(message(ts, user));
        }
        let vms = project(
            &core.workspaces["T1"],
            &bag,
            &ThreadHistory::default(),
            "100.0",
            &MediaRegistry::default(),
        );
        assert_eq!(
            vms.iter().map(|m| m.compact).collect::<Vec<_>>(),
            [false, false, true, false, true, false]
        );
    }
    #[test]
    fn pending_reply_uses_thread_bag_and_page_gap_starts_a_new_group() {
        let core = crate::fixture::fixture_core();
        let mut bag = ChannelMessages::default();
        for ts in [
            "100.0",
            "101.0",
            "102.0",
            "150.0",
            "200.000000",
            "201.000000",
        ] {
            bag.upsert(message(ts, "U1"));
        }
        bag.pending.push("201.000000".into());
        let mut history = ThreadHistory::default();
        history.watch_live(200);
        history.start(&ThreadHistory::initial(Some("101.0"),200),"100.0",&serde_json::from_value(serde_json::json!({"messages":[{"ts":"100.0"},{"ts":"101.0"},{"ts":"102.0"}],"has_more":true})).unwrap());
        let vms = project(
            &core.workspaces["T1"],
            &bag,
            &history,
            "100.0",
            &MediaRegistry::default(),
        );
        assert_eq!(
            vms.iter().map(|m| m.ts.as_str()).collect::<Vec<_>>(),
            ["100.0", "101.0", "102.0", "200.000000", "201.000000"]
        );
        assert!(
            !vms[3].compact,
            "first arrival below a page gap needs its author"
        );
        assert!(
            vms[4].pending,
            "thread pending state must not come from channel bags"
        );
        bag.pending.clear();
        let vms = project(
            &core.workspaces["T1"],
            &bag,
            &history,
            "100.0",
            &MediaRegistry::default(),
        );
        assert!(!vms[4].pending);
        assert!(vms[4].compact);
    }
    #[test]
    fn skipped_replies_start_a_group_and_unread_boundary_stays_exact() {
        let mut shell = crate::state::ShellState::fixture(MediaRegistry::default());
        let root = "1719800400.000000";
        let mut bag = fixture_bag(2000);
        bag.last_read = Some("1719800430.000000".into());
        shell.thread_root = Some(root.into());
        shell.core.threads.insert(
            (
                "T1".into(),
                shell.core.active_channel.clone().unwrap(),
                root.into(),
            ),
            bag,
        );
        shell.refresh_thread_from_core();
        crate::thread_unread::annotate(&mut shell);
        assert_eq!(shell.thread_messages.len(), 101);
        assert!(!shell.thread_messages[1].compact);
        assert!(
            !shell.thread_messages.iter().any(|m| m.show_unread_divider),
            "offscreen unread divider must not move to visible tail"
        );
        shell.thread_at_bottom = false;
        shell.thread_target = Some("1719800431.000000".into());
        shell.refresh_thread_from_core();
        crate::thread_unread::annotate(&mut shell);
        assert_eq!(
            shell
                .thread_messages
                .iter()
                .find(|m| m.show_unread_divider)
                .map(|m| m.ts.as_str()),
            Some("1719800431.000000")
        );
    }
}

#[cfg(test)]
mod benchmark;

// Shared synthetic data for the named disconnected fixtures and timing probe.
pub(crate) fn fixture_bag(count: usize) -> ChannelMessages {
    let mut bag = super_platinum_core::state::ChannelMessages::default();
    bag.messages=(0..=count).map(|i|serde_json::from_value(serde_json::json!({
   "ts":format!("{}.000000",1719800400+i),"user":if i%4==0 {"U2"}else{"U1"},
   "text":"A mixed thread reply with *formatting* and <https://example.com|link> :wave:",
   "blocks":[{"type":"rich_text","elements":[{"type":"rich_text_section","elements":[{"type":"text","text":format!("Synthetic reply {i} with rich text. ")},{"type":"text","text":"Emphasis","style":{"bold":true}},{"type":"link","url":"https://example.com","text":"Reference"}]}]}],
   "reactions":if i%3==0 {serde_json::json!([{"name":"eyes","count":2,"users":["U1","U2"]}])}else{serde_json::json!([])},
   "attachments":if i%5==0 {serde_json::json!([{"title":"Preview","text":"A mixed-content preview with a description.","footer":"Synthetic fixture"}])}else{serde_json::json!([])}
  })).unwrap()).collect();

    bag
}

#[cfg(test)]
mod index_tests;
