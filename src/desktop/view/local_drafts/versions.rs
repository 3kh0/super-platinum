//! Explicit review of all durable local choices; frozen sends stay separate.
use crate::{
    drafts::{Fault, settlement},
    state::ShellState,
};
use dioxus::prelude::*;
use std::sync::Arc;
use super_platinum_core::drafts::{
    DraftKey, LocalDraft,
    store::sync::{LocalChoice, LocalReview},
};

#[component]
pub(super) fn Versions(
    state: Signal<ShellState>,
    account: String,
    epoch: u64,
    draft_key: DraftKey,
) -> Element {
    let mut prepared = use_signal(|| None::<Arc<settlement::Prepared>>);
    let mut selected = use_signal(|| 0usize);
    let mut busy = use_signal(|| false);
    let mut error = use_signal(|| None::<&'static str>);
    let visit = state.read().draft_writes.view_id;
    let id = format!(
        "local-versions-{}-{}-{}",
        draft_key.team,
        draft_key.channel,
        draft_key.thread.as_deref().unwrap_or("channel")
    );
    let opener = format!("{id}-open");
    let current = prepared();
    let is_open = current.is_some();
    let available = current
        .as_ref()
        .map(|prepared| choices(&prepared.review))
        .unwrap_or_default();
    let open_account = account.clone();
    let open_key = draft_key.clone();
    rsx! {
        div {class:"scheduled-actions",
            button {id:opener.clone(),disabled:busy(),"aria-expanded":is_open,"aria-controls":id.clone(),onclick:move |_|{
                if busy(){return;} busy.set(true);error.set(None);selected.set(0);
                let account=open_account.clone();let key=open_key.clone();
                dioxus::core::spawn_forever(async move {
                    let result=settlement::open(state,account,epoch,key).await;
                    if let Ok(mut value)=prepared.try_write(){*value=result.as_ref().ok().map(|review|Arc::new(review.clone()));}
                    if let Err(fault)=result {if let Ok(mut value)=error.try_write(){*value=Some(failure(fault));}}
                    if let Ok(mut value)=busy.try_write(){*value=false;}
                });
            },if busy()&&!is_open{"Loading local versions…"}else{"Review local versions"}}
        }
        if let Some(review)=current {
            section {id:id.clone(),"data-local-version-review":"true",role:"region","aria-label":"Review local versions",
                onmounted:{let first=format!("{id}-choice-0");move |_|focus(&first)},
                onkeydown:{let opener=opener.clone();move |event|if event.key()==Key::Escape&&!busy(){event.stop_propagation();prepared.set(None);error.set(None);focus(&opener);}},
                p {class:"scheduled-hint","Choose which saved local version to keep. Review the text and attached files below."}
                div {role:"radiogroup","aria-label":"Local draft versions",
                    for (index,(_,label,local)) in available.iter().enumerate() {
                        div {class:"scheduled-item",key:"{index}","data-local-version-choice":index,
                            label {class:"schedule-toggle",
                                input {id:format!("{id}-choice-{index}"),r#type:"radio",name:id.clone(),checked:selected()==index,disabled:busy(),onchange:move |_|selected.set(index)}
                                strong {"{label}"}
                            }
                            if local.empty() {p {class:"scheduled-hint","Empty draft"}}
                            if !local.composer.text.is_empty() {p {class:"scheduled-content",style:"white-space: pre-wrap;","{local.composer.text}"}}
                            for file in &local.attachments {p {class:"scheduled-hint","Attached file: {file.name}"}}
                        }
                    }
                }
                p {class:"scheduled-hint","Use the selected version as the local draft and remove the other reviewed local versions. Unconfirmed sends and Slack versions stay separate. Nothing will be sent."}
                div {class:"scheduled-actions",
                    button {disabled:busy(),onclick:{let opener=opener.clone();move |_|{prepared.set(None);error.set(None);focus(&opener);}},"Cancel"}
                    button {disabled:busy()||available.get(selected()).is_none(),onclick:{let review=review.clone();let available=available.clone();let opener=opener.clone();move |_|{
                        if busy(){return;} let Some((choice,_,_))=available.get(selected()).cloned()else{return;};
                        busy.set(true);error.set(None);let review=review.clone();let opener=opener.clone();
                        dioxus::core::spawn_forever(async move {
                            let result=settlement::apply(state,(*review).clone(),choice).await;
                            if result.is_ok(){if let Ok(mut value)=prepared.try_write(){*value=None;}focus_after_apply(&opener,visit);}
                            if let Err(fault)=result {if let Ok(mut value)=error.try_write(){*value=Some(failure(fault));}}
                            if let Ok(mut value)=busy.try_write(){*value=false;}
                        });
                    }},if busy(){"Saving selected version…"}else{"Use selected version"}}
                }
            }
        }
        if let Some(message)=error(){p {class:"scheduled-hint",role:"alert","{message}"}}
    }
}
fn choices(review: &LocalReview) -> Vec<(LocalChoice, String, LocalDraft)> {
    let mut result = vec![(
        LocalChoice::Current,
        "Current local draft".into(),
        review.local.clone(),
    )];
    if let Some(blocked) = &review.blocked {
        result.push((
            LocalChoice::Blocked {
                token: blocked.token.clone(),
            },
            "Preserved local version".into(),
            blocked.draft.clone(),
        ));
    }
    for (index, version) in review.alternates.iter().enumerate() {
        result.push((
            LocalChoice::Alternate {
                writer_id: version.writer_id.clone(),
                revision: version.revision,
            },
            format!("Saved local version {}", index + 1),
            version.draft.clone(),
        ));
    }
    result
}
fn failure(fault: Fault) -> &'static str {
    match fault {
        Fault::Stale => {
            "These versions or your current draft changed. Reopen the review before choosing; all versions have been kept."
        }
        Fault::Pending => {
            "Local edits are still being saved. Reopen the review when saving finishes."
        }
        Fault::AlternateEdits | Fault::Unsupported => {
            "This selection cannot yet replace the managed draft, or newer local edits need review. All versions have been kept; reopen the review or keep the current local draft."
        }
        _ => {
            "Local versions could not be loaded or saved. All versions have been kept. Try the review again when storage is available."
        }
    }
}
fn focus(id: &str) {
    let id = serde_json::to_string(id).unwrap();
    spawn(async move {
        let _ = document::eval(&format!("document.getElementById({id})?.focus();")).await;
    });
}
fn focus_after_apply(id: &str, visit: u64) {
    let id = serde_json::to_string(id).unwrap();
    dioxus::core::spawn_forever(async move {
        let _ = document::eval(&format!(
            "requestAnimationFrame(() => requestAnimationFrame(() => {{ const panel = document.getElementById('managed-draft-recovery'); if (panel?.dataset.draftVisit !== '{visit}') return; const opener = document.getElementById({id}); const target = opener || document.getElementById('managed-draft-recovery-refresh'); if (target && !target.disabled) target.focus(); }}));"
        ))
        .await;
    });
}
