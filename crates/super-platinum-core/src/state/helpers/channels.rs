use std::collections::{HashMap, HashSet};

use crate::slack::models::{Channel, ChannelId};
use crate::state::Workspace;

use super::users::dm_user_id;
use super::util::non_empty;

pub(crate) fn append_unique(ids: &mut Vec<ChannelId>, id: ChannelId) {
    if !ids.iter().any(|existing| existing == &id) {
        ids.push(id);
    }
}

pub(crate) fn merge_channel_metadata(existing: &mut Channel, update: Channel) {
    if update
        .name
        .as_ref()
        .is_some_and(|name| !name.trim().is_empty())
    {
        existing.name = update.name;
    }
    existing.is_channel |= update.is_channel;
    existing.is_group |= update.is_group;
    existing.is_im |= update.is_im;
    existing.is_mpim |= update.is_mpim;
    existing.is_private |= update.is_private;
    existing.is_archived |= update.is_archived;
    existing.is_starred |= update.is_starred;
    existing.has_unreads |= update.has_unreads;
    existing.updated = update.updated.or(existing.updated);
    existing.user = update.user.or_else(|| existing.user.take());
    existing.unread_count = update.unread_count.or(existing.unread_count);
    existing.unread_count_display = update
        .unread_count_display
        .or(existing.unread_count_display);
    existing.mention_count = update.mention_count.or(existing.mention_count);
    existing.last_read = update.last_read.or_else(|| existing.last_read.take());
    existing.extra.extend(update.extra);
}

pub fn channel_label(channel: &Channel) -> String {
    if let Some(name) = non_empty(channel.name.as_deref()) {
        if channel.is_im {
            return name.to_owned();
        }
        return format!("#{name}");
    }
    if channel.is_im {
        return "direct message".to_owned();
    }
    channel.id.clone()
}

pub fn channel_display_name(ws: &Workspace, c: &Channel) -> String {
    if c.is_im || c.is_mpim {
        dm_label(ws, c)
    } else {
        channel_name(c)
    }
}

pub fn channel_name(c: &Channel) -> String {
    c.name
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .unwrap_or(c.id.as_str())
        .to_owned()
}

pub fn dm_label(ws: &Workspace, c: &Channel) -> String {
    if c.is_im
        && let Some(user) = dm_user_id(c)
    {
        return ws.display_name(user);
    }
    if c.is_mpim
        && let Some(name) = c.name.as_deref().and_then(mpdm_name_label)
    {
        return name;
    }
    channel_label(c).trim_start_matches('#').to_owned()
}

pub(crate) fn linked_list_order(
    sections: &[crate::slack::models::ChannelSection],
) -> Vec<&crate::slack::models::ChannelSection> {
    let by_id: HashMap<&str, &crate::slack::models::ChannelSection> = sections
        .iter()
        .map(|s| (s.channel_section_id.as_str(), s))
        .collect();
    let pointed_at: HashSet<&str> = sections
        .iter()
        .filter_map(|s| s.next_channel_section_id.as_deref())
        .collect();
    let Some(head) = sections
        .iter()
        .find(|s| !pointed_at.contains(s.channel_section_id.as_str()))
    else {
        return sections.iter().collect();
    };
    let mut out = Vec::with_capacity(sections.len());
    let mut seen = HashSet::new();
    let mut cursor = Some(head);
    while let Some(section) = cursor {
        if !seen.insert(section.channel_section_id.as_str()) {
            break;
        }
        out.push(section);
        cursor = section
            .next_channel_section_id
            .as_deref()
            .and_then(|id| by_id.get(id).copied());
    }
    for section in sections {
        if !seen.contains(section.channel_section_id.as_str()) {
            out.push(section);
        }
    }
    out
}

pub fn mpdm_name_label(name: &str) -> Option<String> {
    let rest = name.strip_prefix("mpdm-")?;
    let rest = rest
        .rsplit_once('-')
        .and_then(|(prefix, suffix)| suffix.parse::<u32>().ok().map(|_| prefix))
        .unwrap_or(rest);
    let names: Vec<_> = rest
        .split("--")
        .map(|name| name.replace('.', " "))
        .filter(|name| !name.trim().is_empty())
        .collect();
    (!names.is_empty()).then(|| names.join(", "))
}
