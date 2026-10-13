//! Saved-file controls shared by file menus and the Later recovery inventory.
use crate::{later::file_actions as controller, state::ShellState};
use dioxus::prelude::*;
use super_platinum_core::{
    later::{SavedAction, SavedKey, SavedMembership},
    saved_file_receipts::Status,
};

#[component]
pub(crate) fn SavedControl(
    state: Signal<ShellState>,
    target: controller::Target,
    #[props(default)] onsettled: EventHandler<()>,
) -> Element {
    let loaded = use_signal(|| None::<controller::View>);
    let mut busy = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);
    let mut retire_confirm = use_signal(|| false);
    let load_target = target.clone();
    use_future(move || {
        let target = load_target.clone();
        async move {
            let result = controller::load(&target).await;
            finish(state, &target, loaded, busy, error, result);
        }
    });
    let shell = state.read();
    if !target.current(&shell) {
        return rsx! {};
    }
    let saved = shell
        .core
        .later
        .matches_scope(shell.core.account_epoch, &target.team)
        && matches!(
            shell.core.later.membership(&SavedKey::file(&target.id)),
            SavedMembership::Present(_)
        );
    let view = loaded.read().clone();
    let uncertain = view
        .as_ref()
        .and_then(|view| view.receipt.as_ref())
        .filter(|receipt| receipt.status == Status::Uncertain);
    let notice = view.as_ref().and_then(|view| view.message);
    let change_target = target.clone();
    let key = target.id.clone();
    rsx! {
        div {class:"saved-file-control","data-saved-file-control":key,
            if busy() && view.is_none() {p {class:"file-action-note",role:"status","Checking saved file changes…"}}
            else if view.is_none() {
                button { disabled:busy(), onclick:{let target=target.clone(); move |_| {
                    if busy(){return;} busy.set(true);error.set(None);let target=target.clone();
                    dioxus::core::spawn_forever(async move {
                        let result=controller::load(&target).await;
                        finish(state,&target,loaded,busy,error,result);
                    });
                }}, "Retry loading saved file changes"}
            }
            else if let Some(receipt)=uncertain {
                p {class:"file-action-note",role:"status","This change to Later is unconfirmed. Check its status before making another change."}
                if receipt.retry_at > super_platinum_core::state::now_secs().max(0) as u64 {
                    p {class:"file-action-note","Please wait before checking again."}
                }
                if retire_confirm() {
                    p {class:"file-delete-description","Stop tracking this change? This does not undo it or confirm its outcome. Review this file in Later before making another change."}
                    div {class:"file-action-buttons",
                        button {disabled:busy(),onclick:move |_|retire_confirm.set(false),"Keep checking"}
                        button {disabled:busy(),onclick:{let target=target.clone();let id=receipt.operation_id.clone();move |_|{
                            if busy(){return;}busy.set(true);error.set(None);let target=target.clone();let id=id.clone();
                            dioxus::core::spawn_forever(async move {
                                let result=controller::retire(&target,id).await;
                                let settled = result.as_ref().is_ok_and(|view| view.receipt.as_ref().is_some_and(|r| r.status != Status::Uncertain));
                                finish(state,&target,loaded,busy,error,result);
                                if settled && state.try_peek().is_ok_and(|shell| target.current(&shell)) { onsettled.call(()); }
                                if let Ok(mut confirm)=retire_confirm.try_write(){*confirm=false;}
                            });
                        }},"Stop tracking"}
                    }
                } else {
                    div {class:"file-action-buttons",
                        button {disabled:busy(),onclick:{let target=target.clone();let id=receipt.operation_id.clone();move |_|{
                            if busy(){return;}busy.set(true);error.set(None);let target=target.clone();let id=id.clone();
                            dioxus::core::spawn_forever(async move {
                                let result=controller::check(state,target.clone(),id).await;
                                let settled = result.as_ref().is_ok_and(|view| view.receipt.as_ref().is_some_and(|r| r.status != Status::Uncertain));
                                finish(state,&target,loaded,busy,error,result);
                                if settled && state.try_peek().is_ok_and(|shell| target.current(&shell)) { onsettled.call(()); }
                            });
                        }},if busy(){"Checking…"}else{"Check saved status"}}
                        button {disabled:busy(),onclick:move |_|retire_confirm.set(true),"Stop tracking…"}
                    }
                }
            } else if view.as_ref().and_then(|v|v.receipt.as_ref()).is_some_and(|r| r.retry_at > super_platinum_core::state::now_secs().max(0) as u64) {
                p {class:"file-action-note",role:"status","Please wait before making another change."}
                button {disabled:busy(),onclick:{let target=target.clone();move |_|{
                    if busy(){return;}busy.set(true);error.set(None);let target=target.clone();
                    dioxus::core::spawn_forever(async move {
                        let result=controller::load(&target).await;
                        finish(state,&target,loaded,busy,error,result);
                        if state.try_peek().is_ok_and(|shell| target.current(&shell)) {onsettled.call(());}
                    });
                }},"Refresh availability"}
            } else {
                div {class:"file-action-buttons",
                    button {disabled:busy(),"data-saved-file-toggle":"true",onclick:move |_|{
                        if busy(){return;}busy.set(true);error.set(None);let target=change_target.clone();
                        dioxus::core::spawn_forever(async move {
                            let action=if saved{SavedAction::Remove}else{SavedAction::Save};
                            let result=controller::change(state,target.clone(),action).await;
                            let settled = result.as_ref().is_ok_and(|view| view.receipt.as_ref().is_some_and(|r| r.status != Status::Uncertain));
                                finish(state,&target,loaded,busy,error,result);
                                if settled && state.try_peek().is_ok_and(|shell| target.current(&shell)) { onsettled.call(()); }
                        });
                    },if busy(){"Saving change…"}else if saved{"Remove from Later"}else{"Save to Later"}}
                }
            }
            if let Some(message)=notice {p {class:"file-action-note",role:"status","{message}"}}
            if let Some(message)=error() {p {class:"file-action-error",role:"alert","{message}"}}
        }
    }
}

