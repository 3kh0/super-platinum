use crate::slack::models::{Message as SlackMessage, Reaction};

use super::util::non_empty;

pub fn message_text(msg: &SlackMessage) -> String {
    if let Some(text) = non_empty(msg.text.as_deref()) {
        return text.to_owned();
    }
    if !msg.files.is_empty() || !msg.attachments.is_empty() {
        return String::new();
    }
    if let Some(subtype) = non_empty(msg.subtype.as_deref()) {
        return format!("[{subtype}]");
    }
    if !msg.blocks.is_empty() {
        return "[rich message]".to_owned();
    }
    "[no text]".to_owned()
}

pub fn visible_message(msg: SlackMessage) -> SlackMessage {
    if msg.subtype.as_deref() != Some("message_replied") {
        return msg;
    }

    let Some(nested) = msg.message.clone() else {
        return msg;
    };
    let mut nested = *nested;
    nested.team = nested.team.or(msg.team);
    nested.channel = nested.channel.or(msg.channel);
    nested.reply_count = nested.reply_count.or(msg.reply_count);
    nested.reply_users_count = nested.reply_users_count.or(msg.reply_users_count);
    nested.latest_reply = nested.latest_reply.or(msg.latest_reply);
    if nested.reply_users.is_empty() {
        nested.reply_users = msg.reply_users;
    }
    if nested.reactions.is_empty() {
        nested.reactions = msg.reactions;
    }
    if nested.files.is_empty() {
        nested.files = msg.files;
    }
    if nested.attachments.is_empty() {
        nested.attachments = msg.attachments;
    }
    nested
}

pub fn is_channel_timeline_visible(msg: &SlackMessage) -> bool {
    if msg.subtype.as_deref() == Some("message_replied") {
        return false;
    }
    match (msg.thread_ts.as_deref(), msg.ts.as_deref()) {
        (Some(root), Some(ts)) if root != ts => msg.subtype.as_deref() == Some("thread_broadcast"),
        _ => true,
    }
}

pub fn reaction_has_user(reaction: &Reaction, user: &str) -> bool {
    !user.is_empty() && reaction.users.iter().any(|u| u == user)
}

pub(crate) fn apply_message_reaction(
    msg: &mut SlackMessage,
    user: &str,
    name: &str,
    added: bool,
) -> bool {
    if name.is_empty() {
        return false;
    }

    if added {
        if let Some(reaction) = msg.reactions.iter_mut().find(|r| r.name == name) {
            if reaction_has_user(reaction, user) {
                return false;
            }
            if !user.is_empty() {
                reaction.users.push(user.to_owned());
            }
            reaction.count = reaction.count.saturating_add(1).max(1);
            return true;
        }
        msg.reactions.push(Reaction {
            name: name.to_owned(),
            users: if user.is_empty() {
                Vec::new()
            } else {
                vec![user.to_owned()]
            },
            count: 1,
            ..Default::default()
        });
        return true;
    }

    let Some(i) = msg.reactions.iter().position(|r| r.name == name) else {
        return false;
    };
    let reaction = &mut msg.reactions[i];
    let before_users = reaction.users.len();
    reaction.users.retain(|u| u != user);
    let removed_known_user = before_users != reaction.users.len();
    if removed_known_user || reaction.users.is_empty() {
        reaction.count = reaction.count.saturating_sub(1);
    }
    if reaction.count == 0 {
        msg.reactions.remove(i);
    }
    true
}

pub(crate) fn merge_message(existing: &mut SlackMessage, update: SlackMessage) {
    existing.user = update.user.or_else(|| existing.user.take());
    existing.bot_id = update.bot_id.or_else(|| existing.bot_id.take());
    existing.username = update.username.or_else(|| existing.username.take());
    existing.bot_profile = update.bot_profile.or_else(|| existing.bot_profile.take());
    existing.icons = update.icons.or_else(|| existing.icons.take());
    existing.kind = update.kind.or_else(|| existing.kind.take());
    existing.subtype = update.subtype.or_else(|| existing.subtype.take());
    existing.client_msg_id = update
        .client_msg_id
        .or_else(|| existing.client_msg_id.take());
    existing.text = update.text.or_else(|| existing.text.take());
    existing.team = update.team.or_else(|| existing.team.take());
    existing.channel = update.channel.or_else(|| existing.channel.take());
    existing.thread_ts = update.thread_ts.or_else(|| existing.thread_ts.take());
    existing.parent_user_id = update
        .parent_user_id
        .or_else(|| existing.parent_user_id.take());
    existing.reply_count = update.reply_count.or(existing.reply_count);
    existing.reply_users_count = update.reply_users_count.or(existing.reply_users_count);
    existing.latest_reply = update.latest_reply.or_else(|| existing.latest_reply.take());
    if !update.reply_users.is_empty() {
        existing.reply_users = update.reply_users;
    }
    if !update.reactions.is_empty() {
        existing.reactions = update.reactions;
    }
    if !update.blocks.is_empty() {
        existing.blocks = update.blocks;
    }
    if !update.files.is_empty() {
        existing.files = update.files;
    }
    if !update.attachments.is_empty() {
        existing.attachments = update.attachments;
    }
    existing.edited = update.edited.or_else(|| existing.edited.take());
    existing.message = update.message.or_else(|| existing.message.take());
    existing.extra.extend(update.extra);
}

