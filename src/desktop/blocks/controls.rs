//! Read-only labels for Block Kit controls whose action protocol is unsupported.
//!
//! Slack supplies initial selections in the element itself. Do not replace that
//! content with a placeholder, or treat option values/URLs as link buttons.

pub(crate) mod fixture;

use serde_json::Value;

use super::{BlockCtx, text_object_plain};

pub(super) fn label(ctx: BlockCtx<'_>, kind: &str, value: &Value) -> String {
    initial_label(ctx, kind, value)
        .or_else(|| composition_label(value.get("placeholder")))
        .or_else(|| composition_label(value.get("text")))
        .unwrap_or_else(|| match kind {
            "overflow" => "More".into(),
            "datepicker" => "Select a date".into(),
            "timepicker" => "Select a time".into(),
            "datetimepicker" => "Select a date and time".into(),
            "plain_text_input" => "Text input".into(),
            _ => "Select".into(),
        })
}

fn initial_label(ctx: BlockCtx<'_>, kind: &str, value: &Value) -> Option<String> {
    match kind {
        "static_select" | "external_select" | "radio_buttons" => {
            option_label(value.get("initial_option")?)
        }
        "multi_static_select" | "multi_external_select" | "checkboxes" => {
            joined(value.get("initial_options")?, option_label)
        }
        "users_select" => Some(user_label(ctx, nonempty(value.get("initial_user")?)?)),
        "multi_users_select" => joined(value.get("initial_users")?, |user| {
            Some(user_label(ctx, nonempty(user)?))
        }),
        "channels_select" => Some(conversation_label(
            ctx,
            nonempty(value.get("initial_channel")?)?,
        )),
        "multi_channels_select" => joined(value.get("initial_channels")?, |channel| {
            Some(conversation_label(ctx, nonempty(channel)?))
        }),
        // BlockCtx has no owning message/modal conversation. Never substitute
        // workspace.last_active_channel: it may belong to another surface.
        "conversations_select" => Some(conversation_label(
            ctx,
            nonempty(value.get("initial_conversation")?)?,
        )),
        // Unlike the single select, Slack documents that the current-conversation
        // default overrides initial_conversations for a multi-select.
        "multi_conversations_select" if !defaults_to_current(value) => {
            joined(value.get("initial_conversations")?, |channel| {
                Some(conversation_label(ctx, nonempty(channel)?))
            })
        }
        "datepicker" => nonempty(value.get("initial_date")?).map(str::to_owned),
        "timepicker" => {
            let time = nonempty(value.get("initial_time")?)?;
            Some(match value.get("timezone").and_then(nonempty) {
                Some(timezone) => format!("{time} ({timezone})"),
                None => time.to_owned(),
            })
        }
        "datetimepicker" => {
            let seconds = value.get("initial_date_time")?.as_u64()?;
            i64::try_from(seconds).ok()?;
            let ts = seconds.to_string();
            let date = super_platinum_core::state::date_key_for_ts(&ts)?;
            let time = super_platinum_core::state::format_ts_hm(&ts);
            Some(format!("{date} {time}"))
        }
        "plain_text_input" => nonempty(value.get("initial_value")?).map(str::to_owned),
        _ => None,
    }
}

fn defaults_to_current(value: &Value) -> bool {
    value
        .get("default_to_current_conversation")
        .and_then(Value::as_bool)
        == Some(true)
}

fn nonempty(value: &Value) -> Option<&str> {
    value.as_str().filter(|text| !text.trim().is_empty())
}

fn option_label(value: &Value) -> Option<String> {
    composition_label(value.get("text"))
}

fn composition_label(value: Option<&Value>) -> Option<String> {
    // Slack decodes only these three entities. Ampersand comes last so nested
    // escapes decode exactly once; output stays plain text, never mrkdwn/HTML.
    text_object_plain(value)
        .map(|text| {
            text.replace("&lt;", "<")
                .replace("&gt;", ">")
                .replace("&amp;", "&")
        })
        .filter(|text| !text.trim().is_empty())
}

fn joined(value: &Value, label: impl Fn(&Value) -> Option<String>) -> Option<String> {
    let labels: Vec<_> = value.as_array()?.iter().filter_map(label).collect();
    (!labels.is_empty()).then(|| labels.join(", "))
}

fn user_label(ctx: BlockCtx<'_>, id: &str) -> String {
    format!("@{}", ctx.workspace.display_name(id))
}

fn conversation_label(ctx: BlockCtx<'_>, id: &str) -> String {
    let Some(channel) = ctx.workspace.channels.get(id) else {
        return id.to_owned();
    };
    let name = super_platinum_core::state::channel_display_name(ctx.workspace, channel);
    if channel.is_im || channel.is_mpim {
        name
    } else {
        format!("#{name}")
    }
}

#[cfg(test)]
mod tests;
