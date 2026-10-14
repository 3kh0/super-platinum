//! Explicit review beside an existing mounted composer. This surface never
//! enrolls drafts or enables ordinary remote autosave.
use super::*;
use std::sync::Arc;
use super_platinum_core::drafts::store::sync::RemoteChoice;
mod choices;
mod controller;
mod read;
use controller::{Completion, Failure};

#[component]
pub(super) fn RemoteReview(state: Signal<ShellState>, target: ComposerTarget) -> Element {
    let mut prepared = use_signal(|| None::<Arc<controller::Prepared>>);
    let mut selected = use_signal(|| None::<RemoteChoice>);
    let mut busy = use_signal(|| false);
    let mut feedback = use_signal(|| None::<&'static str>);
    let current = prepared().filter(|review| review.destination_current(&state.read()));
    if current.is_none() && feedback().is_none() && !controller::available(&state.read(), target) {
        return rsx! {};
    }
    let id = match target {
        ComposerTarget::Channel => "channel-draft-review",
        ComposerTarget::Thread => "thread-draft-review",
    };
    let opener = format!("{id}-open");
    let stale = current
        .as_ref()
        .is_some_and(|review| !review.current(&state.read()));
    rsx! {
        section {class:"draft-remote-review", "aria-label":"Draft version review",
            div {class:"scheduled-actions",
                if controller::can_finish(&state.read(),target) {
                    button {id:format!("{id}-finish"),disabled:busy(),onclick:move |_|{
                        if busy(){return;}
                        busy.set(true);selected.set(None);prepared.set(None);feedback.set(None);
                        dioxus::core::spawn_forever(async move {
                            let result=controller::finish(state,target).await;
                            let message=match result {Ok(Completion::Applied)=>None,Ok(outcome)=>Some(completion(outcome)),Err(error)=>Some(failure(error))};
                            if let Ok(mut value)=feedback.try_write(){*value=message;}
                            if let Ok(mut value)=busy.try_write(){*value=false;}
                        });
                    },"Finish applying chosen version"}
                }
                button {id:opener.clone(),disabled:busy(),"aria-expanded":current.is_some(),"aria-controls":id,
                    onclick:move |_|{
                        if busy(){return;}
                        busy.set(true);selected.set(None);prepared.set(None);feedback.set(None);
                        dioxus::core::spawn_forever(async move {
                            let result=controller::open(state,target).await;
                            match result {
                                Ok(review)=>{if let Ok(mut value)=prepared.try_write(){*value=Some(Arc::new(review));}},
                                Err(error)=>{if let Ok(mut value)=feedback.try_write(){*value=Some(failure(error));}},
                            }
                            if let Ok(mut value)=busy.try_write(){*value=false;}
                        });
                    },
                    if busy()&&current.is_none(){"Working\u{2026}"}else if current.is_some(){"Check versions again"}else{"Review draft versions"}
                }
            }
            if let Some(review)=current.clone() {
                div {id,class:"draft-remote-review-versions",role:"region","aria-label":"Saved and Slack draft versions",
                    onmounted:move |_|focus(&format!("{id}-choices")),
                    onkeydown:{let opener=opener.clone();move |event|if event.key()==Key::Escape&&!busy(){event.stop_propagation();prepared.set(None);selected.set(None);feedback.set(None);focus(&opener);}},
                    p {class:"scheduled-hint","Review all saved versions, then choose the draft to use here. Nothing will be sent or changed in Slack."}
                    div {id:format!("{id}-choices"),role:"radiogroup",tabindex:"-1","aria-label":"Draft versions",
                        for (index,row) in review.choices.iter().enumerate() {
                            div {class:"scheduled-item",key:"{index}","data-remote-review-choice":index,
                                if let Some(choice)=row.choice.clone() {
                                    label {class:"schedule-toggle",
                                        input {r#type:"radio",name:id,"data-remote-review-kind":choice_kind(&choice),checked:selected().as_ref()==Some(&choice),disabled:busy()||stale||row.unavailable.is_some(),
                                            onchange:move |_|selected.set(Some(choice.clone()))}
                                        strong {"{row.label}"}
                                    }
                                } else {strong {"{row.label}"}}
                                {preview(state,review.team(),&row.content)}
                                if let Some(raw)=&row.retained {
                                    details {summary {"Retained saved content details"}pre {style:"white-space: pre-wrap; overflow-wrap: anywhere;","{serde_json::to_string_pretty(raw).unwrap_or_default()}"}}
                                }
                                if let Some(reason)=row.unavailable {p {class:"scheduled-hint","{reason}"}}
                            }
                        }
                    }
                    p {class:"scheduled-hint","Using a version replaces the other reviewed local versions. Newer edits are kept separately and require another review."}
                    if stale {p {class:"scheduled-hint",role:"status","This draft or its destination changed. Check versions again before choosing."}}
                    div {class:"scheduled-actions",
                        button {id:format!("{id}-cancel"),disabled:busy(),onclick:{let opener=opener.clone();move |_|{prepared.set(None);selected.set(None);feedback.set(None);focus(&opener);}},"Cancel"}
                        button {id:format!("{id}-apply"),disabled:busy()||stale||selected().is_none(),onclick:{let review=review.clone();move |_|{
                            if busy(){return;} let Some(choice)=selected()else{return;};
                            busy.set(true);feedback.set(None);let review=review.clone();
                            dioxus::core::spawn_forever(async move {
                                let result=controller::apply(state,(*review).clone(),choice).await;
                                let message=match result {Ok(Completion::Applied)=>None,Ok(outcome)=>Some(completion(outcome)),Err(error)=>Some(failure(error))};
                                if let Ok(mut value)=prepared.try_write(){*value=None;}
                                if let Ok(mut value)=selected.try_write(){*value=None;}
                                if let Ok(mut value)=feedback.try_write(){*value=message;}
                                if let Ok(mut value)=busy.try_write(){*value=false;}
                            });
                        }},if busy(){"Saving choice\u{2026}"}else{"Use selected version"}}
                    }
                }
            }
            if let Some(message)=feedback(){p {class:"scheduled-hint",role:"status","{message}"}}
        }
    }
}

fn choice_kind(choice: &RemoteChoice) -> &'static str {
    use super_platinum_core::drafts::store::sync::LocalChoice;
    match choice {
        RemoteChoice::Remote => "remote",
        RemoteChoice::Local(LocalChoice::Current) => "current",
        RemoteChoice::Local(LocalChoice::Blocked { .. }) => "blocked",
        RemoteChoice::Local(LocalChoice::Alternate { .. }) => "alternate",
    }
}

