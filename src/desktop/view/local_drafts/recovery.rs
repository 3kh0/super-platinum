//! Saved consumption and conflict content stays visible independently of typing.
use crate::{
    drafts::{recovery, session},
    state::ShellState,
};
use dioxus::prelude::*;
use super_platinum_core::drafts::{DraftKey, sync::Operation};
#[path = "recovery_check.rs"]
mod check;
#[path = "rejected_send_check.rs"]
mod rejected_send_check;
#[path = "rejected_thread_check.rs"]
mod rejected_thread_check;
#[path = "return_draft.rs"]
mod return_draft;
#[path = "versions.rs"]
mod versions;

#[component]
pub(super) fn RecoveryRows(state: Signal<ShellState>) -> Element {
    let shell = state.read();
    let rows = session::recovery_rows(&shell);
    let mut review_rows = std::collections::BTreeMap::new();
    for row in &rows {
        if matches!(row.kind, "alternate" | "writer-alternate") {
            review_rows.entry(row.key.clone()).or_insert_with(|| {
                format!(
                    "{}:{}:{}",
                    row.kind,
                    serde_json::to_string(&row.key).unwrap(),
                    row.writer_id.as_deref().unwrap_or("")
                )
            });
        }
    }
    let managed = &shell.managed_drafts;
    let scoped =
        managed.account == shell.core.active_account && managed.epoch == shell.core.account_epoch;
    let loading = scoped && managed.loading;
    let error = scoped.then_some(managed.error).flatten();
    if rows.is_empty() && error.is_none() && !loading {
        return rsx! {};
    }
    rsx! {
        section { id:"managed-draft-recovery","data-draft-visit":shell.draft_writes.view_id,"aria-label":"Saved draft delivery and versions", onmounted:move |_|{spawn(check::run(state));dioxus::core::spawn_forever(rejected_send_check::run(state));dioxus::core::spawn_forever(rejected_thread_check::run(state));},
            div {class:"scheduled-meta",strong {"Saved draft status"}span {class:"spacer"}
                button {id:"managed-draft-recovery-refresh",disabled:loading,onclick:move |_|{spawn(session::load(state));},"Refresh"}
            }
            if loading {p {class:"scheduled-hint",role:"status","Checking saved draft status…"}}
            if let Some(error)=error {p {class:"scheduled-hint",role:"alert","{error}"}}
            for row in rows {
                {
                    let identity=format!("{}:{}:{}",row.kind,serde_json::to_string(&row.key).unwrap(),row.writer_id.as_deref().unwrap_or(""));
                    let name=super::super::scheduled::destination_label(&shell,&[serde_json::json!({"channel_id":row.key.channel,"thread_ts":row.key.thread})]);
                    let nodes=shell.core.workspaces.get(&row.key.team).map(|workspace|{
                        let ctx=crate::blocks::BlockCtx::new(workspace,&shell.media);
                        if let Some(snapshot)=&row.snapshot {
                            snapshot.blocks.iter().flat_map(|block|crate::blocks::block_nodes(ctx,block)).collect::<Vec<_>>()
                        } else if let Some(local)=&row.local {
                            crate::blocks::mrkdwn_blocks(ctx,&local.composer.text)
                        } else {vec![]}
                    }).unwrap_or_default();
                    let pending = (row.kind == "delivery").then(|| shell.core.active_account.as_ref().and_then(|account| shell.managed_drafts.entry(account, &row.key)).and_then(recovery::operation)).flatten();
                    let rejected = (row.kind == "delivery").then(|| shell.core.active_account.as_ref().and_then(|account| shell.managed_drafts.entry(account, &row.key)).and_then(|view| recovery::rejected(view).zip(view.stored.as_ref().map(|stored| stored.revision)))).flatten();
                    let local_revision = (row.kind == "local").then(|| shell.core.active_account.as_ref().and_then(|account| shell.managed_drafts.entry(account, &row.key)).and_then(|view| view.stored.as_ref()).map(|stored| stored.revision)).flatten();
                    let files=row.snapshot.as_ref().map(|snapshot|snapshot.file_ids.len()).or_else(||row.local.as_ref().map(|local|local.attachments.len())).unwrap_or(0);
                    rsx! {
                        article {class:"scheduled-item",key:"{identity}","data-managed-draft-recovery":row.kind,
                            div {class:"scheduled-meta",strong {"{name}"}span {class:"spacer"}span {"{row.status}"}}
                            div {class:"scheduled-content",for node in &nodes {{super::super::rich::rich_node(node,shell.media_epoch,state)}}}
                            if nodes.is_empty() {p {class:"scheduled-hint","The saved version has no visible text."}}
                            if files>0 {p {class:"scheduled-hint","{files} attached " if files==1 {"file"} else {"files"}}}
                            p {class:"scheduled-hint",role:"status","{row.explanation}"}
                            if review_rows.get(&row.key)==Some(&identity) {
                                versions::Versions {key:"{shell.draft_writes.view_id}:{shell.core.active_account:?}:{identity}",state,account:shell.core.active_account.clone().unwrap_or_default(),epoch:shell.core.account_epoch,draft_key:row.key.clone()}
                            }
                            if let Some(operation) = pending {
                                DeliveryCheck { key:"{operation.id}", state, operation }
                            }
                            if let Some((intent,revision)) = rejected {
                                return_draft::ReturnDraft {key:"{shell.draft_writes.view_id}:{shell.core.active_account:?}:{shell.core.account_epoch}:{intent.id}:{revision}",state,account:shell.core.active_account.clone().unwrap_or_default(),epoch:shell.core.account_epoch,draft_key:row.key.clone(),revision,intent}
                            }
                            if let Some(revision) = local_revision {
                                ContinueLocal { state, account:shell.core.active_account.clone().unwrap_or_default(), epoch:shell.core.account_epoch, draft_key:row.key.clone(), revision }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn DeliveryCheck(state: Signal<ShellState>, operation: Operation) -> Element {
    let mut busy = use_signal(|| false);
    let mut feedback = use_signal(|| None::<&'static str>);
    rsx! {
        div { class:"scheduled-actions",
            button { disabled:busy(), onclick:move |_| {
                if busy() { return; }
                busy.set(true);
                feedback.set(None);
                let operation = operation.clone();
                dioxus::core::spawn_forever(async move {
                    let message = match recovery::check(state, operation).await {
                        Ok(message) => message,
                        Err(recovery::CheckError::Cooldown(_)) => "Please wait before checking delivery again. The saved cooldown is still active.",
                        Err(recovery::CheckError::Fault(crate::drafts::Fault::Unsupported)) => "Connect to Slack before checking delivery.",
                        Err(recovery::CheckError::Fault(crate::drafts::Fault::Stale)) => "Saved delivery state changed. Refresh Drafts to review it.",
                        Err(_) => "Delivery status could not be saved. Check delivery is paused until storage is available; nothing was sent again.",
                    };
                    if let Ok(mut value) = feedback.try_write() { *value = Some(message); }
                    if let Ok(mut value) = busy.try_write() { *value = false; }
                });
            }, if busy() { "Checking…" } else { "Check delivery" } }
        }
        if let Some(message) = feedback() { p {class:"scheduled-hint",role:"status","{message}"} }
    }
}

#[component]
fn ContinueLocal(
    state: Signal<ShellState>,
    account: String,
    epoch: u64,
    draft_key: DraftKey,
    revision: u64,
) -> Element {
    let mut busy = use_signal(|| false);
    let mut error = use_signal(|| false);
    rsx! {
        div { class:"scheduled-actions",
            button { disabled:busy(), onclick:move |_| {
                if busy() { return; }
                busy.set(true);
                error.set(false);
                let key = draft_key.clone();
                let account = account.clone();
                dioxus::core::spawn_forever(async move {
                    let failed = recovery::continue_locally(state, account, epoch, key, revision).await.is_err();
                    if let Ok(mut value) = error.try_write() { *value = failed; }
                    if let Ok(mut value) = busy.try_write() { *value = false; }
                });
            }, if busy() { "Preparing local editing…" } else { "Continue editing locally" } }
        }
        if error() { p {class:"scheduled-hint",role:"status","This draft could not return to local editing. Refresh to review its saved state; nothing was sent."} }
    }
}
