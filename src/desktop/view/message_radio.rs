//! Native message radio groups. Confirmation and state commit belong to inputs.
use dioxus::prelude::*;
use serde_json::Value;
use std::sync::atomic::{AtomicU64, Ordering};
use super_platinum_core::block_actions::BlockElementMetadata;

use crate::{message_buttons::source::Source, model::RichNode, state::ShellState};

static NEXT_GROUP: AtomicU64 = AtomicU64::new(1);

#[component]
pub(crate) fn MessageRadioGroup(
    state: Signal<ShellState>,
    source: Source,
    metadata: BlockElementMetadata,
    accessibility_label: String,
) -> Element {
    let mounted = dioxus::core::use_hook_with_cleanup(
        crate::message_buttons::inputs::display::Mounted::new,
        crate::message_buttons::inputs::display::Mounted::unmount,
    );
    // DOM-only identity keeps two rendered copies of a message from unchecking
    // each other's native radios. It never enters an app action or source ID.
    let group = use_hook(|| {
        format!(
            "message-radio-{}",
            NEXT_GROUP.fetch_add(1, Ordering::Relaxed)
        )
    });
    let hydration_revision = state.read().message_buttons.hydration_revision;
    use_effect(use_reactive(
        (&source, &hydration_revision),
        move |(source, _)| {
            crate::message_buttons::inputs::ensure(state, source.clone());
            crate::message_buttons::hydration::ensure(state, source);
        },
    ));
    let mut presentation =
        crate::message_buttons::inputs::radio_presentation(&state.read(), &source, &metadata);
    presentation.selected = crate::message_buttons::inputs::display_selected(
        &state.read(),
        &source,
        &metadata,
        &mounted,
    );
    let title = presentation
        .reason
        .as_deref()
        .unwrap_or(if presentation.busy {
            "Submitting…"
        } else {
            "Choose an option"
        });
    let label = if accessibility_label.trim().is_empty() {
        "Choose an option"
    } else {
        &accessibility_label
    };
    let hover_source = source.clone();
    rsx! {div {
        class:"block-radio-group",role:"radiogroup",title:"{title}",
        "aria-label":"{label}","aria-busy":presentation.busy.to_string(),"aria-disabled":(!presentation.enabled).to_string(),
        "data-block-id":metadata.block_id.as_deref().unwrap_or_default(),
        "data-action-id":metadata.action_id.as_deref().unwrap_or_default(),
        "data-input-revision":presentation.revision.to_string(),
        onmouseenter:move |_|crate::message_buttons::hydration::retry(state,hover_source.clone()),
        onclick:move |event|event.stop_propagation(),
        ondoubleclick:move |event|event.stop_propagation(),
        if presentation.options.is_empty() {span {class:"block-radio-unavailable","Choose an option"}}
        for (index,option) in presentation.options.iter().enumerate() {
            {let id=format!("{group}-option-{index}");let label_id=format!("{id}-label");let description_id=format!("{id}-description");
            let description=option.raw.get("description").filter(|value|value.get("text").and_then(Value::as_str).is_some());
            let described_by=if description.is_some(){description_id.clone()}else{String::new()};
            let value=option.value.clone();let source=source.clone();let metadata=metadata.clone();let mounted=mounted.clone();
            // Pinned input.checked is NOT volatile, unlike select.value. Force
            // exact state back into the existing node before confirm/cancel.
            let checked_attributes=vec![Attribute::new("checked",presentation.selected.as_deref()==Some(option.value.as_str()),None,true)];
            rsx! {label {key:"option-{index}",class:"block-radio-option",r#for:"{id}",
                input {id:"{id}",r#type:"radio",name:"{group}",value:"option-{index}",
                    disabled:!presentation.enabled,
                    "aria-labelledby":"{label_id}","aria-describedby":"{described_by}",
                    onchange:move |event|{
                        event.stop_propagation();
                        // Pinned native FormEvent reports a radio's value, not
                        // checkbox-style checked=true/false.
                        if event.value()==format!("option-{index}") {
                            crate::message_buttons::inputs::change_radio(state,source.clone(),metadata.clone(),Some(value.clone()),mounted.clone());
                        }
                    },
                    ..checked_attributes,
                }
                span {class:"block-radio-copy",
                    span {id:"{label_id}",class:"block-radio-label",{composition(state,option.raw.get("text"),&option.text)}}
                    if let Some(description)=description {
                        span {id:"{description_id}",class:"block-radio-description",{composition(state,Some(description),"")}}
                    }
                }
            }}}
        }
    }}
}

