//! Message multiselects use an explicit, message-owned draft dialog.
use dioxus::prelude::*;
use serde_json::Value;
use super_platinum_core::block_actions::{BlockElementMetadata, BlockTextFormat};

use super::{button_confirmation::plain_label, message_radio::composition};
use crate::{
    icons::{Icon, icon},
    message_buttons::{inputs::multi, source::Source},
    state::ShellState,
};

#[component]
pub(crate) fn MessageMultiSelect(
    state: Signal<ShellState>,
    source: Source,
    metadata: BlockElementMetadata,
    accessibility_label: String,
) -> Element {
    let mounted = dioxus::core::use_hook_with_cleanup(
        crate::message_buttons::inputs::display::Mounted::new,
        crate::message_buttons::inputs::display::Mounted::unmount,
    );
    let hydration_revision = state.read().message_buttons.hydration_revision;
    use_effect(use_reactive(
        (&source, &hydration_revision),
        move |(source, _)| {
            crate::message_buttons::inputs::ensure(state, source.clone());
            crate::message_buttons::hydration::ensure(state, source);
        },
    ));
    let presentation = multi::presentation(&state.read(), &source, &metadata);
    let selected = multi::display_selected(&state.read(), &source, &metadata, &mounted);
    let label = if selected.is_empty() {
        presentation.placeholder.clone()
    } else {
        format!("{} selected", selected.len())
    };
    let title = presentation.reason.as_deref().unwrap_or(&label);
    let aria_label = if accessibility_label.is_empty() {
        presentation.placeholder.clone()
    } else {
        accessibility_label
    };
    let hover_source = source.clone();
    rsx! {button {
        class:"block-select block-multi-select",r#type:"button",disabled:!presentation.enabled,
        title:"{title}","aria-label":"{aria_label}","aria-haspopup":"dialog",
        "aria-busy":presentation.busy.to_string(),
        "data-block-id":metadata.block_id.as_deref().unwrap_or_default(),
        "data-action-id":metadata.action_id.as_deref().unwrap_or_default(),
        "data-input-revision":presentation.revision.to_string(),
        onmouseenter:move |_|crate::message_buttons::hydration::retry(state,hover_source.clone()),
        onclick:move |event|{event.stop_propagation();multi::open(state,source.clone(),metadata.clone(),mounted.clone());},
        ondoubleclick:move |event|event.stop_propagation(),
        span {"{label}"}
        {icon(Icon::ChevronRight,"icon block-multi-chevron")}
    }}
}

pub(crate) fn dialog(state: Signal<ShellState>) -> Element {
    let Some(revision) = multi::picker(&state.read()).map(|p| p.revision) else {
        return rsx! {};
    };
    rsx! {MultiDialog {key:"message-multi-{revision}",state}}
}