fn preview(state: Signal<ShellState>, team: &str, content: &choices::Content) -> Element {
    match content {
        choices::Content::Local(local) => rsx! {
            if local.composer.text.is_empty(){p {class:"scheduled-hint","Empty text"}}
            else {p {class:"scheduled-content",style:"white-space: pre-wrap; overflow-wrap: anywhere;","{local.composer.text}"}}
            if local.composer.reply_broadcast {p {class:"scheduled-hint","Also shown in the channel"}}
            for file in &local.attachments {p {class:"scheduled-hint","Attached file: {file.name}"}}
        },
        choices::Content::Snapshot(snapshot) => {
            let shell = state.read();
            let nodes = shell
                .core
                .workspaces
                .get(team)
                .map(|workspace| {
                    let context = crate::blocks::BlockCtx::new(workspace, &shell.media);
                    snapshot
                        .blocks
                        .iter()
                        .flat_map(|block| crate::blocks::block_nodes(context, block))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let details = serde_json::to_string_pretty(snapshot).unwrap_or_default();
            rsx! {
                div {class:"scheduled-content",for node in &nodes {{super::super::rich::rich_node(node,shell.media_epoch,state)}}}
                if nodes.is_empty(){p {class:"scheduled-hint","No displayable text. Review the saved content details below."}}
                if !snapshot.file_ids.is_empty(){p {class:"scheduled-hint","{snapshot.file_ids.len()} saved files"}}
                details {summary {"Saved content details"}pre {style:"white-space: pre-wrap; overflow-wrap: anywhere;","{details}"}}
            }
        }
    }
}
fn failure(error: Failure) -> &'static str {
    match error {
        Failure::Cooldown => {
            "Slack asked us to wait. Check versions again later; nothing was sent."
        }
        Failure::Read(_) => {
            "The current Slack draft could not be verified. Saved versions and the composer are unchanged. Check again when connected."
        }
        Failure::Fault(crate::drafts::Fault::Stale) => {
            "The draft or its saved versions changed. Check versions again before choosing."
        }
        Failure::Fault(crate::drafts::Fault::Pending) => {
            "Local edits are still being saved. Wait for saving to finish, then check versions again."
        }
        Failure::Fault(
            crate::drafts::Fault::Unsupported | crate::drafts::Fault::AlternateEdits,
        ) => {
            "This content cannot be installed safely yet. Keep the current draft and review its saved versions in Drafts."
        }
        Failure::Fault(_) => {
            "The review could not finish saving. Keep the app open while local changes are pending, then check versions again. Nothing was sent."
        }
    }
}
fn completion(outcome: Completion) -> &'static str {
    match outcome {
        Completion::Applied => "The selected draft version is ready. Nothing was sent.",
        Completion::NewerInputPreserved => {
            "Your newer input was saved separately. Check versions again to review it with the selected draft."
        }
        Completion::InputNotSaved => {
            "Your newer input is still in the composer but could not be confirmed saved. Keep the app open and check versions again when storage is available."
        }
        Completion::Deferred(projection::ProjectionDeferral::Composing) => {
            "The choice was saved. Finish composing, then use Finish applying chosen version. If you entered new text, check versions again to review it."
        }
        Completion::Deferred(_) | Completion::ReviewAgain => {
            "The choice was saved, but the composer handoff is still pending. Finish applying it if the input is unchanged; otherwise check versions again. Your current input has not been discarded."
        }
    }
}
fn focus(id: &str) {
    let id = serde_json::to_string(id).unwrap();
    spawn(async move {
        let _ = document::eval(&format!(
            "document.getElementById({id})?.focus({{preventScroll:true}});"
        ))
        .await;
    });
}
