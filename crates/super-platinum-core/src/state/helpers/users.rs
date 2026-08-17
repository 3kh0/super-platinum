use crate::slack::models::{Channel, Message as SlackMessage, User};
use crate::state::Workspace;

use super::util::non_empty;

pub fn dm_user_id(channel: &Channel) -> Option<&str> {
    if let Some(user) = channel.user.as_deref() {
        return Some(user);
    }
    for key in ["user", "user_id"] {
        if let Some(user) = channel.extra.get(key).and_then(serde_json::Value::as_str) {
            return Some(user);
        }
    }
    None
}

pub fn display_name(user: Option<&User>, user_id: &str) -> String {
    let Some(user) = user else {
        return user_id.to_owned();
    };
    if let Some(profile) = &user.profile {
        if let Some(dn) = non_empty(profile.display_name.as_deref()) {
            return dn.to_owned();
        }
        if let Some(rn) = non_empty(profile.real_name.as_deref()) {
            return rn.to_owned();
        }
    }
    if let Some(rn) = non_empty(user.real_name.as_deref()) {
        return rn.to_owned();
    }
    if let Some(name) = non_empty(user.name.as_deref()) {
        return name.to_owned();
    }
    user_id.to_owned()
}

pub fn user_avatar_url(user: &User) -> Option<&str> {
    let profile = user.profile.as_ref()?;
    non_empty(profile.image_48.as_deref())
        .or_else(|| non_empty(profile.image_72.as_deref()))
        .or_else(|| non_empty(profile.image_32.as_deref()))
        .or_else(|| non_empty(profile.image_24.as_deref()))
        .or_else(|| non_empty(profile.image_192.as_deref()))
        .or_else(|| non_empty(profile.image_512.as_deref()))
        .or_else(|| non_empty(profile.image_original.as_deref()))
}

pub fn user_profile_image_url(user: &User) -> Option<&str> {
    let profile = user.profile.as_ref()?;
    non_empty(profile.image_original.as_deref())
        .or_else(|| non_empty(profile.image_512.as_deref()))
        .or_else(|| non_empty(profile.image_192.as_deref()))
        .or_else(|| non_empty(profile.image_72.as_deref()))
        .or_else(|| non_empty(profile.image_48.as_deref()))
        .or_else(|| non_empty(profile.image_32.as_deref()))
        .or_else(|| non_empty(profile.image_24.as_deref()))
}

pub fn message_author_name(ws: &Workspace, msg: &SlackMessage) -> String {
    if let Some(user) = msg
        .user
        .as_deref()
        .filter(|_| msg.bot_profile.is_none() && msg.bot_id.is_none())
    {
        return ws.display_name(user);
    }
    if let Some(name) = non_empty(msg.username.as_deref()) {
        return name.to_owned();
    }
    if let Some(name) = msg
        .bot_profile
        .as_ref()
        .and_then(|profile| non_empty(profile.name.as_deref()))
    {
        return name.to_owned();
    }
    if let Some(user) = msg.user.as_deref() {
        return ws.display_name(user);
    }
    msg.bot_id
        .clone()
        .filter(|id| !id.trim().is_empty())
        .unwrap_or_else(|| "unknown".to_owned())
}

pub fn message_avatar(ws: &Workspace, msg: &SlackMessage) -> (Option<String>, Option<String>) {
    if let Some(user) = msg
        .user
        .as_deref()
        .filter(|_| msg.bot_profile.is_none() && msg.bot_id.is_none())
    {
        return (Some(user.to_owned()), ws.avatar_url(user));
    }
    if let Some((key, url)) = message_bot_avatar(msg) {
        return (Some(key), Some(url));
    }
    if let Some(user) = msg.user.as_deref() {
        return (Some(user.to_owned()), ws.avatar_url(user));
    }
    (None, None)
}

pub fn message_bot_avatar(msg: &SlackMessage) -> Option<(String, String)> {
    let source = msg
        .bot_id
        .as_deref()
        .or_else(|| {
            msg.bot_profile
                .as_ref()
                .and_then(|profile| profile.id.as_deref())
        })
        .or_else(|| {
            msg.bot_profile
                .as_ref()
                .and_then(|profile| profile.user_id.as_deref())
        })?;
    let url = msg
        .bot_profile
        .as_ref()
        .and_then(|profile| profile.icons.as_ref())
        .and_then(message_icon_url)
        .or_else(|| msg.icons.as_ref().and_then(message_icon_url))?
        .to_owned();
    let key = format!("bot-icon:{source}:{url}");
    Some((key, url))
}

pub fn message_icon_url(icons: &crate::slack::models::MessageIcons) -> Option<&str> {
    non_empty(icons.image_48.as_deref())
        .or_else(|| non_empty(icons.image_72.as_deref()))
        .or_else(|| non_empty(icons.image_64.as_deref()))
        .or_else(|| non_empty(icons.image_36.as_deref()))
        .or_else(|| non_empty(icons.image_192.as_deref()))
        .or_else(|| non_empty(icons.image_512.as_deref()))
        .or_else(|| non_empty(icons.image_original.as_deref()))
        .or_else(|| non_empty(icons.icon_url.as_deref()))
}