#[component]
fn MultiDialog(state: Signal<ShellState>) -> Element {
    let mut query = use_signal(String::new);
    let Some(picker) = multi::picker(&state.read()) else {
        return rsx! {};
    };
    let revision = picker.revision;
    let confirmation = picker.confirmation.as_ref();
    let title = confirmation
        .and_then(|c| c.title.as_ref())
        .map(plain_label)
        .unwrap_or_else(|| picker.placeholder.clone());
    let apply = confirmation
        .and_then(|c| c.confirm.as_ref())
        .map(plain_label)
        .unwrap_or_else(|| "Apply".into());
    let cancel = confirmation
        .and_then(|c| c.deny.as_ref())
        .map(plain_label)
        .unwrap_or_else(|| "Cancel".into());
    let body = confirmation.and_then(|c|c.text.as_ref()).map(|text|serde_json::json!({"type":if text.format==BlockTextFormat::Mrkdwn {"mrkdwn"} else {"plain_text"},"text":text.text,"emoji":text.emoji}));
    let submit_class = if confirmation.and_then(|c| c.style.as_deref()) == Some("danger") {
        "button-confirmation-submit danger"
    } else {
        "button-confirmation-submit"
    };
    let filter = query.read().to_lowercase();
    let visible = picker
        .options
        .iter()
        .enumerate()
        .filter(|(_, o)| filter.is_empty() || o.text.to_lowercase().contains(&filter))
        .collect::<Vec<_>>();
    rsx! {
        div {class:"scrim",onclick:move |_|multi::cancel(state,revision)}
        section {id:"shell-overlay-dialog",class:"modal button-confirmation message-multi-dialog",role:"dialog",
            "aria-modal":"true","aria-labelledby":"message-multi-title",tabindex:"-1",
            "data-keyboard-dialog":"true","data-dialog-kind":"other","data-dialog-generation":revision,
            onkeydown:move |event|if event.key()==Key::Escape {event.prevent_default();event.stop_propagation();multi::cancel(state,revision);},
            onmounted:move |_|{dioxus::document::eval(&format!("(()=>{{const dialog=document.getElementById('shell-overlay-dialog');if(dialog?.dataset.dialogGeneration!=='{revision}')return;const before=document.activeElement;requestAnimationFrame(()=>{{if(dialog.isConnected&&dialog.dataset.dialogGeneration==='{revision}'&&document.activeElement===before)dialog.querySelector('#message-multi-search')?.focus({{preventScroll:true}});}});}})();"));},
            header {
                h2 {id:"message-multi-title","{title}"}
                button {r#type:"button",class:"icon-btn","aria-label":"Close selection",onclick:move |_|multi::cancel(state,revision),{icon(Icon::Close,"icon")}}
            }
            form {id:"message-multi-form",onsubmit:move |event|{event.prevent_default();multi::apply(state,revision);},
                div {class:"message-multi-content",
                    if let Some(body)=body.as_ref() {div {class:"message-multi-body",{composition(state,Some(body),"")}}}
                    if !picker.selected_options.is_empty() {
                        div {class:"message-multi-selected",role:"group","aria-label":"Selected options",
                            for (index,option) in picker.selected_options.iter().enumerate() {
                                {let value=option.get("value").and_then(Value::as_str).unwrap_or_default().to_owned();
                                let label=option.get("text").and_then(|t|t.get("text")).and_then(Value::as_str).unwrap_or("option");
                                rsx! {div {key:"selected-{index}",class:"message-multi-selected-option",
                                    span {{composition(state,option.get("text"),label)}}
                                    button {r#type:"button",class:"icon-btn","aria-label":"Remove {label}","data-option-value":"{value}",
                                        onclick:move |_|multi::toggle(state,revision,value.clone()),{icon(Icon::Close,"icon")}}
                                }}}
                            }
                        }
                    }
                    input {id:"message-multi-search",r#type:"search",initial_value:"",placeholder:"Find an option",autocomplete:"off","aria-label":"Find an option",oninput:move |event|query.set(event.value())}
                    div {class:"message-multi-options",role:"group","aria-label":"Options",
                        if visible.is_empty() {p {class:"muted","No matching options"}}
                        for (position,(index,option)) in visible.iter().enumerate() {
                            {let value=option.value.clone();let id=format!("message-multi-option-{index}");
                            let checked=vec![Attribute::new("checked",picker.selected.contains(&option.value),None,true)];
                            let group=option.group.as_deref().filter(|_|position==0 || visible[position-1].1.group!=option.group);
                            rsx! {div {key:"option-{index}",class:"message-multi-entry",
                                if let Some(group)=group {div {class:"message-multi-group","{group}"}}
                                label {class:"block-radio-option",r#for:"{id}",
                                    input {id:"{id}",r#type:"checkbox",value:"option-{index}","data-option-value":"{value}",
                                        onchange:move |event|{event.stop_propagation();multi::toggle(state,revision,value.clone());},..checked}
                                    span {class:"block-radio-copy",
                                        span {class:"block-radio-label",{composition(state,option.raw.get("text"),&option.text)}}
                                        if let Some(description)=option.raw.get("description") {span {class:"block-radio-description",{composition(state,Some(description),"")}}}
                                    }
                                }
                            }}}
                        }
                    }
                }
                div {class:"message-multi-status",role:if picker.reason.is_some(){"alert"}else{"status"},
                    if let Some(reason)=picker.reason.as_ref() {"{reason}"} else if let Some(max)=picker.max_selected {"{picker.selected.len()} of {max} selected"} else {"{picker.selected.len()} selected"}
                }
                footer {
                    button {id:"message-multi-cancel",r#type:"button",onclick:move |_|multi::cancel(state,revision),"{cancel}"}
                    button {id:"message-multi-apply",r#type:"submit",class:"{submit_class}",disabled:!picker.enabled,"{apply}"}
                }
            }
        }
    }
}
