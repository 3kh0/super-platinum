//! Shared fixture-only consent semantics: content, focus, keyboard and actions.
//! Both enrollment and adoption use the same IDs and local-only consent copy.
use super::*;
use crate::{
    icons::{Icon, icon},
    state::ComposerTarget,
};
use dioxus::prelude::*;
use std::rc::Rc;

pub(crate) fn dialog(
    generation: &str,
    target: ComposerTarget,
    views: &[View],
    pending: &[LocalDraft],
    busy: bool,
    confirm: Rc<dyn Fn()>,
    cancel: Rc<dyn Fn()>,
) -> Element {
    let generation = generation.to_owned();
    let scrim_cancel = cancel.clone();
    let key_cancel = cancel.clone();
    let close_cancel = cancel.clone();
    let footer_cancel = cancel;
    rsx! {
        style {{include_str!("enrollment_fixture/ui.css")}}
            div {class:"scrim",onclick:move |_|scrim_cancel() }
            section {id:"protected-enrollment-dialog",class:"modal button-confirmation protected-enrollment-dialog",role:"dialog","aria-modal":"true","aria-labelledby":"protected-enrollment-title",tabindex:"-1",
                "data-keyboard-dialog":"true","data-dialog-kind":"other","data-dialog-generation":"{generation}","data-escape-local":"true",
                onkeydown:move|event|if event.key()==Key::Escape{event.prevent_default();event.stop_propagation();key_cancel();},
                onmounted:move |_|{document::eval("requestAnimationFrame(()=>document.getElementById('protected-enrollment-submit')?.focus({preventScroll:true}));");},
                header {h2 {id:"protected-enrollment-title",if target==ComposerTarget::Thread{"Enable thread draft recovery?"}else{"Enable channel draft recovery?"}}
                    button {r#type:"button",class:"icon-btn","aria-label":"Close enrollment",onclick:move |_|close_cancel(),{icon(Icon::Close,"icon")}}
                }
                div {id:"protected-enrollment-inventory",class:"button-confirmation-body protected-enrollment-inventory",
                    p {"Keep this draft with recovery on this device. Enabling recovery does not send the draft or change its text."}
                    for view in views {
                        article {class:"protected-enrollment-draft","data-draft-kind":if view.scope().key.thread.is_some(){"thread"}else{"channel"},
                            h3 {if view.scope().key.thread.is_some(){"Thread draft"}else{"Channel draft"}}
                            p {class:"protected-enrollment-content","{view.local().composer.text}"}
                            if !view.local().attachments.is_empty() {p {"Attachments: {view.local().attachments.len()}"}}
                            p {class:"scheduled-hint",if view.inventory().row().is_some(){"Already stored with recovery"}else{"Saved on this device"}}
                            if let Some(value)=view.uncommitted(){p {class:"protected-enrollment-content","Pending input: {value.composer.text}"}}
                            for (label,value) in preserved(view) {p {class:"protected-enrollment-content","{label}: {value.composer.text}"}}
                        }
                    }
                    if !pending.is_empty() {section {id:"protected-enrollment-pending",h3 {"Input waiting to be saved"}
                        for value in pending {p {class:"protected-enrollment-content","{value.composer.text}"}}
                        p {"Recover this input before enabling recovery for another version."}
                    }}
                }
                form {onsubmit:move|event|{event.prevent_default();confirm();},
                    footer {
                        button {id:"protected-enrollment-cancel",r#type:"button",onclick:move |_|footer_cancel(),if busy{"Cancel waiting"}else{"Cancel"}}
                        button {id:"protected-enrollment-submit",r#type:"submit",class:"button-confirmation-submit",disabled:busy,if busy{"Enabling recovery..."}else{"Enable recovery"}}
                    }
                }
            }
    }
}
fn preserved(view: &View) -> Vec<(String, LocalDraft)> {
    let (blocked, alternates) = match view.inventory() {
        Inventory::Local(value) => (value.blocked(), value.alternates()),
        Inventory::Managed(value) => (value.blocked(), value.alternates()),
    };
    let mut values = vec![];
    if let Some(blocked) = blocked {
        values.push(("Input waiting for review".into(), blocked.draft.clone()));
    }
    values.extend(alternates.iter().enumerate().map(|(index, value)| {
        (
            format!("Preserved input {}", index + 1),
            value.draft.clone(),
        )
    }));
    values
}
