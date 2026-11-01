//! Bounded DATA retention for rows a native reader already displayed. This
//! public domain operation does not create a native receipt or any authority.
use super::*;
use std::collections::BTreeMap;

pub enum VisibleChange {
    Append(String),
    Remove(String),
}
fn timestamp(v: &str) -> bool {
    let Some((a, b)) = v.split_once('.') else {
        return false;
    };
    a.len() == 10
        && b.len() == 6
        && a.bytes().chain(b.bytes()).all(|c| c.is_ascii_digit())
        && a.parse::<u64>().is_ok_and(|n| n > 0)
}
impl CoreAppState {
    /// Restore at most200 previously visible bodies beside actual post-reducer
    /// copies. Callers must independently own the original preimage and native
    /// transition. No current/ready flag, IO or native capability is returned.
    pub fn preserve_thread_feed_context(
        &mut self,
        team: &str,
        channel: &str,
        root: &str,
        previous: &[Message],
        change: &VisibleChange,
    ) -> bool {
        if self.active_team.as_deref() != Some(team)
            || !self.threads_view.matches_scope(self.account_epoch, team)
            || !self.threads_view.loaded
            || previous.is_empty()
            || previous.len() > 200
            || !timestamp(root)
        {
            return false;
        }
        let positions = self
            .threads_view
            .items
            .iter()
            .enumerate()
            .filter(|(_, i)| {
                i.root_msg.channel.as_deref() == Some(channel)
                    && i.root_msg.ts.as_deref() == Some(root)
            })
            .map(|(n, _)| n)
            .collect::<Vec<_>>();
        if positions.len() != 1 {
            return false;
        }
        let index = positions[0];
        let item = &self.threads_view.items[index];
        let Some(workspace) = self.workspaces.get(team) else {
            return false;
        };
        let channel_bag = workspace.messages.get(channel);
        let thread_bag = self
            .threads
            .get(&(team.into(), channel.into(), root.into()));
        let deleted = |ts: &str| {
            channel_bag.is_some_and(|b| b.was_deleted(ts))
                || thread_bag.is_some_and(|b| b.was_deleted(ts))
        };
        let context = |m: &Message| {
            m.kind.as_deref() == Some("message")
                && m.channel.as_deref().is_none_or(|v| v == channel)
                && m.team.as_deref().is_none_or(|v| v == team)
                && m.thread_ts.as_deref().is_none_or(|v| v == root)
                && !crate::channel_threads::hidden(m)
        };
        if !context(&item.root_msg)
            || item.root_msg.thread_ts.as_deref() != Some(root)
            || item.root_msg.extra.get("subscribed") != Some(&serde_json::json!(true))
            || deleted(root)
            || item.latest_replies.len() + item.unread_replies.len() > 1000
        {
            return false;
        }
        let changed = match change {
            VisibleChange::Append(ts) | VisibleChange::Remove(ts) => ts,
        };
        if !timestamp(changed) || changed.as_str() <= root {
            return false;
        }
        let valid = |m: &Message| {
            context(m)
                && m.ts
                    .as_deref()
                    .is_some_and(|t| timestamp(t) && t > root && !deleted(t))
        };
        match change {
            VisibleChange::Append(ts) => {
                if !item
                    .latest_replies
                    .iter()
                    .any(|m| m.ts.as_ref() == Some(ts) && valid(m))
                    || previous.iter().any(|m| m.ts.as_ref() == Some(ts))
                {
                    return false;
                }
            }
            VisibleChange::Remove(ts) => {
                if item.replies().iter().any(|m| m.ts.as_ref() == Some(ts)) {
                    return false;
                }
            }
        }
        let mut rows = BTreeMap::new();
        for message in previous {
            if matches!(change,VisibleChange::Remove(ts) if message.ts.as_ref()==Some(ts)) {
                continue;
            }
            if !valid(message) {
                return false;
            }
            if rows
                .insert(message.ts.clone().unwrap(), message.clone())
                .is_some()
            {
                return false;
            }
        }
        // Existing post-reducer copies win, preserving partial merges/reactions.
        for message in &item.latest_replies {
            if !valid(message) {
                return false;
            }
            rows.insert(message.ts.clone().unwrap(), message.clone());
        }
        if rows.len() > 1000 {
            return false;
        }
        // Do not prune existing larger original packets on a deletion. Newly
        // restored context is bounded to200 when ordinary preview trim ran.
        let budget = item.latest_replies.len().max(200);
        let skip = rows.len().saturating_sub(budget);
        let rows = rows.into_values().skip(skip).collect();
        self.threads_view.items[index].latest_replies = rows;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    const ROOT: &str = "1719800000.000000";
    fn message(n: usize) -> Message {
        serde_json::from_value(json!({"type":"message","channel":"C1","team":"T1","thread_ts":ROOT,"ts":format!("1719800001.{n:06}"),"user":"U1","text":format!("reply{n}")})).unwrap()
    }
    fn core(rows: usize) -> CoreAppState {
        let mut c = CoreAppState::new(Default::default());
        c.active_team = Some("T1".into());
        c.workspaces.insert(
            "T1".into(),
            crate::state::Workspace::from_session(&crate::config::WorkspaceSession {
                team_id: "T1".into(),
                user_id: "U1".into(),
                name: "Fixture".into(),
                url: "https://fixture.invalid".into(),
                token: "synthetic".into(),
                enterprise_id: None,
            }),
        );
        c.threads_view.ensure_scope(c.account_epoch, "T1");
        c.threads_view.loaded = true;
        c.threads_view.items.push(serde_json::from_value(json!({"root_msg":{"type":"message","channel":"C1","team":"T1","thread_ts":ROOT,"ts":ROOT,"subscribed":true,"reply_count":rows},"latest_replies":(1..=rows).map(message).collect::<Vec<_>>()})).unwrap());
        c
    }
    #[test]
    fn thread_context_data_retains_live_bodies_without_changing_default_observe_and_enforces_boundaries()
     {
        let mut c = core(160);
        c.threads_view.observe(&RtEvent::ReactionAdded {
            channel: "C1".into(),
            ts: message(1).ts.unwrap(),
            user: "U2".into(),
            reaction: "eyes".into(),
        });
        let mut edit = message(1);
        edit.text = Some("Edited earlier".into());
        c.threads_view.observe(&RtEvent::MessageChanged {
            channel: "C1".into(),
            message: edit,
            original_update: None,
        });
        let before = c.threads_view.items[0].latest_replies.clone();
        c.threads_view.observe(&RtEvent::Message(message(161)));
        assert_eq!(c.threads_view.items[0].latest_replies.len(), 3);
        assert!(c.preserve_thread_feed_context(
            "T1",
            "C1",
            ROOT,
            &before,
            &VisibleChange::Append(message(161).ts.unwrap())
        ));
        assert_eq!(c.threads_view.items[0].latest_replies.len(), 161);
        assert_eq!(
            c.threads_view.items[0].latest_replies[0].text.as_deref(),
            Some("Edited earlier")
        );
        assert_eq!(
            c.threads_view.items[0].latest_replies[0].reactions[0].count,
            1
        );
        let before = c.threads_view.items[0].latest_replies.clone();
        let removed = message(2).ts.unwrap();
        c.threads_view.observe(&RtEvent::MessageDeleted {
            channel: "C1".into(),
            deleted_ts: removed.clone(),
        });
        assert!(c.preserve_thread_feed_context(
            "T1",
            "C1",
            ROOT,
            &before,
            &VisibleChange::Remove(removed.clone())
        ));
        assert_eq!(c.threads_view.items[0].latest_replies.len(), 160);
        assert!(
            !c.threads_view.items[0]
                .replies()
                .iter()
                .any(|m| m.ts.as_ref() == Some(&removed))
        );
        let snapshot = serde_json::to_value(&c.threads_view.items).unwrap();
        assert!(!c.preserve_thread_feed_context(
            "T2",
            "C1",
            ROOT,
            &before,
            &VisibleChange::Remove(removed)
        ));
        assert_eq!(
            serde_json::to_value(&c.threads_view.items).unwrap(),
            snapshot
        );
        let mut c = core(200);
        let before = c.threads_view.items[0].latest_replies.clone();
        c.threads_view.observe(&RtEvent::Message(message(201)));
        assert!(c.preserve_thread_feed_context(
            "T1",
            "C1",
            ROOT,
            &before,
            &VisibleChange::Append(message(201).ts.unwrap())
        ));
        assert_eq!(c.threads_view.items[0].latest_replies.len(), 200);
        assert_eq!(c.threads_view.items[0].latest_replies[0].ts, message(2).ts);
    }
}
