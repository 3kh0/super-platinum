mod navigation;
mod recovery;
use crate::state::ShellState;
use dioxus::prelude::*;
use super_platinum_core::drafts::DraftKey;

#[component]
pub(super) fn LocalDraftRows(mut state: Signal<ShellState>) -> Element {
    let mut deleting = use_signal(|| None::<DraftKey>);
    let progress = use_signal(navigation::Progress::fixture);
    let shell = state.read();
    let Some(team) = shell.core.active_team.as_ref() else {
        return rsx! {};
    };
    let Some(workspace) = shell.core.workspaces.get(team) else {
        return rsx! {};
    };
    rsx! {
        recovery::RecoveryRows {state}
        for (key,draft) in shell.core.drafts.items.iter().filter(|(key,_)|&key.team==team) {
            {
                let identity=serde_json::to_string(key).unwrap();
                let managed=shell.core.active_account.as_deref().and_then(|account|shell.managed_drafts.entry(account,key));
                let status=managed.map(crate::drafts::session::status).unwrap_or("On this device");
                let name=super::scheduled::destination_label(&shell,&[serde_json::json!({"channel_id":key.channel,"thread_ts":key.thread})]);
                let resume_key=key.clone();let delete_key=key.clone();let confirm_key=key.clone();
                let opening=progress.read().loading && progress.read().key.as_ref()==Some(key);
                let error=progress.read().error.filter(|_|progress.read().key.as_ref()==Some(key));
                let nodes=crate::blocks::mrkdwn_blocks(crate::blocks::BlockCtx::new(workspace,&shell.media),&draft.composer.text);
                rsx! {
                    article {class:"scheduled-item",key:"{identity}","data-local-draft-key":identity.clone(),"data-draft-visit":shell.draft_writes.view_id,
                        div {class:"scheduled-meta",strong {"{name}"}span {class:"spacer"}span {"{status}"}}
                        div {class:"scheduled-content",for node in &nodes {{super::rich::rich_node(node,shell.media_epoch,state)}}}
                        if !draft.attachments.is_empty() {p {class:"scheduled-hint","{draft.attachments.len()} attached " if draft.attachments.len()==1 {"file"} else {"files"}}}
                        if let Some(error)=error {p {class:"scheduled-hint",role:"status","{error}"}}
                        div {class:"scheduled-actions",
                            if deleting.read().as_ref()==Some(key) {
                                span {"Delete this draft?"}
                                button {class:"danger",onclick:move |_|{state.write().discard_local_draft(&delete_key);deleting.set(None);},"Delete draft"}
                                button {onclick:move |_|deleting.set(None),"Cancel"}
                            } else {
                                button {class:"draft-resume",disabled:opening,"aria-busy":opening,onclick:move |_|navigation::resume(state,progress,resume_key.clone()),
                                    if opening {"Opening…"} else {"Continue writing"}
                                }
                                button {onclick:move |_|deleting.set(Some(confirm_key.clone())),"Delete draft"}
                            }
                        }
                    }
                }
            }
        }
    }
}
