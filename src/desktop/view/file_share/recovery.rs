//! Discovery of retained outcomes without requiring the source attachment.
use super::*;

fn owner(shell: &ShellState) -> String {
    format!(
        "{:?}:{}:{:?}:{}",
        shell.core.active_account,
        shell.core.account_epoch,
        shell.core.active_team,
        shell.draft_writes.view_id
    )
}
#[component]
pub(crate) fn SavedFileShares(state: Signal<ShellState>) -> Element {
    let owner = owner(&state.read());
    rsx! {
        div {class:"saved-file-shares",
            button {id:"saved-file-shares-open","data-share-owner":owner,"data-file-share-owner":state.read().core.account_epoch,r#type:"button",class:"later-refresh",onclick:move |_|file_share::open_recovery(state),"Saved file shares"}
        }
    }
}
fn close_saved(
    mut state: Signal<ShellState>,
    serial: u64,
    generation: &str,
    expected_owner: &str,
    mut closing: Signal<Option<String>>,
) {
    if closing.peek().as_deref() == Some(generation)
        || owner(&state.peek()) != expected_owner
        || !file_share::recovery_visible(&state.peek()).is_some_and(|r| r.serial == serial)
    {
        return;
    }
    let generation = generation.to_owned();
    let expected_owner = expected_owner.to_owned();
    closing.set(Some(generation.clone()));
    dioxus::core::spawn_forever(async move {
        let script = include_str!("../../file_share/return_focus.js");
        let generation_js = serde_json::to_string(&generation).unwrap();
        let owner_js = serde_json::to_string(&expected_owner).unwrap();
        let mut eval = document::eval(&format!(
            "await(async()=>{{const cancel={script}('saved-file-shares-open',{owner_js},{generation_js},'recovery');dioxus.send(!!cancel);const disposition=await dioxus.recv();if(disposition!=='closed')cancel?.();}})();"
        ));
        let armed = tokio::time::timeout(std::time::Duration::from_secs(1), eval.recv::<bool>())
            .await
            .ok()
            .and_then(Result::ok)
            .unwrap_or(false);
        let closed = {
            let mut shell = state.write();
            if owner(&shell) == expected_owner
                && file_share::recovery_visible(&shell).is_some_and(|r| r.serial == serial)
            {
                file_share::close_recovery(&mut shell, serial);
                true
            } else {
                false
            }
        };
        let _ = eval.send(if closed && armed { "closed" } else { "cancel" });
        // Refresh or owner rotation can reuse this component. An old completion
        // must not release a newer visit's pending-close guard.
        if let Ok(mut pending) = closing.try_write()
            && pending.as_deref() == Some(generation.as_str())
        {
            *pending = None;
        }
    });
}
fn review(mut state: Signal<ShellState>, serial: u64, expected_owner: &str, attempt: &str) {
    let result = {
        let mut shell = state.write();
        if owner(&shell) != expected_owner {
            return;
        }
        file_share::open_receipt(&mut shell, serial, attempt)
    };
    if let Err(error) = result {
        state.write().show_toast(error.message());
    }
}
fn review_draft(
    mut state: Signal<ShellState>,
    serial: u64,
    expected_owner: &str,
    expected: &super_platinum_core::file_sharing::store::drafts::Draft,
) {
    let result = {
        let mut shell = state.write();
        if owner(&shell) != expected_owner {
            return;
        }
        file_share::open_saved_draft(&mut shell, serial, expected)
    };
    if let Err(error) = result {
        state.write().show_toast(error.message());
    }
}
fn review_capture(mut state: Signal<ShellState>, serial: u64, expected_owner: &str, job: u64) {
    let result = {
        let mut shell = state.write();
        if owner(&shell) != expected_owner {
            return;
        }
        file_share::open_captured_note(&mut shell, serial, job)
    };
    if let Err(error) = result {
        state.write().show_toast(error.message());
    }
}
#[component]
pub(crate) fn RecoveryDialog(state: Signal<ShellState>) -> Element {
    let closing = use_signal(|| None::<String>);
    let mut discard = use_signal(|| None::<String>);
    let snapshot = state.read();
    let Some(recovery) = file_share::recovery_visible(&snapshot) else {
        return rsx! {};
    };
    let retained = file_share::retained_direct_results(&snapshot)
        .into_iter()
        .filter(|held| {
            !recovery
                .direct_opens
                .iter()
                .any(|r| r.id() == held.record().id())
        })
        .collect::<Vec<_>>();
    let serial = recovery.serial;
    let owner = owner(&snapshot);
    let generation = format!("{owner}:{serial}");
    let close_pending = closing.read().as_deref() == Some(generation.as_str());
    let scrim_owner = owner.clone();
    let scrim_generation = generation.clone();
    let key_owner = owner.clone();
    let key_generation = generation.clone();
    let mount_generation = generation.clone();
    let review_owner = owner.clone();
    rsx! {
        div {class:"scrim",onclick:move |_|close_saved(state,serial,&scrim_generation,&scrim_owner,closing)}
        section {id:"saved-file-shares-dialog",class:"modal file-share-dialog",role:"dialog","aria-modal":"true","aria-labelledby":"saved-file-shares-title",
            "data-keyboard-dialog":"true","data-dialog-kind":"file-share-recovery","data-dialog-generation":generation.clone(),"data-escape-local":"true",
            onmounted:move |_|{document::eval(include_str!("keyboard.js"));frame("saved-file-shares-dialog",&mount_generation,&["#saved-file-shares-close"],false);},
            onkeydown:move |event|if event.key()==Key::Escape {event.prevent_default();event.stop_propagation();close_saved(state,serial,&key_generation,&key_owner,closing);},
            header {
                h2 {id:"saved-file-shares-title","Saved file shares"}
                button {id:"saved-file-shares-close",r#type:"button",class:"icon-btn","aria-label":"Close saved file shares",disabled:close_pending,onclick:move |_|close_saved(state,serial,&generation,&owner,closing),{icon(Icon::Close,"icon")}}
            }
            div {class:"file-share-body","aria-busy":recovery.loading,
                p {class:"file-share-hint","Review saved messages and file shares that need attention."}
                if recovery.loading {p {class:"file-share-hint",role:"status","Loading saved shares..."}}
                if let Some(error)=recovery.error {p {class:"file-share-error",role:"alert","{error}"}}
                if !recovery.loading&&recovery.error.is_none()&&recovery.receipts.is_empty()&&recovery.drafts.is_empty()&&recovery.captured.is_empty()&&recovery.direct_opens.is_empty()&&retained.is_empty() {p {class:"file-share-hint",role:"status","No saved messages or file shares need review."}}
                div {role:"list","aria-label":"DM results retained for this session",
                    for result in &retained {
                        {let result=result.clone();let expected=result.clone();let record=result.record();let id=record.id().to_owned();let draft=record.draft();let name=draft.content.destination.as_ref().map(|d|d.name.as_str()).unwrap_or("Person");let save_id=id.clone();let ask_id=format!("{review_owner}:{serial}:{id}");let confirmed=discard.read().as_deref()==Some(ask_id.as_str());rsx!{
                            div {class:"saved-file-share",role:"listitem",key:"retained-direct-{id}","data-retained-direct-open":id,
                                div {class:"saved-file-share-copy",strong {"{draft.content.title}"}p {"To {name}"}
                                    p {"A returned DM result is kept in this session. Its saved history may have changed or expired."}
                                    p {class:"file-share-error","This copy is not yet saved and will be lost when you quit. Check Slack before sharing again."}
                                    {direct_note(&snapshot,&draft.scope.team,&draft.content.note,state)}
                                }
                                button {disabled:close_pending,onclick:move |_|file_share::retry_direct_returned(state,save_id.clone()),"Save DM result"}
                                if confirmed {
                                    p {class:"file-share-error","Discard this session's retained copy? This does not change Slack or saved messages."}
                                    button {disabled:close_pending,onclick:move |_|{file_share::discard_direct_result(state,serial,expected.clone());discard.set(None);},"Discard this copy"}
                                    button {disabled:close_pending,onclick:move |_|discard.set(None),"Keep this copy"}
                                } else {
                                    button {disabled:close_pending,onclick:move |_|discard.set(Some(ask_id.clone())),"Discard retained result"}
                                }
                            }
                        }}
                    }
                }
                div {role:"list","aria-label":"Saved DM opening attempts",
                    for record in &recovery.direct_opens {
                        {let id=record.id().to_owned();let save_id=id.clone();let draft=record.draft();let name=draft.content.destination.as_ref().map(|d|d.name.as_str()).unwrap_or("Person");let status=if record.resolved().is_some(){"DM found. Your original message is retained below."}else{"DM opening result needs review. Review the saved message and Share to look up the DM without opening it again."};rsx!{
                            div {class:"saved-file-share",role:"listitem",key:"direct-{id}","data-direct-open":id.clone(),
                                div {class:"saved-file-share-copy",strong {"{draft.content.title}"}p {"To {name}"}p {"{status}"}
                                    {direct_note(&snapshot, &draft.scope.team, &draft.content.note, state)}
                                }
                                if file_share::direct_retry_available(&snapshot,&id) {button {disabled:close_pending,onclick:move |_|file_share::retry_direct_returned(state,save_id.clone()),"Save DM result"}}
                            }
                        }}
                    }
                }
                div {role:"list","aria-label":"Saved messages",
                    for draft in &recovery.drafts {
                        {let expected=draft.clone();let expected_owner=review_owner.clone();rsx!{
                            div {class:"saved-file-share saved-draft-row",role:"listitem",key:"draft-{draft.token}-{draft.revision}","data-share-draft":draft.scope.file.clone(),"data-draft-token":draft.token.clone(),"data-draft-revision":draft.revision,
                                div {class:"saved-file-share-copy",strong {"{draft.content.title}"}p {if let Some(destination)=&draft.content.destination {"To {destination.name}"}else{"No conversation selected"}}p {"Message saved on this computer"}}
                                button {r#type:"button",class:"later-refresh",disabled:close_pending,"aria-label":"Review saved message for {draft.content.title}",onclick:move |_|review_draft(state,serial,&expected_owner,&expected),"Review"}
                            }
                        }}
                    }
                }
                div {role:"list","aria-label":"Captured messages kept for this session",
                    for captured in &recovery.captured {
                        {let job=captured.id;let expected_owner=review_owner.clone();rsx!{
                            div {class:"saved-file-share captured-draft-row",role:"listitem",key:"capture-{job}","data-captured-draft":job,
                                div {class:"saved-file-share-copy",strong {"{captured.file_title}"}if let Some(name)=&captured.destination_name {p {"To {name}"}}p {"Captured copy kept for this session"}p {class:"file-share-error","This copy is not saved to disk and will be lost when you quit."}}
                                button {r#type:"button",class:"later-refresh",disabled:close_pending,"aria-label":"Review captured message for {captured.file_title}",onclick:move |_|review_capture(state,serial,&expected_owner,job),"Review"}
                            }
                        }}
                    }
                }
                div {role:"list","aria-label":"Saved file shares",
                    for receipt in &recovery.receipts {
                        {let attempt=receipt.attempt_id.clone();let expected_owner=review_owner.clone();let result=status::receipt(receipt,file_share::retained_status(&snapshot,&receipt.attempt_id));rsx!{
                            div {class:"saved-file-share",role:"listitem",key:"{attempt}","data-share-attempt":attempt.clone(),
                                div {class:"saved-file-share-copy",strong {"{receipt.file_title}"}p {"To {receipt.destination_name}"}p {"{result.title}"}if result.result_not_saved {p {class:"file-share-error","This result has not been saved. Check in Slack before sharing again."}}}
                                button {r#type:"button",class:"later-refresh",disabled:close_pending,"aria-label":"Review share of {receipt.file_title} to {receipt.destination_name}",onclick:move |_|review(state,serial,&expected_owner,&attempt),"Review"}
                            }
                        }}
                    }
                }
            }
            footer {button {id:"saved-file-shares-refresh",r#type:"button",disabled:recovery.loading||close_pending,onclick:move |_|file_share::open_recovery(state),"Refresh"}}
        }
    }
}

fn direct_note(
    shell: &ShellState,
    team: &str,
    note: &[serde_json::Value],
    state: Signal<ShellState>,
) -> Element {
    let Some(workspace) = shell.core.workspaces.get(team) else {
        return rsx! {};
    };
    let context = crate::blocks::BlockCtx::new(workspace, &shell.media);
    let nodes: Vec<_> = note
        .iter()
        .flat_map(|b| crate::blocks::block_nodes(context, b))
        .collect();
    rsx! {div {class:"message-body","aria-label":"Original saved message",for node in &nodes {{crate::view::rich::rich_node(node,shell.media_epoch,state)}}}}
}