/// Every user id a message references anywhere the renderer will need a display
/// name or avatar for: the author, Block Kit `user` elements, `<@Uxxx>` mentions
/// in text/mrkdwn leaves, and attachment (unfurl) authors.
pub fn collect_message_user_ids(msg: &SlackMessage, ids: &mut std::collections::HashSet<String>) {
    let mut push = |id: &str| {
        let id = id.trim();
        // Slack user ids are `U…` (or `W…` on enterprise grids). Bot ids
        // (`B…`) are not resolvable through users.info.
        if id.len() > 1 && id.starts_with(['U', 'W']) {
            ids.insert(id.to_owned());
        }
    };
    if let Some(user) = msg.user.as_deref() {
        push(user);
    }
    if let Some(user) = msg
        .bot_profile
        .as_ref()
        .and_then(|profile| profile.user_id.as_deref())
    {
        push(user);
    }
    // The thread reply bar shows replier avatars.
    for user in &msg.reply_users {
        push(user);
    }
    if let Some(text) = msg.text.as_deref() {
        visit_text_user_mentions(text, &mut push);
    }
    for block in &msg.blocks {
        visit_block_user_ids(block, &mut push);
    }
    for attachment in &msg.attachments {
        if let Some(author) = attachment.author_id.as_deref() {
            push(author);
        }
        for field in [attachment.text.as_deref(), attachment.pretext.as_deref()]
            .into_iter()
            .flatten()
        {
            visit_text_user_mentions(field, &mut push);
        }
        for block in &attachment.blocks {
            visit_block_user_ids(block, &mut push);
        }
    }
}

/// Every custom (non-standard) emoji name a message references: Block Kit
/// `emoji` elements, `:shortcode:` runs in text leaves, reactions, and the same
/// again inside attachments.
pub fn collect_message_emoji_names(
    msg: &SlackMessage,
    names: &mut std::collections::HashSet<String>,
) {
    let mut push = |name: &str| {
        let base = name.split("::").next().unwrap_or(name);
        if !base.is_empty() && !super::emoji::is_standard_emoji(base) {
            names.insert(base.to_owned());
        }
    };
    if let Some(text) = msg.text.as_deref() {
        super::emoji::visit_emoji_names_in_text(text, &mut push);
    }
    for reaction in &msg.reactions {
        push(&reaction.name);
    }
    for block in &msg.blocks {
        visit_block_emoji_names(block, &mut push);
    }
    for attachment in &msg.attachments {
        for field in [
            attachment.text.as_deref(),
            attachment.pretext.as_deref(),
            attachment.title.as_deref(),
            attachment.footer.as_deref(),
        ]
        .into_iter()
        .flatten()
        {
            super::emoji::visit_emoji_names_in_text(field, &mut push);
        }
        for block in &attachment.blocks {
            visit_block_emoji_names(block, &mut push);
        }
    }
}

/// Channel ids a message points at, so unfurl footers and `#channel` chips can
/// resolve a name instead of printing a raw id.
pub fn collect_message_channel_ids(
    msg: &SlackMessage,
    ids: &mut std::collections::HashSet<String>,
) {
    fn visit(value: &serde_json::Value, ids: &mut std::collections::HashSet<String>) {
        match value {
            serde_json::Value::Object(object) => {
                if object.get("type").and_then(serde_json::Value::as_str) == Some("channel")
                    && let Some(id) = object.get("channel_id").and_then(serde_json::Value::as_str)
                {
                    ids.insert(id.to_owned());
                }
                for child in object.values() {
                    visit(child, ids);
                }
            }
            serde_json::Value::Array(values) => values.iter().for_each(|child| visit(child, ids)),
            _ => {}
        }
    }
    for block in &msg.blocks {
        visit(block, ids);
    }
    for attachment in &msg.attachments {
        if let Some(channel) = attachment.channel_id.as_deref().filter(|id| !id.is_empty()) {
            ids.insert(channel.to_owned());
        }
        for block in &attachment.blocks {
            visit(block, ids);
        }
    }
}

fn visit_text_user_mentions(text: &str, push: &mut impl FnMut(&str)) {
    let mut rest = text;
    while let Some(start) = rest.find("<@") {
        let after = &rest[start + 2..];
        let Some(end) = after.find('>') else { break };
        let token = &after[..end];
        push(token.split_once('|').map_or(token, |(id, _)| id));
        rest = &after[end + 1..];
    }
}

fn visit_block_user_ids(value: &serde_json::Value, push: &mut impl FnMut(&str)) {
    match value {
        serde_json::Value::Object(object) => {
            match object.get("type").and_then(serde_json::Value::as_str) {
                Some("user") => {
                    if let Some(id) = object.get("user_id").and_then(serde_json::Value::as_str) {
                        push(id);
                    }
                }
                Some("mrkdwn" | "plain_text" | "text") => {
                    if let Some(text) = object.get("text").and_then(serde_json::Value::as_str) {
                        visit_text_user_mentions(text, push);
                    }
                }
                _ => {}
            }
            for child in object.values() {
                visit_block_user_ids(child, push);
            }
        }
        serde_json::Value::Array(values) => values
            .iter()
            .for_each(|child| visit_block_user_ids(child, push)),
        _ => {}
    }
}

fn visit_block_emoji_names(value: &serde_json::Value, push: &mut impl FnMut(&str)) {
    match value {
        serde_json::Value::Object(object) => {
            match object.get("type").and_then(serde_json::Value::as_str) {
                Some("emoji") => {
                    if let Some(name) = object.get("name").and_then(serde_json::Value::as_str) {
                        push(name);
                    }
                }
                Some("mrkdwn" | "plain_text" | "text") => {
                    if let Some(text) = object.get("text").and_then(serde_json::Value::as_str) {
                        super::emoji::visit_emoji_names_in_text(text, &mut *push);
                    }
                }
                _ => {}
            }
            for child in object.values() {
                visit_block_emoji_names(child, push);
            }
        }
        serde_json::Value::Array(values) => values
            .iter()
            .for_each(|child| visit_block_emoji_names(child, push)),
        _ => {}
    }
}
