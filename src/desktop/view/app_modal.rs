//! Native app forms. This renderer only reports local edits to the controller.

use dioxus::prelude::*;
use super_platinum_core::block_actions::form::{
    FormBlock, FormField, InputKey, InputKind, InputValue, OptionSpec, datetime_local_value,
    parse_datetime_local,
};

use crate::{
    icons::{Icon, icon},
    state::ShellState,
};

pub(crate) fn app_modal(state: Signal<ShellState>) -> Element {
    let Some(revision) = crate::app_modal::presentation(&state.read()).map(|p| p.reset_revision)
    else {
        return rsx! {};
    };
    rsx! {AppModalDialog {key:"app-modal-{revision}",state}}
}

#[component]
fn AppModalDialog(state: Signal<ShellState>) -> Element {
    let shell = state.read();
    let Some(presentation) = crate::app_modal::presentation(&shell) else {
        return rsx! {};
    };
    let locked = presentation.locked;
    let unsupported = presentation
        .form
        .as_ref()
        .is_some_and(|form| !form.unsupported.is_empty());
    let display_blocks: Vec<Vec<crate::model::RichNode>> = presentation
        .form
        .as_ref()
        .map(|form| {
            form.blocks
                .iter()
                .map(|block| match block {
                    FormBlock::Display(block) => shell
                        .core
                        .active_team
                        .as_ref()
                        .and_then(|team| shell.core.workspaces.get(team))
                        .map(|workspace| {
                            crate::blocks::block_nodes(
                                crate::blocks::BlockCtx::new(workspace, &shell.media),
                                block,
                            )
                        })
                        .unwrap_or_default(),
                    FormBlock::Input(_) => vec![],
                })
                .collect()
        })
        .unwrap_or_default();
    let media_epoch = shell.media_epoch;
    drop(shell);
    rsx! {
        div {class:"scrim", onclick:move |_|crate::app_modal::close(state)}
        section {
            id:"shell-overlay-dialog",
            class:"modal app-modal", role:"dialog", "aria-modal":"true",
            "aria-labelledby":"app-modal-title", tabindex:"-1",
            "data-keyboard-dialog":"true", "data-dialog-kind":"other",
            "data-dialog-generation":presentation.reset_revision,
            onkeydown:move |event|if event.key()==Key::Escape {
                event.prevent_default(); event.stop_propagation(); crate::app_modal::close(state);
            },
            onmounted:move |_|{dioxus::document::eval("requestAnimationFrame(()=>{const d=document.getElementById('shell-overlay-dialog');(d?.querySelector('#app-modal-form input:not(:disabled),#app-modal-form textarea:not(:disabled),#app-modal-form select:not(:disabled)')||d?.querySelector('button:not(:disabled)')||d)?.focus({preventScroll:true});});");},
            header {
                h2 {id:"app-modal-title", "{presentation.title}"}
                button {r#type:"button",class:"icon-btn","aria-label":"Close app form",
                    onclick:move |_|crate::app_modal::close(state),{icon(Icon::Close,"icon")}}
            }
            form {id:"app-modal-form",
                onsubmit:move |event|{event.prevent_default();crate::app_modal::submit(state);},
                div {class:"app-modal-body",
                    if presentation.loading {p {class:"app-modal-status",role:"status","Loading form…"}}
                    if let Some(form)=presentation.form.as_ref() {
                        for (index, block) in form.blocks.iter().enumerate() {
                            match block {
                                FormBlock::Display(_) => rsx! {
                                    div {class:"app-modal-display",key:"display-{index}",
                                        for node in &display_blocks[index] {{super::rich::rich_node(node,media_epoch,state)}}
                                    }
                                },
                                FormBlock::Input(input_key) => {
                                    if let Some(field)=form.fields.get(input_key) {
                                        let dom_id=field_id(input_key);
                                        let shape=control_shape(&field.kind);
                                        rsx! {ModalField {
                                            key:"{form.view_id}-{presentation.reset_revision}-{dom_id}-{shape}-{field.revision}",
                                            state, input_key:input_key.clone(), field:field.clone(), locked,
                                        }}
                                    } else {rsx!{}}
                                }
                            }
                        }
                    }
                    if unsupported {p {class:"app-modal-error",role:"alert","This form includes fields that cannot be submitted here yet."}}
                }
                if let Some(error)=presentation.view_error.as_ref() {
                    div {class:"app-modal-notice",p {class:"app-modal-error",role:"alert","{error}"}}
                }
                if presentation.busy {
                    div {class:"app-modal-notice",p {class:"app-modal-status",role:"status","Submitting…"}}
                } else if presentation.uncertain {
                    div {class:"app-modal-notice",p {class:"app-modal-status",role:"status","Submission has not been confirmed. Your entries are retained."}}
                }
                footer {
                    if presentation.can_back {
                        button {r#type:"button",disabled:locked,onclick:move |_|crate::app_modal::back(state),"Back"}
                    }
                    button {r#type:"button",onclick:move |_|crate::app_modal::close(state),"{presentation.close_label}"}
                    span {class:"spacer"}
                    if let Some(label)=presentation.submit_label.as_ref() {
                        button {r#type:"submit",class:"primary",disabled:locked||unsupported,
                            if presentation.busy {"Submitting…"} else {"{label}"}}
                    }
                }
            }
        }
    }
}

#[component]
fn ModalField(
    state: Signal<ShellState>,
    input_key: InputKey,
    field: FormField,
    locked: bool,
) -> Element {
    // Capture once per keyed mount. Current values/errors may rerender after
    // every keystroke, but native defaults must not overwrite the active DOM.
    let seed = use_hook(|| field.value.clone());
    let id = field_id(&input_key);
    let label_id = format!("{id}-label");
    let hint_id = format!("{id}-hint");
    let error_id = format!("{id}-error");
    let described_by = format!("{hint_id} {error_id}");
    let invalid = field.error.is_some().to_string();
    let required = (!field.optional).to_string();
    let placeholder = field.placeholder.as_deref().unwrap_or_default();
    let control = match &field.kind {
        InputKind::Text { multiline, .. } => {
            let text = match seed {
                InputValue::Text(text) => text,
                _ => String::new(),
            };
            let key = input_key.clone();
            if *multiline {
                rsx! {
                    textarea {id:"{id}",name:"{id}",initial_value:"{text}",placeholder:"{placeholder}",
                        disabled:locked,"aria-labelledby":"{label_id}","aria-describedby":"{described_by}",
                        "aria-invalid":"{invalid}","aria-required":"{required}",
                        oninput:move |event|crate::app_modal::set_value(state,key.clone(),InputValue::Text(event.value()))}
                }
            } else {
                rsx! {
                    input {id:"{id}",name:"{id}",r#type:"text",initial_value:"{text}",placeholder:"{placeholder}",
                        autocorrect:"off",autocapitalize:"off",disabled:locked,
                        "aria-labelledby":"{label_id}","aria-describedby":"{described_by}",
                        "aria-invalid":"{invalid}","aria-required":"{required}",
                        oninput:move |event|crate::app_modal::set_value(state,key.clone(),InputValue::Text(event.value()))}
                }
            }
        }
        InputKind::StaticSelect {
            options, multiple, ..
        } => {
            let selected = selected(&seed);
            let key = input_key.clone();
            let name = id.clone();
            let choices = options.clone();
            rsx! {select {id:"{id}",name:"{id}",multiple:*multiple,
                size:if *multiple {options.len().clamp(3,6)}else{1},disabled:locked,
                "aria-labelledby":"{label_id}","aria-describedby":"{described_by}",
                "aria-invalid":"{invalid}","aria-required":"{required}",
                onchange:move |event|crate::app_modal::set_value(state,key.clone(),InputValue::Selected(selected_values(&event,&name,&choices))),
                if !*multiple {option {value:"",initial_selected:selected.is_empty(),"{placeholder}"}}
                for (index,option) in options.iter().enumerate() {
                    option {key:"{option.value}",value:"option-{index}",initial_selected:selected.contains(&option.value),
                        if let Some(group)=option.group.as_ref() {"{group} — "}
                        "{option.text}"
                    }
                }
            }}
        }
        InputKind::Checkboxes { options } | InputKind::Radio { options } => {
            let selected = selected(&seed);
            let radio = matches!(&field.kind, InputKind::Radio { .. });
            rsx! {div {id:"{id}",class:"app-modal-options",role:if radio{"radiogroup"}else{"group"},
                "aria-labelledby":"{label_id}","aria-describedby":"{described_by}",
                "aria-invalid":"{invalid}","aria-required":"{required}",
                for (index,option) in options.iter().enumerate() {
                    {let key=input_key.clone();let name=id.clone();let choices=options.clone();rsx! {
                        label {class:"app-modal-option",key:"{option.value}",
                            input {r#type:if radio{"radio"}else{"checkbox"},name:"{id}",value:"option-{index}",
                                initial_checked:selected.contains(&option.value),disabled:locked,
                                onchange:move |event|crate::app_modal::set_value(state,key.clone(),InputValue::Selected(selected_values(&event,&name,&choices)))}
                            span {"{option.text}"}
                        }
                    }}
                }
            }}
        }
        InputKind::Date | InputKind::Time { .. } | InputKind::DateTime => {
            let kind = field.kind.clone();
            let key = input_key.clone();
            let (input_type, initial) = match &seed {
                InputValue::Date(value) => ("date", value.clone().unwrap_or_default()),
                InputValue::Time(value) => ("time", value.clone().unwrap_or_default()),
                InputValue::DateTime(value) => ("datetime-local", datetime_local_value(*value)),
                _ => ("text", String::new()),
            };
            rsx! {input {id:"{id}",name:"{id}",r#type:input_type,initial_value:"{initial}",disabled:locked,
                "aria-labelledby":"{label_id}","aria-describedby":"{described_by}",
                "aria-invalid":"{invalid}","aria-required":"{required}",
                oninput:move |event|{
                    let value=event.value();
                    match kind {
                        InputKind::Date=>crate::app_modal::set_value(state,key.clone(),InputValue::Date((!value.is_empty()).then_some(value))),
                        InputKind::Time{..}=>crate::app_modal::set_value(state,key.clone(),InputValue::Time((!value.is_empty()).then_some(value))),
                        _=>match parse_datetime_local(&value) {
                            Ok(value)=>crate::app_modal::set_value(state,key.clone(),InputValue::DateTime(value)),
                            Err(error)=>crate::app_modal::set_field_error(state,key.clone(),error),
                        }
                    }
                }
            }}
        }
        InputKind::ExternalSelect { .. } | InputKind::MultiExternalSelect { .. } => {
            rsx! {super::app_modal_external::ModalExternal {state,input_key:input_key.clone(),field:field.clone(),id:id.clone(),locked}}
        }
        InputKind::User
        | InputKind::MultiUser { .. }
        | InputKind::MultiChannel { .. }
        | InputKind::MultiConversation { .. }
        | InputKind::Conversation
        | InputKind::Channel => {
            rsx! {super::app_modal_users::ModalUsers {
                state,input_key:input_key.clone(),field:field.clone(),id:id.clone(),locked,
            }}
        }
        InputKind::Unsupported { .. } => {
            rsx! {p {class:"app-modal-hint","This field is not supported yet."}}
        }
    };
    rsx! {div {class:"app-modal-field","data-block-id":"{input_key.block_id}","data-action-id":"{input_key.action_id}",
        label {id:"{label_id}",r#for:"{id}",class:"app-modal-label","{field.label}",
            span {class:"app-modal-required",if field.optional {"Optional"}else{"Required"}}}
        {control}
        p {id:"{hint_id}",class:"app-modal-hint",
            if let Some(hint)=field.hint.as_ref() {"{hint}"}
            if let InputKind::Time{timezone:Some(timezone)}=&field.kind {" ({timezone})"}
        }
        p {id:"{error_id}",class:"app-modal-error",role:"alert",if let Some(error)=field.error.as_ref(){"{error}"}}
    }}
}

fn selected(value: &InputValue) -> Vec<String> {
    match value {
        InputValue::Selected(values) => values.clone(),
        _ => vec![],
    }
}

fn selected_values(event: &FormEvent, name: &str, options: &[OptionSpec]) -> Vec<String> {
    event
        .values()
        .into_iter()
        .filter_map(|(key, value)| {
            if key != name {
                return None;
            }
            let dioxus::html::FormValue::Text(value) = value else {
                return None;
            };
            let index = value.strip_prefix("option-")?.parse::<usize>().ok()?;
            options.get(index).map(|option| option.value.clone())
        })
        .collect()
}

fn field_id(key: &InputKey) -> String {
    fn encoded(text: &str) -> String {
        text.as_bytes()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }
    format!(
        "app-input-{}-{}",
        encoded(&key.block_id),
        encoded(&key.action_id)
    )
}

// A remote text field can change from single-line to multiline without changing
// its wire type. Remount that DOM shape using the retained current value.
fn control_shape(kind: &InputKind) -> String {
    let shape = match kind {
        InputKind::Text {
            multiline: true, ..
        } => "textarea",
        InputKind::Text {
            multiline: false, ..
        } => "text",
        InputKind::StaticSelect { multiple: true, .. } => "multi",
        InputKind::StaticSelect {
            multiple: false, ..
        } => "select",
        InputKind::Checkboxes { .. } => "checkboxes",
        InputKind::Radio { .. } => "radio",
        InputKind::Conversation => "conversation",
        InputKind::Channel => "channel",
        InputKind::MultiChannel { .. } => "channels",
        InputKind::MultiConversation { .. } => "conversations",
        InputKind::ExternalSelect { .. } => "external",
        InputKind::MultiExternalSelect { .. } => "external-options",
        InputKind::User => "user",
        InputKind::MultiUser { .. } => "users",
        InputKind::Date => "date",
        InputKind::Time { .. } => "time",
        InputKind::DateTime => "datetime",
        InputKind::Unsupported { .. } => "unsupported",
    };
    // Option insertion/removal/reordering changes native option identity. Seed
    // the remounted control from current typed state, never its first value.
    match kind {
        InputKind::StaticSelect { options, .. }
        | InputKind::Checkboxes { options }
        | InputKind::Radio { options } => format!(
            "{shape}:{}",
            serde_json::to_string(
                &options
                    .iter()
                    .map(|option| &option.value)
                    .collect::<Vec<_>>()
            )
            .expect("string list serializes")
        ),
        _ => shape.into(),
    }
}
