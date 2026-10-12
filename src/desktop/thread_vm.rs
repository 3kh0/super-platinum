//! Thread projection shares the channel's author-grouping rule while keeping
//! the parent, first reply and unloaded page boundaries visually distinct.
use crate::{media::MediaRegistry, model::MessageVm};
use super_platinum_core::{
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
    let raw = bag
        .messages
        .iter()
        .filter(|message| {
            message
                .ts
                .as_deref()
                .is_some_and(|ts| history.visible(root, ts))
        })
        .collect::<Vec<_>>();
    raw.iter()
        .enumerate()
        .map(|(index, message)| {
            let mut vm = crate::message_vm::message_vm_with_pending(
                workspace,
                message,
                media,
                message.ts.as_deref().is_some_and(|ts| bag.is_pending(ts)),
            );
            vm.compact = index.checked_sub(1).is_some_and(|previous| {
                let previous = raw[previous];
                previous.ts.as_deref() != Some(root)
                    && previous.ts.as_deref().is_some_and(|ts| {
                        history.contains(root, ts) == history.contains(root, &vm.ts)
                    })
                    && super_platinum_core::state::same_message_group(previous, message)
            });
            vm
        })
        .collect()
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
}
