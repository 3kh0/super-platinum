//! Existing hosted-file Share UI. Parent owns admission and durable outcomes.
use crate::{
    file_share::{self, Phase},
    icons::{Icon, icon},
    state::ShellState,
};
use dioxus::prelude::*;
mod pressure;
mod recovery;
mod status;
pub(crate) use recovery::{RecoveryDialog, SavedFileShares};

fn destination_icon(shell: &ShellState, destination: &file_share::Destination) -> Icon {
    let direct = shell
        .core
        .active_team
        .as_ref()
        .and_then(|team| shell.core.workspaces.get(team))
        .and_then(|w| w.channels.get(&destination.id))
        .is_some_and(|c| c.is_im || c.is_mpim);
    if direct || destination.id.starts_with("person:") {
        Icon::DirectMessages
    } else if destination.private {
        Icon::Lock
    } else {
        Icon::Tag
    }
}
fn frame(root: &str, generation: &str, selectors: &[&str], scroll: bool) {
    let script = include_str!("file_share/frame.js");
    let root = serde_json::to_string(root).unwrap();
    let generation = serde_json::to_string(generation).unwrap();
    let selectors = serde_json::to_string(selectors).unwrap();
    document::eval(&format!(
        "{script}({root},{generation},{selectors},{scroll});"
    ));
}
fn active_index(index: usize, len: usize) -> usize {
    index.min(len.saturating_sub(1))
}

fn close(state: Signal<ShellState>, id: &str) {
    // Parent captures an exact editable DOM note before hiding, then restores
    // focus. A failed read keeps this dialog mounted with its draft intact.
    dioxus::core::spawn_forever(file_share::close_dialog(state, id.to_owned()));
    frame("file-share-dialog", id, &["#file-share-note"], false);
}

fn open_slack(mut state: Signal<ShellState>, id: &str) {
    let link = file_share::visible(&state.peek())
        .filter(|d| d.id == id)
        .and_then(|d| d.slack_url.clone());
    if let Some(link) = link {
        if crate::media::open_external(&link).is_err() {
            state
                .write()
                .show_toast("This Slack link could not be opened.");
        }
    }
}
fn choose(
    mut state: Signal<ShellState>,
    id: &str,
    destination: &str,
    mut query: Signal<String>,
    mut active: Signal<usize>,
    mut searching: Signal<bool>,
) -> bool {
    let result = file_share::choose(&mut state.write(), id, destination);
    match result {
        Ok(()) => {
            query.set(String::new());
            active.set(0);
            searching.set(false);
            true
        }
        Err(error) => {
            state.write().show_toast(error.message());
            false
        }
    }
}

fn submit(state: Signal<ShellState>, id: String) {
    file_share::start(state, id.clone());
    if file_share::visible(&state.peek()).is_some_and(|d| d.id == id && d.phase != Phase::Editing) {
        frame("file-share-dialog", &id, &["#file-share-note"], false);
    }
}

fn save_result(state: Signal<ShellState>, id: &str, attempt: &str) {
    file_share::retry_result(state, id.to_owned(), attempt.to_owned());
    if file_share::retry_status(&state.peek(), id) == Some(file_share::RetryState::Saving) {
        // The clicked button becomes disabled and disappears after success.
        // Keep focus in the same mounted read-only editor, with no selection reset.
        frame("file-share-dialog", id, &["#file-share-note"], false);
    }
}

