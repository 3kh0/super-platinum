//! Typed StepText projection. Channel actions require a separate occurrence guard.
use super::{members, presentation};
use crate::{icons::Icon, state::ShellState, view::media_image::MediaImage};
use dioxus::prelude::*;
use serde_json::Value;
use super_platinum_core::MediaAssetId;
pub(super) mod files;
pub(super) mod navigation;
#[cfg(test)]
mod tests;
mod tokens;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Inline {
    Text(String),
    User {
        id: String,
        label: String,
        deactivated: bool,
        part: usize,
    },
    Channel {
        id: String,
        label: String,
        icon: Option<Icon>,
        part: usize,
    },
    File {
        team: String,
        id: String,
        kind: files::Kind,
        label: Option<files::Label>,
        part: usize,
    },
    Emoji {
        text: String,
        title: Option<String>,
        image: Option<MediaAssetId>,
    },
}

/// AjOk's explicit 5kVY value is one emoji, while HOzI scans only the trailing
/// text. Neither path parses HTML, mrkdwn, links or Slack angle entities.
pub(super) fn project(s: &ShellState, team: &str, visit: u64, raw: &Value) -> Vec<Inline> {
    let mut nodes = Vec::new();
    for (ordinal, part) in presentation::parts(raw).into_iter().enumerate() {
        match part {
            presentation::Part::Text(text) => nodes.push(Inline::Text(text)),
            presentation::Part::Emoji(text) => nodes.push(single(s, team, &text, false)),
            presentation::Part::EmojiText(text) => {
                for (emoji, text) in tokens::split(&text) {
                    nodes.push(if emoji {
                        single(s, team, text, true)
                    } else {
                        Inline::Text(text.into())
                    });
                }
            }
            presentation::Part::User(id) => {
                let label = user_label(s, team, &id, members::names::observed_policy(s, visit));
                // wZ+Y uses a8B4.o: only the member's deleted flag. This fact
                // is independent of preference availability and label text.
                let deactivated = s
                    .core
                    .workspaces
                    .get(team)
                    .and_then(|w| w.users.get(&id))
                    .is_some_and(|u| u.deleted);
                nodes.push(Inline::User {
                    id,
                    label,
                    deactivated,
                    part: ordinal,
                });
            }
            presentation::Part::Channel(id) => {
                let channel = s
                    .core
                    .workspaces
                    .get(team)
                    .and_then(|w| w.channels.get(&id));
                let name = channel
                    .and_then(|c| c.name.as_deref())
                    .filter(|name| !name.is_empty());
                let (label, icon) = super::channel_labels::presentation(s, visit, &id)
                    .unwrap_or_else(|| {
                        let icon = channel.and_then(local_channel_icon);
                        let label = match (name, icon) {
                            (Some(name), Some(Icon::Archive | Icon::ArchiveFilled)) => {
                                format!("{name} (archived)")
                            }
                            (Some(name), Some(_)) => name.to_owned(),
                            (Some(name), None) => format!("#{name}"),
                            _ => "Channel unavailable".into(),
                        };
                        (label, icon)
                    });
                nodes.push(Inline::Channel {
                    id,
                    label,
                    icon,
                    part: ordinal,
                });
            }
            presentation::Part::Canvas(id) => {
                let label = files::project(s, team, &id, files::Kind::Canvas);
                nodes.push(Inline::File {
                    team: team.into(),
                    id,
                    kind: files::Kind::Canvas,
                    label,
                    part: ordinal,
                });
            }
            presentation::Part::List(id) => {
                let label = files::project(s, team, &id, files::Kind::List);
                nodes.push(Inline::File {
                    team: team.into(),
                    id,
                    kind: files::Kind::List,
                    label,
                    part: ordinal,
                });
            }
        }
    }
    nodes
}

fn user_label(
    s: &ShellState,
    team: &str,
    id: &str,
    policy: Option<&super_platinum_core::block_actions::name_policy::Snapshot>,
) -> String {
    let unavailable = || "Person unavailable".to_owned();
    let Some(policy) = policy else {
        return unavailable();
    };
    let Some(member) = s.core.workspaces.get(team).and_then(|w| w.users.get(id)) else {
        return unavailable();
    };
    // hWCs uses the profile's real name before the top-level real name, and
    // never substitutes the username or normalized-name fields. Read only the
    // name facts needed by the existing observed-policy decoder.
    let raw = serde_json::json!({
        "id": member.id,
        "real_name": member.real_name,
        "profile": {
            "real_name": member.profile.as_ref().and_then(|p| p.real_name.as_deref()),
            "display_name": member.profile.as_ref().and_then(|p| p.display_name.as_deref()),
        },
    });
    policy
        .member_name(&raw, "Name hidden")
        .unwrap_or_else(|_| unavailable())
}

