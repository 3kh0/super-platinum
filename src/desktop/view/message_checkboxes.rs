//! Ordinary message checkbox groups retain exact option objects until Go.
use dioxus::prelude::*;
use serde_json::Value;
use std::sync::atomic::{AtomicU64, Ordering};
use super_platinum_core::block_actions::BlockElementMetadata;

use super::message_radio::composition;
use crate::{
    message_buttons::{inputs, source::Source},
    state::ShellState,
};

static NEXT_GROUP: AtomicU64 = AtomicU64::new(1);

#[component]
pub(crate) fn MessageCheckboxGroup(
    state: Signal<ShellState>,
    source: Source,
    metadata: BlockElementMetadata,
    accessibility_label: String,
) -> Element {
    let mounted = dioxus::core::use_hook_with_cleanup(
        inputs::display::Mounted::new,
        inputs::display::Mounted::unmount,
    );
    let group = use_hook(|| {
        format!(
            "message-checkbox-{}",
            NEXT_GROUP.fetch_add(1, Ordering::Relaxed)
        )
    });
    let hydration_revision = state.read().message_buttons.hydration_revision;
    use_effect(use_reactive(
        (&source, &hydration_revision),
        move |(source, _)| {
            inputs::ensure(state, source.clone());
            crate::message_buttons::hydration::ensure(state, source);
        },
    ));
    let presentation = inputs::multi::presentation(&state.read(), &source, &metadata);
    let selected = inputs::multi::display_selected(&state.read(), &source, &metadata, &mounted);
    let title = presentation
        .reason
        .as_deref()
        .unwrap_or(if presentation.busy {
            "Submitting…"
        } else {
            "Choose options"
        });
    let label = if accessibility_label.trim().is_empty() {
        "Choose options"
    } else {
        &accessibility_label
    };
    let hover_source = source.clone();
    rsx! {div {
        class:"block-checkbox-group",role:"group",title:"{title}",
        "aria-label":"{label}","aria-busy":presentation.busy.to_string(),"aria-disabled":(!presentation.enabled).to_string(),
        "data-block-id":metadata.block_id.as_deref().unwrap_or_default(),
        "data-action-id":metadata.action_id.as_deref().unwrap_or_default(),
        "data-input-revision":presentation.revision.to_string(),
        onmouseenter:move |_|crate::message_buttons::hydration::retry(state,hover_source.clone()),
        onclick:move |event|event.stop_propagation(),
        ondoubleclick:move |event|event.stop_propagation(),
        if presentation.options.is_empty() {span {class:"block-checkbox-unavailable","Choose options"}}
        for (index,option) in presentation.options.iter().enumerate() {
            {let id=format!("{group}-option-{index}");let label_id=format!("{id}-label");let description_id=format!("{id}-description");
            let description=option.raw.get("description").filter(|value|value.get("text").and_then(Value::as_str).is_some());
            let described_by=if description.is_some(){description_id.clone()}else{String::new()};
            let raw=option.raw.clone();let value=option.value.clone();let source=source.clone();let metadata=metadata.clone();let mounted=mounted.clone();
            // Pinned checked is not volatile. Reapply the committed mounted
            // value while a confirmation is open and after cancellation.
            let checked=selected.iter().any(|raw|raw.get("value").and_then(Value::as_str)==Some(option.value.as_str()));
            let checked_attributes=vec![Attribute::new("checked",checked,None,true)];
            rsx! {label {key:"option-{index}",class:"block-checkbox-option block-radio-option",r#for:"{id}",
                input {id:"{id}",r#type:"checkbox",name:"{group}",value:"option-{index}",
                    "data-option-value":"{value}",disabled:!presentation.enabled,
                    "aria-labelledby":"{label_id}","aria-describedby":"{described_by}",
                    onchange:move |event|{
                        event.stop_propagation();
                        let selected=inputs::multi::display_selected(&state.peek(),&source,&metadata,&mounted);
                        let next=toggled(selected,&raw,event.checked());
                        inputs::multi::change(state,source.clone(),metadata.clone(),next,mounted.clone());
                    },
                    ..checked_attributes,
                }
                span {class:"block-checkbox-copy block-radio-copy",
                    span {id:"{label_id}",class:"block-checkbox-label block-radio-label",{composition(state,option.raw.get("text"),&option.text)}}
                    if let Some(description)=description {span {id:"{description_id}",class:"block-checkbox-description block-radio-description",{composition(state,Some(description),"")}}}
                }
            }}}
        }
    }}
}

/// Unselected additions use today's canonical option; every surviving sibling
/// keeps the exact raw object retained by this mounted control.
fn toggled(mut selected: Vec<Value>, option: &Value, checked: bool) -> Vec<Value> {
    let value = option.get("value").and_then(Value::as_str);
    if checked {
        if !selected
            .iter()
            .any(|raw| raw.get("value").and_then(Value::as_str) == value)
        {
            selected.push(option.clone());
        }
    } else {
        selected.retain(|raw| raw.get("value").and_then(Value::as_str) != value);
    }
    selected
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn checkbox_changes_preserve_raw_sibling_order_and_use_current_added_option() {
        let old = json!({"value":"a","text":{"type":"plain_text","text":"Old label"},"extension":{"keep":true}});
        let current = json!({"value":"b","text":{"type":"plain_text","text":"Current label"}});
        assert_eq!(
            toggled(vec![old.clone()], &current, true),
            vec![old.clone(), current.clone()]
        );
        assert_eq!(
            toggled(vec![old.clone(), current.clone()], &current, false),
            vec![old.clone()]
        );
        let renamed = json!({"value":"a","text":{"type":"plain_text","text":"New label"}});
        assert_eq!(
            toggled(vec![old.clone()], &renamed, true),
            vec![old.clone()]
        );
        assert!(toggled(vec![old], &renamed, false).is_empty());
    }
}