#[component]
pub(crate) fn FileShareDialog(mut state: Signal<ShellState>) -> Element {
    let id = use_hook(|| {
        file_share::visible(&state.peek())
            .map(|d| d.id.clone())
            .unwrap_or_default()
    });
    let mut query = use_signal(String::new);
    let mut active = use_signal(|| 0usize);
    let mut searching = use_signal(|| false);
    // Only a successful destination choice may focus the newly mounted button.
    // A Close read failure also remounts it and must retain note selection.
    let mut choice_focus = use_signal(|| false);
    let matches = use_memo(move || file_share::destinations(&state.read(), &query.read()));
    let selected_index = use_memo(move || active_index(active(), matches.read().len()));
    let scroll_id = id.clone();
    use_effect(move || {
        let index = selected_index();
        if index < matches.read().len() {
            frame(
                "file-share-dialog",
                &scroll_id,
                &[&format!("#file-share-option-{index}")],
                true,
            );
        }
    });
    let feedback = use_memo(move || {
        file_share::visible(&state.read()).map(|d| (d.id.clone(), d.phase, d.error))
    });
    use_effect(move || {
        if let Some((owner, phase, error)) = feedback()
            && (error.is_some() || matches!(phase, Phase::Outcome | Phase::StorageFailed))
        {
            frame("file-share-dialog", &owner, &[".file-share-feedback"], true);
        }
    });
    let snapshot = state.read();
    let Some(dialog) = file_share::visible(&snapshot).filter(|d| d.id == id) else {
        return rsx! {};
    };
    let editable = dialog.phase == Phase::Editing && !dialog.closing;
    let picker_open = editable && (dialog.destination.is_none() || searching());
    let can_submit =
        editable && dialog.editor_ready && file_share::destination_current(&snapshot, dialog);
    let blocks = serde_json::to_string(&dialog.note).unwrap_or_else(|_| "[]".into());
    let selected = dialog.destination.clone();
    let result = matches!(dialog.phase, Phase::Outcome | Phase::StorageFailed)
        .then(|| {
            dialog.receipt.as_ref().map(|receipt| {
                status::receipt(
                    receipt,
                    file_share::retained_status(&snapshot, &receipt.attempt_id),
                )
            })
        })
        .flatten();
    let scrim_id = id.clone();
    let key_id = id.clone();
    let close_id = id.clone();
    let submit_id = id.clone();
    let link_id = id.clone();
    let choose_id = id.clone();
    let mount_id = id.clone();
    let input_mount_id = id.clone();
    let change_id = id.clone();
    let discard_id = id.clone();
    let keep_id = id.clone();
    let retry_id = id.clone();
    let load_retry_id = id.clone();
    let review_capture_id = id.clone();
    let captured = file_share::has_capture(&snapshot, &id);
    let retry_attempt = dialog.receipt.as_ref().map(|r| r.attempt_id.clone());
    let retry = file_share::retry_status(&snapshot, &id);
    let retrying = retry == Some(file_share::RetryState::Saving);
    rsx! {
        div {class:"scrim",onclick:move |_|close(state,&scrim_id)}
        section {id:"file-share-dialog",class:"modal file-share-dialog",role:"dialog","aria-modal":"true","aria-labelledby":"file-share-title",
            "data-keyboard-dialog":"true","data-dialog-kind":"file-share","data-dialog-generation":id.clone(),"data-file-share-id":id.clone(),"data-file-share-phase":format!("{:?}",dialog.phase),"data-file-share-closing":dialog.closing,"data-escape-local":"true",
            onmounted:move |_|{document::eval(include_str!("file_share/keyboard.js"));frame("file-share-dialog",&mount_id,&["#file-share-destination","#file-share-close"],false);},
            onkeydown:move |event|if event.key()==Key::Escape {event.prevent_default();event.stop_propagation();close(state,&key_id);},
            header {
                h2 {id:"file-share-title",if editable {"Share file"}else{"File share"}}
                button {id:"file-share-close",r#type:"button",class:"icon-btn","aria-label":"Close file share",disabled:dialog.closing,onclick:move |_|close(state,&close_id),{icon(Icon::Close,"icon")}}
            }
            div {class:"file-share-body",
                if editable && dialog.destination.is_some() && !file_share::destination_current(&snapshot, dialog) {
                    p {class:"file-share-hint", "Choose the destination again to confirm the current recipient. Your message is retained."}
                }
                div {class:"file-share-source",{icon(Icon::AttachFile,"icon")}strong {"{dialog.file_title}"}}
                if let Some(destination)=&selected {
                    div {class:"file-share-selected",
                        if dialog.receipt.is_none() {{icon(destination_icon(&snapshot, destination),"icon sm")}}
                        span {"To {destination.name}"}
                        if editable && !picker_open {button {id:"file-share-change-destination",r#type:"button",
                            onmounted:move |_|{let pending=*choice_focus.peek();if pending {choice_focus.set(false);frame("file-share-dialog",&change_id,&["#file-share-change-destination"],false);}},
                            onclick:move |_|searching.set(true),"Change destination"}}
                    }
                }
                if picker_open {
                    label {r#for:"file-share-destination","Conversation"}
                    input {id:"file-share-destination",value:query(),placeholder:"Search channels and direct messages",autocomplete:"off",role:"combobox","aria-autocomplete":"list","aria-expanded":true,"aria-controls":"file-share-options",
                        "aria-activedescendant":(!matches.read().is_empty()).then(||format!("file-share-option-{}",selected_index())),
                        onmounted:move |_|frame("file-share-dialog",&input_mount_id,&["#file-share-destination"],false),
                        oninput:move |event|{query.set(event.value());active.set(0);},
                        onkeydown:move |event|match event.key(){
                            Key::ArrowDown=>{event.prevent_default();active.set((selected_index()+1).min(matches.read().len().saturating_sub(1)));},
                            Key::ArrowUp=>{event.prevent_default();active.set(selected_index().saturating_sub(1));},
                            Key::Enter=>{event.prevent_default();let destination=matches.read().get(selected_index()).cloned();if let Some(destination)=destination{if choose(state,&choose_id,&destination.id,query,active,searching){choice_focus.set(true);}}},
                            _=>{}
                        }
                    }
                    div {id:"file-share-options",class:"file-share-options",role:"listbox","aria-label":"Available conversations",
                        for (index,destination) in matches.read().iter().enumerate() {
                            {let destination=destination.clone();let choice_icon=destination_icon(&snapshot,&destination);let dialog_id=id.clone();rsx!{
                                button {id:"file-share-option-{index}",key:"{destination.id}",r#type:"button",class:if index==selected_index(){"file-share-option selected"}else{"file-share-option"},role:"option","aria-selected":index==selected_index(),"data-destination":destination.id.clone(),
                                    onclick:move |_|{if choose(state,&dialog_id,&destination.id,query,active,searching){choice_focus.set(true);}},
                                    {icon(choice_icon,"icon sm")}span {"{destination.name}"}
                                }
                            }}
                        }
                        if matches.read().is_empty() {p {class:"file-share-hint",role:"status","No available conversations match."}}
                    }
                    p {class:"file-share-hint","Choose a conversation or search for a person in this workspace."}
                }
                if dialog.editor_ready {
                    super::rich_editor::RichEditor {id:"file-share-note",label:"Add a message (optional)",blocks,autofocus:false,allow_lists:true,disabled:!editable}
                } else if dialog.phase==Phase::Loading {p {class:"file-share-hint",role:"status","Loading saved message…"}}
                pressure::EarlierCapture {state,dialog_id:id.clone()}
                div {class:"file-share-feedback",
                    if retry.is_some() {p {class:"file-share-hint",if retrying {"Saving the result on this computer..."}else{"Save the result on this computer without sharing the file again."}}}
                    if let Some(error)=dialog.error {p {class:"file-share-error",role:"alert","{error}"}}
                    if let Some(result)=result {
                        div {class:"file-share-status",role:"status",
                            strong {"{result.title}"}p {"{result.detail}"}
                            if result.result_not_saved {p {class:"file-share-error","This result has not been saved. Check in Slack before sharing again."}}
                            if let Some(custom)=result.custom {p {class:"file-share-custom","{custom}"}}
                        }
                    } else if dialog.saved_draft_stale {
                        p {class:"file-share-hint",if dialog.draft_local {"This captured copy is not saved to disk and will be lost when you quit. Keep it for this session or copy it before discarding."}else{"This saved message is read-only. Discarding it removes this exact saved version."}}
                    } else {
                        match dialog.phase {
                            Phase::Preparing=>rsx!{p{class:"file-share-hint",role:"status",if dialog.closing {"Closing…"}else{"Preparing to share…"}}},
                            Phase::Sending=>rsx!{p{class:"file-share-hint",role:"status","Sharing file…"}},
                            Phase::StorageFailed=>rsx!{p{class:"file-share-error",role:"alert","The share status could not be saved. Check in Slack before sharing again."}},
                            Phase::Outcome=>rsx!{p{class:"file-share-hint",role:"status","Sharing is not confirmed. Check in Slack before sharing again."}},
                            Phase::Loading=>rsx!{},
                            Phase::LoadFailed=>rsx!{},
                            Phase::DraftReview=>rsx!{},
                            Phase::Editing=>rsx!{},
                        }
                    }
                }
            }
            footer {
                if captured {button {id:"file-share-review-capture",r#type:"button",onclick:move |_|file_share::review_captured_note(state,review_capture_id.clone()),"Review captured copy"}}
                if dialog.phase==Phase::DraftReview&&dialog.draft_local {button {id:"file-share-keep-session",r#type:"button",disabled:dialog.closing,onclick:move |_|close(state,&keep_id),"Keep for this session"}}
                if let Some(attempt)=retry_attempt.filter(|_|retry.is_some()) {
                    button {id:"file-share-retry-result",r#type:"button",disabled:retrying,"aria-busy":retrying,
                        onclick:move |_|save_result(state,&retry_id,&attempt),"Save result again"}
                }
                if dialog.phase==Phase::LoadFailed {button {id:"file-share-retry-load",r#type:"button",disabled:dialog.closing,onclick:move |_|file_share::retry_open(state,load_retry_id.clone()),"Try loading again"}}
                if dialog.saved_draft_stale {button {id:"file-share-discard-note",r#type:"button",disabled:dialog.closing,onclick:move |_|file_share::discard_saved_note(state,discard_id.clone()),if dialog.draft_local {"Discard captured copy"}else{"Discard saved note"}}}
                if dialog.slack_url.is_some() {button {id:"file-share-open-slack",r#type:"button",onclick:move |_|open_slack(state,&link_id),"Open in Slack"}}
                if !dialog.closing&&matches!(dialog.phase,Phase::Editing|Phase::Preparing|Phase::Sending) {button {id:"file-share-submit",r#type:"button",class:"primary",disabled:!can_submit,onclick:move |_|submit(state,submit_id.clone()),if dialog.phase==Phase::Preparing {"Preparing…"}else if dialog.phase==Phase::Sending {"Sharing…"}else{"Share"}}}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::active_index;
    #[test]
    fn destination_shrink_clamps_keyboard_and_rendered_selection_together() {
        assert_eq!(active_index(2, 3), 2);
        assert_eq!(active_index(2, 1), 0);
        assert_eq!(active_index(2, 0), 0);
        assert_eq!(active_index(usize::MAX, 2), 1);
    }
}
