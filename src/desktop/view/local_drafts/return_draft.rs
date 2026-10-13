//! A definite rejection may restore editing; it never authorizes another send.
use crate::{
    drafts::{Fault, recovery},
    state::ShellState,
};
use dioxus::prelude::*;
use super_platinum_core::drafts::{DraftKey, sync::ConsumeIntent};

#[component]
pub(super) fn ReturnDraft(
    state: Signal<ShellState>,
    account: String,
    epoch: u64,
    draft_key: DraftKey,
    revision: u64,
    intent: ConsumeIntent,
) -> Element {
    let mut busy = use_signal(|| false);
    let mut feedback = use_signal(|| None::<&'static str>);
    let visit = state.read().draft_writes.view_id;
    rsx! {
        div {class:"scheduled-actions",
            button {"data-return-rejected-send":intent.id.clone(),disabled:busy(),onclick:move |_|{
                if busy(){return;}busy.set(true);feedback.set(None);
                let account=account.clone();let key=draft_key.clone();let intent=intent.clone();
                dioxus::core::spawn_forever(async move {
                    let result=recovery::return_to_draft(state,account.clone(),epoch,key.clone(),revision,intent).await;
                    if result.is_ok() && state.try_peek().is_ok_and(|shell| shell.core.active_account.as_ref()==Some(&account) && shell.core.account_epoch==epoch && shell.draft_writes.view_id==visit) {
                        focus_local(&key,visit);
                    }
                    if let Err(fault)=result {if let Ok(mut value)=feedback.try_write(){*value=Some(failure(fault));}}
                    if let Ok(mut value)=busy.try_write(){*value=false;}
                });
            },if busy(){"Restoring draft…"}else{"Return to draft"}}
        }
        if let Some(message)=feedback(){p {class:"scheduled-hint",role:"status","{message}"}}
    }
}

fn failure(fault: Fault) -> &'static str {
    match fault {
        Fault::Stale | Fault::AlternateEdits => {
            "This draft or your current writing changed. Refresh Drafts to review both; nothing was overwritten or sent."
        }
        Fault::Pending => {
            "An edit or delivery change is still being saved. Try again when it finishes; nothing was sent."
        }
        Fault::Unsupported => {
            "This saved message cannot yet return to the composer without losing content. Its saved version is still here."
        }
        _ => {
            "The draft could not be restored. Its saved message is still here; try again when storage is available."
        }
    }
}

fn focus_local(key: &DraftKey, visit: u64) {
    let key = serde_json::to_string(&serde_json::to_string(key).unwrap()).unwrap();
    document::eval(&format!(
        "requestAnimationFrame(()=>requestAnimationFrame(()=>{{const row=[...document.querySelectorAll('[data-local-draft-key]')].find(row=>row.dataset.localDraftKey==={key}&&row.dataset.draftVisit==='{visit}');const button=row?.querySelector('.draft-resume');if(button&&!button.disabled){{button.focus({{preventScroll:true}});button.scrollIntoView({{block:'nearest'}});}}}}));"
    ));
}
