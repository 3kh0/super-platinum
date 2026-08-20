use super_platinum_core::MediaAssetKind;

use crate::media::MediaRegistry;
use crate::model::{ChannelVm, PresenceVm, SidebarSectionVm};

pub(crate) fn channel_vm(
    workspace: &super_platinum_core::state::Workspace,
    channel: &super_platinum_core::slack::models::Channel,
    media: &MediaRegistry,
) -> ChannelVm {
    let name = super_platinum_core::state::channel_display_name(workspace, channel);
    let unread_count = workspace.unread_total(channel);
    let unread = unread_count > 0;
    let mention_count = workspace
        .messages
        .get(&channel.id)
        .map(|messages| messages.mention_count)
        .or(channel.mention_count)
        .unwrap_or(0);
    let user_id = super_platinum_core::state::dm_user_id(channel).map(str::to_owned);
    let avatar = user_id.as_ref().and_then(|user| {
        workspace
            .avatar_url(user)
            .map(|url| media.register(MediaAssetKind::Avatar, &url, "image/jpeg", true))
    });
    let avatar_initials = name
        .chars()
        .find(|ch| ch.is_alphanumeric())
        .map(|ch| ch.to_uppercase().to_string())
        .unwrap_or_else(|| "?".into());
    let presence = PresenceVm::from_core(workspace.presence_for_channel(channel));
    ChannelVm {
        id: channel.id.clone(),
        name,
        unread,
        unread_count,
        mention_count,
        is_im: channel.is_im,
        is_mpim: channel.is_mpim,
        is_private: channel.is_private || channel.is_group,
        is_starred: workspace.is_starred_channel(channel),
        is_ext_shared: channel.is_ext_shared,
        member_count: super_platinum_core::state::group_member_count(channel),
        avatar,
        avatar_initials,
        presence,
        topic: super_platinum_core::state::channel_topic_or_purpose(channel),
        user_id,
    }
}

pub(crate) fn project_channels(
    workspace: &super_platinum_core::state::Workspace,
    media: &MediaRegistry,
) -> (Vec<ChannelVm>, Vec<SidebarSectionVm>, usize, Option<String>) {
    let active_id = workspace.last_active_channel.clone();
    let mut channels = workspace
        .channels
        .values()
        .filter(|channel| !channel.is_archived)
        .map(|channel| channel_vm(workspace, channel, media))
        .collect::<Vec<_>>();
    // Stable id → index map for section rows; do not alpha-sort.
    channels.sort_by(|left, right| left.id.cmp(&right.id));
    let index_by_id: std::collections::HashMap<&str, usize> = channels
        .iter()
        .enumerate()
        .map(|(index, channel)| (channel.id.as_str(), index))
        .collect();
    let active_channel = active_id
        .as_ref()
        .and_then(|id| index_by_id.get(id.as_str()).copied())
        .unwrap_or(0);
    let active_for_grouping = channels
        .get(active_channel)
        .map(|channel| channel.id.as_str());
    let sidebar_sections =
        super_platinum_core::state::grouped_sidebar_sections(workspace, active_for_grouping)
            .into_iter()
            .filter_map(|section| {
                let channel_indices = section
                    .channel_ids
                    .iter()
                    .filter_map(|id| index_by_id.get(id.as_str()).copied())
                    .collect::<Vec<_>>();
                if channel_indices.is_empty() {
                    return None;
                }
                Some(SidebarSectionVm {
                    id: section.id,
                    kind: section.kind,
                    title: section.title,
                    channel_indices,
                })
            })
            .collect();
    (channels, sidebar_sections, active_channel, active_id)
}

pub(crate) fn project_messages_for_channel(
    workspace: &super_platinum_core::state::Workspace,
    channel_id: &str,
    media: &MediaRegistry,
) -> Vec<crate::model::MessageVm> {
    let Some(bag) = workspace.messages.get(channel_id) else {
        return Vec::new();
    };
    let raw: Vec<&super_platinum_core::slack::models::Message> = bag
        .messages
        .iter()
        .filter(|message| super_platinum_core::state::is_channel_timeline_visible(message))
        .collect();
    let mut messages = raw
        .iter()
        .map(|message| crate::message_vm::message_vm(workspace, message, media))
        .collect::<Vec<_>>();
    crate::message_vm::annotate_timeline(&mut messages, &raw, bag.last_read.as_deref());
    messages
}
