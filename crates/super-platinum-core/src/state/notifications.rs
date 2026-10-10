//! Desktop delivery rules from Slack's `Fpyc`, `zN7l`, `Mvkl`, and `uSWp`
//! client modules. Preferences come from userBoot and pref_change, not local
//! copies of the user's settings. Mobile and Activity preferences are separate.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::slack::models::{BootPrefs, Message};
use crate::state::{Presence, Workspace};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NotificationConfig {
    #[serde(default)]
    pub loaded: bool,
    #[serde(default)]
    pub global: BTreeMap<String, Value>,
    #[serde(default)]
    pub channels: BTreeMap<String, BTreeMap<String, Value>>,
    #[serde(default)]
    pub user: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationOptions {
    pub preview: bool,
    /// Slack's asset name; None means a silent banner.
    pub sound: Option<String>,
}

const USER_PREFS: &[&str] = &[
    "priority_dnd_override",
    "mute_sounds",
    "mute_huddle_sounds",
    "huddle_invite_sound_v2",
    "huddles_allow_smart_notif",
];

impl NotificationConfig {
    pub fn apply_prefs(&mut self, prefs: &BootPrefs) {
        if let Some(value) = prefs.extra.get("all_notifications_prefs") {
            self.apply_change("all_notifications_prefs", value.clone());
        }
        for name in USER_PREFS {
            if let Some(value) = prefs.extra.get(*name) {
                self.apply_change(name, value.clone());
            }
        }
    }

    pub fn apply_change(&mut self, name: &str, value: Value) {
        let value = match &value {
            Value::String(text) => serde_json::from_str(text).unwrap_or(value),
            _ => value,
        };
        if name == "all_notifications_prefs" {
            // Malformed updates must not replace working preferences.
            if !value.is_object() {
                return;
            }
            self.global = value
                .get("global")
                .cloned()
                .and_then(|v| serde_json::from_value(v).ok())
                .unwrap_or_default();
            self.channels = value
                .get("channels")
                .cloned()
                .and_then(|v| serde_json::from_value(v).ok())
                .unwrap_or_default();
            self.loaded = true;
        } else if USER_PREFS.contains(&name) {
            self.user.insert(name.into(), value);
        }
    }

    pub fn desktop_enabled(&self, channel: &str) -> bool {
        self.loaded
            && self
                .channel_flag(channel, "desktop_push_enabled")
                .unwrap_or_else(|| {
                    self.global_flag("global_desktop_push_enabled")
                        .unwrap_or(true)
                })
    }

    pub fn muted(&self, channel: &str, now: i64) -> bool {
        self.channel_flag(channel, "muted").unwrap_or(false)
            || self
                .channels
                .get(channel)
                .and_then(|p| p.get("temp_mute_expiration_ts"))
                .and_then(|v| v.as_i64().or_else(|| v.as_str()?.parse().ok()))
                .is_some_and(|end| end > now)
    }

    fn setting(&self, channel: &str, direct: bool) -> &str {
        let explicit = self
            .channels
            .get(channel)
            .and_then(|p| p.get("desktop"))
            .and_then(Value::as_str);
        let default = if direct {
            self.global_text("global_mpdm_desktop")
                .unwrap_or("everything")
        } else {
            self.global_text("global_desktop").unwrap_or("mentions_dms")
        };
        // Slack migrated "nothing" to a push-enabled boolean. The message
        // filter stays mentions_dms so Activity can continue receiving mentions.
        match explicit.unwrap_or(default) {
            "nothing" => "mentions_dms",
            setting => setting,
        }
    }

    pub fn message_options(
        &self,
        workspace: &Workspace,
        message: &Message,
        now: i64,
    ) -> Option<NotificationOptions> {
        let channel = message.channel.as_deref()?;
        if !self.desktop_enabled(channel)
            || message.user.as_deref() == Some(&workspace.self_user_id)
            || flag(message, "no_display")
            || flag(message, "no_notifications")
            || flag(message, "is_ephemeral")
            || flag(message, "dnd_suppressed")
            || message.extra.get("streaming_state").and_then(Value::as_str) == Some("in_progress")
            || message.subtype.as_deref().is_some_and(|s| {
                !matches!(
                    s,
                    "bot_message" | "file_share" | "me_message" | "thread_broadcast"
                )
            })
        {
            return None;
        }
        let direct = workspace
            .channels
            .get(channel)
            .is_some_and(|c| c.is_im || c.is_mpim);
        let thread = message
            .thread_ts
            .as_deref()
            .is_some_and(|root| message.ts.as_deref() != Some(root));
        let muted = self.muted(channel, now);
        let personal = has_personal_mention(workspace, message, true);
        let setting = self.setting(channel, direct);
        let broadcast_allowed = !muted
            && (setting == "everything"
                || !self
                    .channel_flag(channel, "suppress_at_channel")
                    .unwrap_or(false));
        let broadcast = broadcast_allowed && has_broadcast(workspace, message, thread);
        let keyword = !thread
            && !muted
            && message.subtype.as_deref() != Some("bot_message")
            && self
                .global_text("global_keywords")
                .is_some_and(|words| matches_keywords(message, words));
        let mentions = personal || broadcast || keyword;
        if muted && (direct || !has_personal_mention(workspace, message, false)) {
            return None;
        }
        let follows_all = self
            .channel_flag(channel, "follow_all_threads")
            .unwrap_or_else(|| self.global_flag("threads_everything").unwrap_or(false));
        let server_thread = message.kind.as_deref() == Some("desktop_notification");
        if thread && !personal && !(server_thread && follows_all) {
            return None;
        }
        if !direct
            && setting != "everything"
            && !mentions
            && !(thread && server_thread && follows_all)
        {
            return None;
        }
        let vip = self.vip_message(workspace, message, direct);
        if workspace.self_dnd.is_snoozed(now)
            && !flag(message, "_ignore_dnd")
            && !(vip && self.user_flag("priority_dnd_override"))
        {
            return None;
        }
        Some(self.options(vip, false))
    }

    pub fn huddle_options(
        &self,
        workspace: &Workspace,
        channel: &str,
        now: i64,
    ) -> Option<NotificationOptions> {
        if !self.desktop_enabled(channel)
            || self.muted(channel, now)
            || workspace.self_dnd.is_snoozed(now)
        {
            return None;
        }
        Some(self.options(false, true))
    }

    fn vip_message(&self, workspace: &Workspace, message: &Message, direct: bool) -> bool {
        if message.subtype.as_deref() == Some("bot_message")
            && message
                .bot_profile
                .as_ref()
                .and_then(|b| b.user_id.as_ref())
                .is_some_and(|u| workspace.vip_users.contains(u))
        {
            return true;
        }
        message
            .user
            .as_ref()
            .is_some_and(|u| workspace.vip_users.contains(u))
            && (direct || has_any_mention(message))
    }

    fn options(&self, vip: bool, huddle: bool) -> NotificationOptions {
        let normal = self.global_text("desktop_sound").unwrap_or("b2.mp3");
        let priority = self.global_text("priority_desktop_sound").unwrap_or(normal);
        let sound = if huddle {
            self.user
                .get("huddle_invite_sound_v2")
                .and_then(Value::as_str)
                .unwrap_or("boop_remix.mp3")
        } else if vip && priority != "none" {
            priority
        } else {
            normal
        };
        let muted = if huddle {
            self.user_flag("mute_huddle_sounds")
        } else {
            self.user_flag("mute_sounds")
        };
        NotificationOptions {
            preview: !self
                .global_flag("no_text_in_notifications")
                .unwrap_or(false),
            sound: (!muted && sound != "none" && !sound.is_empty()).then(|| sound.into()),
        }
    }

    fn global_text(&self, key: &str) -> Option<&str> {
        self.global.get(key)?.as_str()
    }
    fn global_flag(&self, key: &str) -> Option<bool> {
        self.global.get(key)?.as_bool()
    }
    fn channel_flag(&self, channel: &str, key: &str) -> Option<bool> {
        self.channels.get(channel)?.get(key)?.as_bool()
    }
    fn user_flag(&self, key: &str) -> bool {
        self.user.get(key).and_then(Value::as_bool).unwrap_or(false)
    }
}

fn flag(message: &Message, name: &str) -> bool {
    message
        .extra
        .get(name)
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn has_personal_mention(workspace: &Workspace, message: &Message, groups: bool) -> bool {
    text_parts(message).iter().any(|text| {
        text.split("<@").skip(1).any(|tail| {
            tail.split_once('>').is_some_and(|(token, _)| {
                token.split('|').next() == Some(workspace.self_user_id.as_str())
            })
        })
    }) || values(message).any(|v| value_has(v, "user_id", &workspace.self_user_id))
        || groups
            && workspace
                .usergroups
                .values()
                .any(|g| g.includes(&workspace.self_user_id) && g.mentioned_in(message))
}

fn has_broadcast(workspace: &Workspace, message: &Message, thread: bool) -> bool {
    ["here", "channel", "everyone"].iter().any(|name| {
        if *name == "here"
            && workspace.presence.get(&workspace.self_user_id) == Some(&Presence::Away)
            || thread && *name != "here"
        {
            return false;
        }
        text_parts(message).iter().any(|text| {
            text.contains(&format!("<!{name}>")) || text.contains(&format!("<!{name}|"))
        }) || values(message).any(|v| value_has(v, "range", name))
    })
}

fn has_any_mention(message: &Message) -> bool {
    text_parts(message)
        .iter()
        .any(|t| t.contains("<@") || t.contains("<!"))
        || values(message)
            .any(|v| value_has_type(v, &["user", "usergroup", "broadcast", "channel"]))
}

fn values(message: &Message) -> impl Iterator<Item = &Value> {
    message
        .blocks
        .iter()
        .chain(message.attachments.iter().flat_map(|a| a.blocks.iter()))
}

fn value_has(value: &Value, key: &str, needle: &str) -> bool {
    match value {
        Value::Object(map) => {
            map.get(key).and_then(Value::as_str) == Some(needle)
                || map.values().any(|v| value_has(v, key, needle))
        }
        Value::Array(items) => items.iter().any(|v| value_has(v, key, needle)),
        _ => false,
    }
}

fn value_has_type(value: &Value, types: &[&str]) -> bool {
    match value {
        Value::Object(map) => {
            map.get("type")
                .and_then(Value::as_str)
                .is_some_and(|t| types.contains(&t))
                || map.values().any(|v| value_has_type(v, types))
        }
        Value::Array(items) => items.iter().any(|v| value_has_type(v, types)),
        _ => false,
    }
}

fn text_parts(message: &Message) -> Vec<String> {
    let mut out = Vec::new();
    if message.blocks.is_empty() {
        out.extend(message.text.clone());
    } else {
        for block in &message.blocks {
            let mut text = String::new();
            collect_text(block, &mut text);
            out.push(text);
        }
    }
    for attachment in &message.attachments {
        if attachment.from_url.is_some() {
            continue;
        }
        out.extend(attachment.text.clone());
        out.extend(attachment.pretext.clone());
        out.extend(attachment.title.clone());
        out.extend(attachment.footer.clone());
        out.extend(
            attachment
                .fields
                .iter()
                .filter_map(|field| field.value.clone()),
        );
        for block in &attachment.blocks {
            let mut text = String::new();
            collect_text(block, &mut text);
            out.push(text);
        }
    }
    out
}

fn collect_text(value: &Value, out: &mut String) {
    match value {
        Value::Object(map) => {
            if map.get("type").and_then(Value::as_str).is_some_and(|t| {
                matches!(t, "user" | "usergroup" | "broadcast" | "emoji" | "channel")
            }) {
                out.push(' ');
                return;
            }
            if let Some(text) = map.get("text").and_then(Value::as_str) {
                out.push_str(text);
            }
            for value in map.values().filter(|v| v.is_array() || v.is_object()) {
                collect_text(value, out);
            }
            if map.get("type").and_then(Value::as_str) == Some("rich_text_section") {
                out.push('\n');
            }
        }
        Value::Array(items) => {
            for value in items {
                collect_text(value, out);
            }
        }
        _ => {}
    }
}

fn normalize(text: &str) -> String {
    text.to_lowercase()
        .replace(['\u{2018}', '\u{2019}'], "'")
        .replace(['\u{201c}', '\u{201d}'], "\"")
}

fn matches_keywords(message: &Message, words: &str) -> bool {
    text_parts(message).iter().any(|text| {
        let text = normalize(text);
        words
            .split(',')
            .map(str::trim)
            .filter(|w| !w.is_empty())
            .any(|word| {
                let word = normalize(word);
                text.match_indices(&word).any(|(i, _)| {
                    let prefix = &text[..i];
                    let suffix = &text[i + word.len()..];
                    let before = prefix.chars().next_back();
                    let after = suffix.chars().next();
                    let in_entity = prefix
                        .rfind('<')
                        .is_some_and(|open| prefix.rfind('>').is_none_or(|close| open > close))
                        && suffix.contains('>');
                    let in_mention = prefix
                        .split_whitespace()
                        .next_back()
                        .is_some_and(|tail| tail.contains('@'));
                    !in_entity
                        && !in_mention
                        && !before.is_some_and(char::is_alphanumeric)
                        && !after.is_some_and(char::is_alphanumeric)
                })
            })
    })
}

#[cfg(test)]
mod tests;