fn finish(
    state: Signal<ShellState>,
    target: &controller::Target,
    mut loaded: Signal<Option<controller::View>>,
    mut busy: Signal<bool>,
    mut error: Signal<Option<String>>,
    result: Result<controller::View, String>,
) {
    if !state.try_peek().is_ok_and(|shell| target.current(&shell)) {
        return;
    }
    match result {
        Ok(view) => {
            if let Ok(mut value) = loaded.try_write() {
                *value = Some(view);
            }
        }
        Err(message) => {
            if let Ok(mut value) = loaded.try_write() {
                *value = None;
            }
            if let Ok(mut value) = error.try_write() {
                *value = Some(message);
            }
        }
    }
    if let Ok(mut value) = busy.try_write() {
        *value = false;
    }
}

#[component]
pub(crate) fn RecoveryInventory(state: Signal<ShellState>) -> Element {
    let shell = state.read();
    let key = format!(
        "{}:{:?}:{}:{:?}",
        shell.draft_writes.view_id,
        shell.core.active_account,
        shell.core.account_epoch,
        shell.core.active_team
    );
    rsx! {ScopedInventory {key:"{key}",state}}
}

#[component]
fn ScopedInventory(state: Signal<ShellState>) -> Element {
    let entries = use_signal(Vec::<(controller::Target, controller::View)>::new);
    let loading = use_signal(|| true);
    let error = use_signal(|| None::<String>);
    let generation = use_signal(|| 0u64);
    let revision = use_memo(move || state.read().core.later.file_actions_revision);
    use_effect(move || {
        let _ = revision();
        dioxus::core::spawn_forever(refresh(state, entries, loading, error, generation));
    });
    let shell = state.read();
    let rows: Vec<_> = entries
        .read()
        .iter()
        .filter(|(target, view)| {
            target.current(&shell)
                && view.receipt.as_ref().is_some_and(|r| {
                    r.status == Status::Uncertain
                        || r.retry_at > super_platinum_core::state::now_secs().max(0) as u64
                })
        })
        .map(|(target, view)| {
            let receipt = view.receipt.as_ref().expect("recovery receipt");
            (
                target.clone(),
                format!(
                    "{}:{:?}:{}",
                    receipt.operation_id, receipt.status, receipt.retry_at
                ),
            )
        })
        .collect();
    if rows.is_empty() && error().is_none() {
        return rsx! {};
    }
    rsx! {
        section {class:"saved-file-recovery","aria-label":"Saved file changes","data-saved-file-recovery":"true",
            div {class:"scheduled-meta",strong {"Saved file changes"}span {class:"spacer"}
                button {disabled:loading(),onclick:move |_|{dioxus::core::spawn_forever(refresh(state,entries,loading,error,generation));},"Refresh changes"}
            }
            if let Some(message)=error() {p {class:"later-error",role:"alert","{message}"}}
            for (target,row_key) in rows {
                article {class:"later-item",key:"{row_key}","data-saved-file-change":target.id.clone(),
                    strong {"{target.title}"}
                    SavedControl {state,target:target.clone(),onsettled:move |_|{dioxus::core::spawn_forever(refresh(state,entries,loading,error,generation));}}
                }
            }
        }
    }
}

async fn refresh(
    state: Signal<ShellState>,
    mut entries: Signal<Vec<(controller::Target, controller::View)>>,
    mut loading: Signal<bool>,
    mut error: Signal<Option<String>>,
    mut generation: Signal<u64>,
) {
    let visit = {
        let shell = state.peek();
        (
            shell.draft_writes.view_id,
            shell.core.active_account.clone(),
            shell.core.active_team.clone(),
        )
    };
    generation += 1;
    let serial = *generation.peek();
    loading.set(true);
    error.set(None);
    let result = controller::inventory(state).await;
    if !state.try_peek().is_ok_and(|shell| {
        (
            shell.draft_writes.view_id,
            shell.core.active_account.clone(),
            shell.core.active_team.clone(),
        ) == visit
    }) || !generation
        .try_peek()
        .is_ok_and(|current| *current == serial)
    {
        return;
    }
    match result {
        Ok(rows) => {
            if let Ok(mut current) = entries.try_write() {
                *current = rows;
            }
        }
        Err(message) => {
            if let Ok(mut current) = error.try_write() {
                *current = Some(message);
            }
        }
    }
    if let Ok(mut current) = loading.try_write() {
        *current = false;
    }
}
