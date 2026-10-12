use crate::state::{ChannelOpen, ShellState};
use dioxus::prelude::*;
use super_platinum_core::drafts::DraftKey;

#[component]
pub(super) fn LocalDraftRows(mut state: Signal<ShellState>) -> Element {
    let mut deleting = use_signal(|| None::<DraftKey>);
    let shell = state.read();
    let Some(team) = shell.core.active_team.as_ref() else {
        return rsx! {};
    };
    let Some(workspace) = shell.core.workspaces.get(team) else {
        return rsx! {};
    };
    rsx! {
        for (key,draft) in shell.core.drafts.items.iter().filter(|(key,_)|&key.team==team) {
            {
                let identity=serde_json::to_string(key).unwrap();
                let name=super::scheduled::destination_label(&shell,&[serde_json::json!({"channel_id":key.channel,"thread_ts":key.thread})]);
                let resume_key=key.clone();let delete_key=key.clone();let confirm_key=key.clone();
                let nodes=crate::blocks::mrkdwn_blocks(crate::blocks::BlockCtx::new(workspace,&shell.media),&draft.composer.text);
                rsx! {
                    article {class:"scheduled-item",key:"{identity}",
                        div {class:"scheduled-meta",strong {"{name}"}span {class:"spacer"}span {"On this device"}}
                        div {class:"scheduled-content",for node in &nodes {{super::rich::rich_node(node,shell.media_epoch,state)}}}
                        if !draft.attachments.is_empty() {p {class:"scheduled-hint","{draft.attachments.len()} attached " if draft.attachments.len()==1 {"file"} else {"files"}}}
                        div {class:"scheduled-actions",
                            if deleting.read().as_ref()==Some(key) {
                                span {"Delete this draft?"}
                                button {class:"danger",onclick:move |_|{state.write().discard_local_draft(&delete_key);deleting.set(None);},"Delete draft"}
                                button {onclick:move |_|deleting.set(None),"Cancel"}
                            } else {
                                button {onclick:move |_|resume(state,resume_key.clone()),"Continue writing"}
                                button {onclick:move |_|deleting.set(Some(confirm_key.clone())),"Delete draft"}
                            }
                        }
                    }
                }
            }
        }
    }
}
fn resume(mut state: Signal<ShellState>, key: DraftKey) {
    {
        let mut shell = state.write();
        if shell.core.active_team.as_ref() != Some(&key.team) {
            return;
        }
        shell.checkpoint_composers();
        // Dormant conversations may not be projected in the sidebar yet.
        if !shell
            .channels
            .iter()
            .any(|channel| channel.id == key.channel)
        {
            if !shell
                .core
                .workspaces
                .get(&key.team)
                .is_some_and(|w| w.channels.contains_key(&key.channel))
            {
                shell.show_toast(
                    "This draft's conversation is unavailable. Its content is still saved.",
                );
                return;
            }
            shell.close_thread();
            shell.core.active_channel = Some(key.channel.clone());
            shell.refresh_from_core();
        }
        let Some(index) = shell
            .channels
            .iter()
            .position(|channel| channel.id == key.channel)
        else {
            return;
        };
        shell.select_channel(index, ChannelOpen::Global);
    }
    super::composer::focus_field_when_mounted(if key.thread.is_some() {
        "thread-composer"
    } else {
        "channel-composer"
    });
    let scope = {
        let shell = state.peek();
        (shell.core.active_account.clone(), shell.channel_generation)
    };
    dioxus::core::spawn_forever(async move {
        if {
            let shell = state.peek();
            (shell.core.active_account.clone(), shell.channel_generation) != scope
        } {
            return;
        }
        if let Some(root) = key.thread {
            crate::bootstrap::open_thread(state, key.channel, root).await;
        } else {
            crate::bootstrap::refresh_selected_channel(state).await;
        }
    });
}