pub(crate) fn composition(
    state: Signal<ShellState>,
    value: Option<&Value>,
    fallback: &str,
) -> Element {
    let Some(raw) = value else {
        return rsx! {"{fallback}"};
    };
    let text = raw.get("text").and_then(Value::as_str).unwrap_or(fallback);
    let shell = state.read();
    let media_epoch = shell.media_epoch;
    let nodes = if raw.get("type").and_then(Value::as_str) == Some("mrkdwn") {
        shell
            .core
            .active_team
            .as_ref()
            .and_then(|team| shell.core.workspaces.get(team))
            .map(|workspace| {
                inert_nodes(crate::blocks::mrkdwn_inline(
                    crate::blocks::BlockCtx::new(workspace, &shell.media),
                    text,
                ))
            })
            .unwrap_or_else(|| vec![RichNode::Text(decode(text))])
    } else {
        let text = decode(text);
        vec![RichNode::Text(
            if raw.get("emoji").and_then(Value::as_bool) == Some(false) {
                text
            } else {
                super_platinum_core::state::emoji_text_to_display(&text)
            },
        )]
    };
    drop(shell);
    rsx! {for node in &nodes {{super::rich::rich_node(node,media_epoch,state)}}}
}

// Captured gMsa renders label/description compositions with noLinking=true.
// Keep formatting, but never put clickable mentions or anchors inside a label.
fn inert_nodes(nodes: Vec<RichNode>) -> Vec<RichNode> {
    nodes
        .into_iter()
        .filter_map(|node| {
            Some(match node {
                RichNode::Text(text) => RichNode::Text(decode(&text)),
                RichNode::StyledText {
                    text,
                    bold,
                    italic,
                    strike,
                    code,
                } => RichNode::StyledText {
                    text: decode(&text),
                    bold,
                    italic,
                    strike,
                    code,
                },
                RichNode::Link { label, .. }
                | RichNode::UserMention { label, .. }
                | RichNode::ChannelMention { label, .. }
                | RichNode::GroupMention { label, .. } => RichNode::Text(decode(&label)),
                node @ (RichNode::Emoji { .. } | RichNode::EmojiImage { .. }) => node,
                _ => return None,
            })
        })
        .collect()
}
fn decode(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn radio_compositions_keep_styles_but_never_link_or_activate_mentions() {
        let nodes = inert_nodes(vec![
            RichNode::StyledText {
                text: "R&amp;D".into(),
                bold: true,
                italic: false,
                strike: false,
                code: false,
            },
            RichNode::Link {
                label: "Review".into(),
                url: "https://example.test".into(),
            },
            RichNode::UserMention {
                user_id: "U1".into(),
                label: "@Reviewer".into(),
            },
            RichNode::Text("&lt;https://example.test|literal&gt; &amp;lt;tag&amp;gt;".into()),
        ]);
        assert!(matches!(&nodes[0],RichNode::StyledText{text,bold:true,..} if text=="R&D"));
        assert!(matches!(&nodes[1],RichNode::Text(text) if text=="Review"));
        assert!(matches!(&nodes[2],RichNode::Text(text) if text=="@Reviewer"));
        assert!(
            matches!(&nodes[3],RichNode::Text(text) if text=="<https://example.test|literal> &lt;tag&gt;")
        );
    }
}
