//! Original-message confirmation; mounting and keyboard focus never dispatch a delete.
use crate::{slack_lists::comments::own_delete as controller, state::ShellState};
use dioxus::prelude::*;

pub(crate) fn panel(state: Signal<ShellState>) -> Element {
    let Some(view) = controller::presentation(&state.read()) else {
        return rsx! {};
    };
    rsx! { Confirmation { key: "comment-delete-{view.generation}-{view.id}", state, id: view.id, generation: view.generation } }
}

#[component]
fn Confirmation(state: Signal<ShellState>, id: u64, generation: u64) -> Element {
    let Some(view) = controller::presentation(&state.read())
        .filter(|view| view.id == id && view.generation == generation)
    else {
        return rsx! {};
    };
    let options = serde_json::json!({"id":id,"generation":generation,"ts":view.preview.ts});
    let preview_text = format!(
        "{}, {}. {}",
        view.preview.author,
        view.preview.timestamp,
        crate::view::rich::message_plain_text(&view.preview)
    );
    let snapshot = state.read();
    rsx! {
        div { class: "slack-list-editor-shade",
            section {
                id: "slack-list-comment-delete",
                class: "slack-list-cell-editor slack-list-comment-delete",
                role: "alertdialog",
                "aria-modal": "true",
                "aria-labelledby": "slack-list-comment-delete-title",
                "aria-describedby": "slack-list-comment-delete-description",
                "aria-busy": view.busy,
                "data-keyboard-dialog": "true",
                "data-escape-local": "true",
                "data-delete-id": id,
                "data-delete-generation": generation,
                tabindex: "-1",
                onmounted: move |_| { dioxus::document::eval(&format!("{FOCUS}\nmountListCommentDelete({options});")); },
                onkeydown: move |event| {
                    if event.key() == Key::Escape {
                        event.prevent_default();
                        event.stop_propagation();
                        controller::cancel(state, id);
                    }
                },
                h3 { id: "slack-list-comment-delete-title", "Delete message" }
                p { id: "slack-list-comment-delete-description",
                    "Are you sure you want to delete this message? This cannot be undone."
                }
                if let Some(note) = view.secondary_note {
                    p { class: "slack-list-comment-delete-note", "{note}" }
                }
                div { class: "slack-list-comment-delete-preview", role: "group", "aria-label": "Message preview",
                    p { class: "slack-list-help", "{preview_text}" }
                    // ReadOnlyThread removes message/reaction mutation controls;
                    // inert also prevents profile, link, file and embedded actions.
                    div { inert: true, "aria-hidden": "true",
                        {crate::view::message::message_row(state, &snapshot, &view.preview, &view.channel, None, crate::view::message::RowSurface::ReadOnlyThread, None)}
                    }
                }
                if let Some(notice) = view.notice {
                    p { id: "slack-list-comment-delete-status", role: "status", "{notice}" }
                } else if view.busy {
                    p { id: "slack-list-comment-delete-status", role: "status", "Working…" }
                }
                footer {
                    if view.can_check {
                        button { id: "slack-list-comment-delete-check", r#type: "button", class: "slack-list-action",
                            disabled: view.busy, onclick: move |_| controller::check_result(state, id), "Check result"
                        }
                    }
                    if view.can_retry_local {
                        button { id: "slack-list-comment-delete-retry", r#type: "button", class: "slack-list-action",
                            disabled: view.busy, onclick: move |_| controller::retry_local(state, id), "Retry saving result"
                        }
                    }
                    button { id: "slack-list-comment-delete-cancel", r#type: "button", class: "slack-list-action",
                        disabled: view.busy, onclick: move |_| controller::cancel(state, id),
                        if view.terminal { "Done" } else { "Cancel" }
                    }
                    button { id: "slack-list-comment-delete-confirm", r#type: "button", class: "slack-list-action slack-list-comment-delete-danger",
                        disabled: view.busy || !view.can_confirm,
                        onclick: move |_| controller::confirm(state, id), "Delete"
                    }
                }
            }
        }
    }
}

// Tab containment is the existing shell overlay_keyboard.js contract. This
// small mount only owns initial/return focus and never adopts successor nodes.
const FOCUS: &str = r#"
function mountListCommentDelete(options, environment = globalThis) {
 const {document,MutationObserver,requestAnimationFrame}=environment;
 const panel=document.getElementById('slack-list-comment-delete');
 const comments=document.getElementById('slack-list-comments');
 const dialog=comments?.closest('.slack-list-dialog');
 if(!panel||!comments||!dialog||!comments.contains(panel)
   ||panel.dataset.deleteId!==String(options.id)
   ||panel.dataset.deleteGeneration!==String(options.generation)
   ||comments.dataset.commentsGeneration!==String(options.generation))return false;
 const previous=document.activeElement;
 const opener=[...comments.querySelectorAll('[data-list-comment-delete-ts]')]
   .find(node=>node.dataset.listCommentDeleteTs===options.ts);
 const reply=comments.querySelector('#slack-list-comment-draft');
 const back=comments.querySelector('#slack-list-comments-back');
 const visible=node=>node?.isConnected&&!node.disabled
   &&!node.closest('[hidden],[inert]')&&node.getClientRects().length>0;
 const original=()=>comments.isConnected&&dialog.isConnected
   &&document.getElementById('slack-list-comments')===comments
   &&comments.dataset.commentsGeneration===String(options.generation);
 const initial=()=>panel.querySelector('#slack-list-comment-delete-confirm:not(:disabled)')
   ||panel.querySelector('#slack-list-comment-delete-check:not(:disabled)')
   ||panel.querySelector('#slack-list-comment-delete-retry:not(:disabled)')
   ||panel.querySelector('#slack-list-comment-delete-cancel:not(:disabled)')||panel;
 let waitingForPrimary=true;
 const focusReadyPrimary=()=>{
   if(!waitingForPrimary||!original()
     ||document.getElementById('slack-list-comment-delete')!==panel)return;
   const primary=panel.querySelector('#slack-list-comment-delete-confirm:not(:disabled)');
   if(!primary)return;
   waitingForPrimary=false;
   if(document.activeElement===panel)primary.focus({preventScroll:true});
 };
 const observer=new MutationObserver(()=>{
   if(panel.isConnected){focusReadyPrimary();return;}
   observer.disconnect();
   if(!original()||comments.querySelector('#slack-list-comment-delete'))return;
   const active=document.activeElement;
   if(active&&active!==document.body&&active.isConnected)return;
   [opener,reply,back].find(visible)?.focus({preventScroll:true});
 });
 observer.observe(comments,{childList:true,subtree:true,attributes:true,attributeFilter:['disabled','aria-busy']});
 requestAnimationFrame(()=>{
   if(!original()||document.getElementById('slack-list-comment-delete')!==panel)return;
   const active=document.activeElement;
   if(active&&active!==document.body&&active!==previous&&!panel.contains(active))return;
   const target=initial();
   target.focus({preventScroll:true});
   if(target!==panel)waitingForPrimary=false;
 });
 return true;
}
"#;