/// EXeY/OK9l disable the text prefix; BWO1 fills private icons and J+l2
/// gives archived precedence over private. Frozen grace, shared and special
/// channel policies remain outside this local metadata projection.
fn local_channel_icon(c: &super_platinum_core::slack::models::Channel) -> Option<Icon> {
    if !(c.is_channel || c.is_private || c.is_group)
        || c.is_im
        || c.is_mpim
        || c.is_ext_shared
        || !c.connected_teams.is_empty()
        || c.name
            .as_deref()
            .is_none_or(|n| n.is_empty() || n.starts_with("mc:"))
    {
        return None;
    }
    for key in [
        "is_file",
        "isUnknown",
        "isNonExistent",
        "is_shared",
        "is_org_shared",
        "is_frozen",
    ] {
        if c.extra
            .get(key)
            .is_some_and(|v| !matches!(v, Value::Null | Value::Bool(false)))
        {
            return None;
        }
    }
    let properties = c.extra.get("properties").filter(|v| !v.is_null());
    if properties.is_some_and(|v| !v.is_object()) {
        return None;
    }
    for (group, field) in [
        ("huddles", "is_huddle_channel"),
        ("meetings_channel", "is_meeting_channel"),
        ("record_channel", "record_id"),
    ] {
        let group = properties
            .and_then(|p| p.get(group))
            .filter(|v| !v.is_null());
        if group.is_some_and(|v| !v.is_object())
            || group.and_then(|v| v.get(field)).is_some_and(|v| {
                !matches!(v, Value::Null | Value::Bool(false)) && v.as_str() != Some("")
            })
        {
            return None;
        }
    }
    Some(match (c.is_archived, c.is_private || c.is_group) {
        (true, true) => Icon::ArchiveFilled,
        (true, false) => Icon::Archive,
        (false, true) => Icon::LockFilled,
        (false, false) => Icon::Tag,
    })
}

fn single(s: &ShellState, team: &str, raw: &str, tooltip: bool) -> Inline {
    match crate::emoji_text::single(s, team, raw, tooltip) {
        crate::emoji_text::Part::Text(text) => Inline::Text(text),
        crate::emoji_text::Part::Emoji { text, title, image } => {
            Inline::Emoji { text, title, image }
        }
    }
}

pub(super) fn render(
    nodes: &[Inline],
    epoch: u64,
    state: Signal<ShellState>,
    visit: u64,
    location: navigation::Location,
) -> Element {
    rsx! {
        for node in nodes {
            {match node {
                Inline::Text(text) => rsx! {span {"{text}"}},
                Inline::User {id,label,deactivated,part} => rsx! {strong {
                    class:if *deactivated {"workflow-activity-user workflow-activity-user--deactivated"} else {"workflow-activity-user"},
                    "data-user":id.clone(), "data-part":*part,
                    "data-deactivated":if *deactivated {"true"} else {"false"},
                    span{class:"workflow-activity-user-name","{label}"}
                    if *deactivated {span{class:"workflow-activity-user-deactivated","(deactivated)"}}
                }},
                Inline::Channel {id,label,icon,part} => {
                    let source = navigation::Source::capture(&state.peek(),visit,location,*part,id);
                    let content = rsx! {span {class:"workflow-activity-channel-label",
                        if let Some(icon) = icon {
                            {crate::icons::icon(*icon,"icon sm workflow-activity-channel-icon")}
                        }
                        span {class:"workflow-activity-channel-name", "{label}"}
                    }};
                    match source {
                        Some(source) => rsx! {button {
                            id:source.button(), r#type:"button", class:"mention-chip channel-chip workflow-activity-channel", "data-channel":id.clone(),
                            onclick:move |_| navigation::open(state,source.clone()), {content}
                        }},
                        None => content,
                    }
                }
                Inline::File{team,id,kind,label,part} => files::render(state,visit,location,*part,team,id.clone(),*kind,label.clone(),epoch),
                Inline::Emoji {text, title, image} => {
                    let fallback = rsx! {span {class:"custom-emoji", title:title.clone(), "{text}"}};
                    match image.as_ref().filter(|id| state.read().media.is_ready(id)) {
                        Some(id) => rsx! {MediaImage {
                            state, id:id.clone(), media_epoch:epoch, compact:true,
                            class:"emoji-image", alt:text.clone(), title:title.clone().unwrap_or_default(),
                            fallback,
                        }},
                        None => fallback,
                    }
                }
            }}
        }
    }
}
