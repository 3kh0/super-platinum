//! Passive current raw-topic reader. No fetch, action, resource or ready mint.
use crate::app_home::channel_fetches::canonical;
use crate::{
    blocks::{TopicRun, passive_topic},
    state::ShellState,
};
use dioxus::prelude::*;
use serde_json::{Value, json};

// Defensive native parse/render bound, not a Source topic-length claim.
const MAX_BYTES: usize = 4096;
mod private;
pub(crate) use private::fixture as private_fixture;
const FIXTURE_TOPIC: &str =
    "*Release planning* · _Read only_ &amp; quiet updates · `notes` · <@U_UNKNOWN> :custom_emoji:";
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Provenance {
    Owned,
    Layout,
}
struct Topic {
    raw: String,
    runs: Vec<TopicRun>,
    provenance: Provenance,
}
fn text(value: &Value) -> Option<&str> {
    value
        .as_object()?
        .get("value")?
        .as_str()
        .filter(|s| !s.is_empty() && s.len() <= MAX_BYTES)
}
fn qualified(row: &Value, id: &str) -> bool {
    if row["id"].as_str() != Some(id) {
        return false;
    }
    for (key, value) in [
        ("is_channel", true),
        ("is_member", true),
        ("is_private", false),
        ("is_group", false),
        ("is_im", false),
        ("is_mpim", false),
        ("is_archived", false),
    ] {
        if row.get(key) != Some(&Value::Bool(value)) {
            return false;
        }
    }
    for key in [
        "is_file",
        "isUnknown",
        "isNonExistent",
        "is_deleted",
        "deleted",
    ] {
        if row
            .get(key)
            .is_some_and(|v| !matches!(v, Value::Null | Value::Bool(false)))
        {
            return false;
        }
    }
    let properties = row.get("properties").filter(|v| !v.is_null());
    if properties.is_some_and(|v| !v.is_object())
        || properties
            .and_then(|v| v.get("record_channel"))
            .is_some_and(|v| !v.is_null())
    {
        return false;
    }
    canonical::ordinary_label(&json!({"type":"channel","channel_id":id}), row).is_ok()
}
fn project(s: &ShellState) -> Option<Topic> {
    if std::env::var_os("SUPER_PLATINUM_FIXTURE").is_some()
        || !s.signed_in
        || s.loading
        || s.core.screen != super_platinum_core::state::Screen::Main
    {
        return None;
    }
    let team = s.core.active_team.as_deref()?;
    let id = s.core.active_channel.as_deref()?;
    let vm = s.channels.get(s.active_channel)?;
    if vm.id != id || vm.is_im || vm.is_mpim || vm.is_private {
        return None;
    }
    let model = s.core.workspaces.get(team)?.channels.get(id)?;
    let source = canonical::observed(s, team).ok()?;
    if !source.current_for(s, team) {
        return None;
    }
    let raw = source.row(id)?;
    if !qualified(raw, id)
        || !model.is_channel
        || model.is_private
        || model.is_group
        || model.is_im
        || model.is_mpim
        || model.is_archived
    {
        return None;
    }
    let topic = raw.get("topic")?;
    if model.extra.get("topic") != Some(topic) {
        return None;
    }
    let value = text(topic)?;
    let runs = passive_topic(value);
    if !source.current_for(s, team) {
        return None;
    }
    Some(Topic {
        raw: value.to_owned(),
        runs,
        provenance: Provenance::Owned,
    })
}
fn fixture_model(s: &ShellState) -> Option<Topic> {
    if s.core.session.is_some() || s.core.transport.is_some() || s.core.active_account.is_some() {
        return None;
    }
    let team = s.core.active_team.as_deref()?;
    let id = s.core.active_channel.as_deref()?;
    if s.channels.get(s.active_channel)?.id != id {
        return None;
    }
    let value = text(
        s.core
            .workspaces
            .get(team)?
            .channels
            .get(id)?
            .extra
            .get("topic")?,
    )?;
    if value != FIXTURE_TOPIC {
        return None;
    }
    Some(Topic {
        raw: value.to_owned(),
        runs: passive_topic(value),
        provenance: Provenance::Layout,
    })
}
fn render_topic(topic: Topic) -> Element {
    let plain = topic
        .runs
        .iter()
        .map(|r| r.text.as_str())
        .collect::<String>();
    let provenance = if topic.provenance == Provenance::Owned {
        "owned"
    } else {
        "layout"
    };
    rsx! {span{id:"conversation-header-topic",class:"conversation-header-topic",role:"note","aria-label":format!("Channel topic: {plain}"),title:plain,
        "data-topic-provenance":provenance,"data-topic-bytes":topic.raw.len().to_string(),
        for(index,run)in topic.runs.into_iter().enumerate(){
            span{key:"{index}",class:format!("topic-run{}{}{}{}",if run.bold{" topic-bold"}else{""},if run.italic{" topic-italic"}else{""},if run.strike{" topic-strike"}else{""},if run.code{" topic-code"}else{""}),"{run.text}"}
        }
    }}
}
pub(super) fn render(s: &ShellState) -> Element {
    let topic = match std::env::var("SUPER_PLATINUM_FIXTURE").ok().as_deref() {
        Some("conversation-header-private-topic" | "conversation-header-private-topic-narrow") => {
            private::fixture_model(s)
        }
        Some("conversation-header-topic" | "conversation-header-topic-narrow") => fixture_model(s),
        Some(_) => None,
        None => project(s).or_else(|| private::project(s)),
    };
    topic.map(render_topic).unwrap_or_else(|| rsx! {})
}
/// Disconnected layout data only. It cannot construct the live raw witness.
pub(crate) fn fixture(s: &mut ShellState) {
    s.core.session = None;
    s.core.transport = None;
    s.core.active_account = None;
    s.overlay = None;
    if let (Some(team), Some(id)) = (s.core.active_team.clone(), s.core.active_channel.clone()) {
        if let Some(channel) = s
            .core
            .workspaces
            .get_mut(&team)
            .and_then(|w| w.channels.get_mut(&id))
        {
            channel
                .extra
                .insert("topic".into(), json!({"value":FIXTURE_TOPIC}));
            channel.extra.insert(
                "purpose".into(),
                json!({"value":"Purpose must not replace the raw topic"}),
            );
        }
    }
    s.refresh_from_core();
}
#[cfg(test)]
mod tests;
