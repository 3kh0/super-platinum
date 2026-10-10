//! Small renderer-neutral helpers for sidebar rows and message grouping.
//! Section layout lives in `sidebar_layout`.

use crate::slack::models::Channel;

/// Number of people in a group DM, parsed from the `mpdm-a--b--c-1` name.
pub fn group_member_count(c: &Channel) -> Option<usize> {
    let name = c.name.as_deref()?;
    let rest = name.strip_prefix("mpdm-")?;
    let rest = rest
        .rsplit_once('-')
        .and_then(|(prefix, suffix)| suffix.parse::<u32>().ok().map(|_| prefix))
        .unwrap_or(rest);
    let count = rest
        .split("--")
        .filter(|name| !name.trim().is_empty())
        .count();
    (count > 0).then_some(count)
}

/// Topic or purpose text stored on a channel when Slack provides it.
pub fn channel_topic_or_purpose(c: &Channel) -> Option<String> {
    for key in ["topic", "purpose"] {
        if let Some(value) = c.extra.get(key)
            && let Some(text) = value
                .get("value")
                .and_then(|v| v.as_str())
                .or_else(|| value.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
        {
            return Some(text.to_owned());
        }
    }
    None
}

pub fn is_app_message(msg: &crate::slack::models::Message) -> bool {
    msg.subtype.as_deref() == Some("bot_message")
        || msg.bot_id.is_some()
        || msg.bot_profile.is_some()
}

/// Consecutive messages from the same author within five minutes on the same day.
pub fn same_message_group(
    previous: &crate::slack::models::Message,
    current: &crate::slack::models::Message,
) -> bool {
    let has_author =
        previous.user.is_some() || previous.bot_id.is_some() || previous.username.is_some();
    if !has_author {
        return false;
    }
    let same_author = previous.user.as_deref() == current.user.as_deref()
        && previous.bot_id.as_deref() == current.bot_id.as_deref()
        && previous.username.as_deref() == current.username.as_deref();
    if !same_author {
        return false;
    }
    let (Some(previous_ts), Some(current_ts)) = (previous.ts.as_deref(), current.ts.as_deref())
    else {
        return false;
    };
    if crate::state::date_key_for_ts(previous_ts) != crate::state::date_key_for_ts(current_ts) {
        return false;
    }
    let previous_secs = crate::state::ts_key(previous_ts).0;
    let current_secs = crate::state::ts_key(current_ts).0;
    current_secs.saturating_sub(previous_secs) <= 300
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, HashMap};

    use super::*;
    use crate::state::{ChannelMessages, RealtimeStatus, Workspace, channel_display_name};

    fn channel(id: &str, name: &str) -> Channel {
        Channel {
            id: id.into(),
            name: Some(name.into()),
            is_channel: true,
            ..Default::default()
        }
    }

    fn dm(id: &str, name: &str) -> Channel {
        Channel {
            id: id.into(),
            name: Some(name.into()),
            is_im: true,
            ..Default::default()
        }
    }

    fn workspace(channels: Vec<Channel>, unreads: &[(&str, u32)]) -> Workspace {
        let mut messages = HashMap::new();
        for (channel, count) in unreads {
            messages.insert(
                (*channel).to_owned(),
                ChannelMessages {
                    unread_count: *count,
                    ..Default::default()
                },
            );
        }
        Workspace {
            team_id: "T".into(),
            name: "Test".into(),
            url: "https://test.slack.com".into(),
            self_user_id: "U_SELF".into(),
            activity_unread_count: None,
            channels: channels
                .into_iter()
                .map(|channel| (channel.id.clone(), channel))
                .collect::<BTreeMap<_, _>>(),
            starred_order: Vec::new(),
            dm_order: Vec::new(),
            recent_channels: Vec::new(),
            last_active_channel: None,
            priority_scores: BTreeMap::new(),
            frecency: BTreeMap::new(),
            priority_sidebar_section: false,
            vip_users: std::collections::HashSet::new(),
            sidebar: Default::default(),
            notifications: Default::default(),
            users: HashMap::new(),
            usergroups: HashMap::new(),
            custom_emoji: HashMap::new(),
            messages,
            typing: HashMap::new(),
            presence: HashMap::new(),
            self_dnd: Default::default(),
            active_huddles: HashMap::new(),
            rt: RealtimeStatus::default(),
            rt_generation: 0,
        }
    }

    #[test]
    fn group_member_count_parses_mpdm_name() {
        let mut mpdm = dm("G_MPDM", "mpdm-aarav54897--echo--alanlichen1-1");
        mpdm.is_im = false;
        mpdm.is_mpim = true;
        assert_eq!(group_member_count(&mpdm), Some(3));

        let public = channel("C_PUBLIC", "general");
        assert_eq!(group_member_count(&public), None);
    }

    #[test]
    fn dm_labels_do_not_use_private_channel_formatting() {
        let mut mpdm = dm("G_MPDM", "mpdm-aarav54897--echo--alanlichen1-1");
        mpdm.is_im = false;
        mpdm.is_mpim = true;
        mpdm.is_group = true;
        mpdm.is_private = true;
        let ws = workspace(vec![mpdm], &[]);

        let label = channel_display_name(&ws, ws.channels.get("G_MPDM").unwrap());

        assert_eq!(label.trim(), "aarav54897, echo, alanlichen1");
        assert!(!label.contains("mpdm-"));
    }
}
