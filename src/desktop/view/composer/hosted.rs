//! Single selection + explicit Insert, repeatable to build the composer set.
use super::*;
use crate::drafts::protected::production::hosted as runtime;
use crate::{
    file_share::composer::{Choice, Page, Picker},
    icons::{Icon, icon},
};
use std::rc::Rc;
use super_platinum_core::slack::api::HostedFileKind;

#[derive(Clone, Default)]
struct Browse {
    serial: u64,
    owner: Option<Rc<Picker>>,
    loading: bool,
    inserting: bool,
    error: Option<&'static str>,
    page: Option<Page>,
    choice: Option<Choice>,
}
fn current_request<T>(
    owner: Option<&Rc<T>>,
    serial: u64,
    expected: &Rc<T>,
    requested: u64,
) -> bool {
    serial == requested && owner.is_some_and(|owner| Rc::ptr_eq(owner, expected))
}
fn search(
    state: Signal<ShellState>,
    picker: Rc<Picker>,
    mut browse: Signal<Browse>,
    text: String,
    kind: HostedFileKind,
    page: u32,
) {
    let serial = {
        let mut b = browse.write();
        let Some(serial) = b.serial.checked_add(1) else {
            return;
        };
        if !b
            .owner
            .as_ref()
            .is_some_and(|owner| Rc::ptr_eq(owner, &picker))
        {
            return;
        }
        b.serial = serial;
        b.loading = true;
        b.choice = None;
        b.error = None;
        serial
    };
    spawn(async move {
        let result = picker.search(state, &text, kind, page).await;
        let Ok(mut b) = browse.try_write() else {
            return;
        };
        if !current_request(b.owner.as_ref(), b.serial, &picker, serial) {
            return;
        }
        b.loading = false;
        match result {
            Ok(page) => b.page = Some(page),
            Err(_) => {
                b.page = None;
                b.error = Some("Files could not be loaded. Try again.");
            }
        }
    });
}
fn restore_focus() {
    document::eval(
        "requestAnimationFrame(()=>document.getElementById('hosted-attach-trigger')?.focus({preventScroll:true}));",
    );
}
#[component]
pub(super) fn AttachmentControl(mut state: Signal<ShellState>) -> Element {
    let mut menu = use_signal(|| false);
    let mut picker = use_signal(|| None::<Rc<Picker>>);
    let mut browse = use_signal(Browse::default);
    let mut query = use_signal(String::new);
    let mut kind = use_signal(HostedFileKind::default);
    let current = picker.read().clone();
    let picker_open = current.is_some();
    let shown = browse.read().clone();
    let has_files = runtime::present(&state.read(), ComposerTarget::Channel);
    rsx! {
        button {id:"hosted-attach-trigger",class:"composer-icon-btn",title:"Attach files","aria-label":"Attach files","aria-expanded":menu() || picker_open,
            onclick:move |_|menu.set(!menu()),{icon(Icon::Add,"icon sm")}}
        if menu() {
            div {class:"hosted-attach-menu",role:"menu","aria-label":"Attach files",
                button {r#type:"button",role:"menuitem",disabled:has_files,onclick:move |_|{menu.set(false);spawn(super::choose_attachments(state,ComposerTarget::Channel));},"Upload from your computer"}
                button {r#type:"button",role:"menuitem",disabled:!state.read().core.composer_attachments.is_empty(),onclick:move |_|{
                    menu.set(false);
                    let captured = {
                        let shell = state.peek();
                        Picker::capture(&shell)
                    };
                    match captured {
                        Ok(value)=>{let value=Rc::new(value);picker.set(Some(value.clone()));query.set(String::new());kind.set(HostedFileKind::All);browse.set(Browse {owner:Some(value.clone()),..Browse::default()});search(state,value,browse,String::new(),HostedFileKind::All,1);}
                        Err(_)=>state.write().show_toast("Reconnect to this conversation before attaching a Slack file."),
                    }
                },"Attach from Slack"}
            }
        }
        if let Some(current)=current {
            div {class:"scrim",onclick:move |_|{if !browse.peek().inserting {browse.set(Browse::default());picker.set(None);restore_focus();}}}
            section {id:"hosted-file-picker",class:"modal file-share-dialog hosted-file-picker",role:"dialog","aria-modal":"true","aria-labelledby":"hosted-picker-title",
                "data-keyboard-dialog":"true","data-dialog-generation":"hosted-picker","data-escape-local":"true",
                onmounted:move |_|{document::eval(include_str!("hosted_keyboard.js"));},
                onkeydown:move |event|if event.key()==Key::Escape {event.prevent_default();event.stop_propagation();if !browse.peek().inserting{browse.set(Browse::default());picker.set(None);restore_focus();}},
                header {h2 {id:"hosted-picker-title","Attach a file"}button {class:"icon-btn","aria-label":"Close",disabled:shown.inserting,onclick:move |_|{browse.set(Browse::default());picker.set(None);restore_focus();},{icon(Icon::Close,"icon")}}}
                div {class:"file-share-body",
                    div {class:"hosted-picker-types",role:"group","aria-label":"File type",
                        for option in HostedFileKind::ALL {
                            {let current=current.clone();rsx!{button {r#type:"button","aria-pressed":kind()==option,disabled:shown.inserting,onclick:move |_|{kind.set(option);search(state,current.clone(),browse,query.peek().clone(),option,1);},"{option.label()}"}}}
                        }
                    }
                    label {r#for:"hosted-picker-query","Search files"}
                    input {id:"hosted-picker-query",value:query(),autocomplete:"off",disabled:shown.inserting,
                        oninput:move |event|query.set(event.value()),
                        onkeydown:{let current=current.clone();move |event|if event.key()==Key::Enter {event.prevent_default();event.stop_propagation();search(state,current.clone(),browse,query.peek().clone(),kind(),1);}}}
                    button {r#type:"button",disabled:shown.inserting,onclick:{let current=current.clone();move |_|search(state,current.clone(),browse,query.peek().clone(),kind(),1)},"Search"}
                    if shown.loading {p {role:"status","Loading files…"}}
                    if let Some(error)=shown.error {p {class:"file-share-error",role:"alert","{error}"}}
                    if let Some(page)=shown.page {
                        div {class:"hosted-picker-results",role:"listbox","aria-label":"Files",
                            for (index,item) in page.items.iter().enumerate() {
                                {let title=item["title"].as_str().or_else(||item["name"].as_str()).unwrap_or("File").to_owned();let id=item["id"].as_str().unwrap_or_default().to_owned();let selected=shown.choice.as_ref().is_some_and(|c|c.id()==id);let choice=page.choose(index);rsx!{
                                    button {class:if selected{"file-share-option selected"}else{"file-share-option"},role:"option","aria-selected":selected,disabled:shown.loading||shown.inserting,
                                        onclick:move |_|browse.write().choice=choice.clone(),{icon(Icon::AttachFile,"icon sm")}span {"{title}"}}
                                }}
                            }
                        }
                        if page.items.is_empty() && !shown.loading {p {role:"status","No files found."}}
                        div {class:"hosted-picker-pages",
                            button {disabled:page.page<=1||shown.loading||shown.inserting,onclick:{let current=current.clone();move |_|search(state,current.clone(),browse,query.peek().clone(),kind(),page.page.saturating_sub(1))},"Previous"}
                            span {"Page {page.page}"}
                            button {disabled:!page.more||shown.loading||shown.inserting,onclick:{let current=current.clone();move |_|search(state,current.clone(),browse,query.peek().clone(),kind(),page.page+1)},"Next"}
                        }
                    }
                }
                footer {
                    button {disabled:shown.inserting,onclick:move |_|{browse.set(Browse::default());picker.set(None);restore_focus();},"Cancel"}
                    button {class:"primary",disabled:shown.loading||shown.inserting||shown.choice.is_none(),onclick:move |_|{
                        let Some(choice)=browse.peek().choice.clone() else{return};
                        browse.write().inserting=true;
                        let current=current.clone();
                        dioxus::core::spawn_forever(async move {
                            let result=current.insert(state,choice).await;
                            // The root-owned operation survives this view. UI is
                            // optional after the actual actor receives its set.
                            if let Ok(mut b)=browse.try_write(){if !b.owner.as_ref().is_some_and(|owner|Rc::ptr_eq(owner,&current)){return;}b.inserting=false;if result.is_err(){b.error=Some("The file is retained if inserted. Review the saved attachment before trying again.");}}
                            if result.is_ok(){if let Ok(mut p)=picker.try_write(){if p.as_ref().is_some_and(|value|Rc::ptr_eq(value,&current)){*p=None;if let Ok(mut b)=browse.try_write(){*b=Browse::default();}restore_focus();}}}
                            if let Ok(mut shell)=state.try_write(){shell.refresh_from_core();}
                        });
                    },if shown.inserting {"Inserting…"}else{"Insert"}}
                }
            }
        }
    }
}

#[component]
pub(super) fn HostedChips(mut state: Signal<ShellState>) -> Element {
    let mut confirmed = use_signal(|| None::<(String, String, u64)>);
    let owner = runtime::review_owner(&state.read());
    let snapshot = runtime::snapshot(&state.read(), ComposerTarget::Channel);
    let Some(snapshot) = snapshot else {
        return rsx! {};
    };
    let ids = snapshot.selection.ids();
    rsx! {div {class:"hosted-composer-files","aria-label":"Attached Slack files",
        for (index,raw) in snapshot.selection.sources().iter().enumerate() {
            {let id=ids[index].clone();let title=raw["title"].as_str().or_else(||raw["name"].as_str()).unwrap_or("File").to_owned();let reviewed_remove=snapshot.clone();let reviewed_order=snapshot.clone();let mut order=ids.clone();if index>0{order.swap(index,index-1);}rsx!{
                div {class:"hosted-composer-file",{icon(Icon::AttachFile,"icon sm")}span {"{title}"}
                    button {disabled:snapshot.sending||index==0,"aria-label":format!("Move {title} earlier"),onclick:move |_|{let order=order.clone();let reviewed=reviewed_order.clone();dioxus::core::spawn_forever(async move{let _=runtime::edit_reviewed(state,reviewed,|c,v|c.reorder(v,&order)).await;state.write().refresh_from_core();});},"Move earlier"}
                    button {class:"icon-btn",disabled:snapshot.sending,"aria-label":format!("Remove {title}"),onclick:move |_|{let id=id.clone();let reviewed=reviewed_remove.clone();dioxus::core::spawn_forever(async move{let _=runtime::edit_reviewed(state,reviewed,|c,v|c.remove(v,&id)).await;state.write().refresh_from_core();});},{icon(Icon::Close,"icon sm")}}
                }
            }}
        }
        for receipt in &snapshot.completed {
            {let id=receipt.record().id().to_owned();let token=(owner.clone(),id.clone(),snapshot.version);let reviewed=confirmed.read().as_ref()==Some(&token);let expected=snapshot.clone();let owner=owner.clone();let names=receipt.record().input().sources().iter().map(|r|r["title"].as_str().or_else(||r["name"].as_str()).unwrap_or("File")).collect::<Vec<_>>().join(", ");rsx!{
                div {class:"file-share-review","aria-label":"Accepted file share",
                    p {"Slack accepted this share: {names}"}
                    {accepted_note(&state.read(),receipt.record().scope().key.team.as_str(),&receipt.record().input().snapshot().blocks,state)}
                    p {"Remove this local delivery record and any unchanged sent attachments. Your current message stays in the composer."}
                    if reviewed {
                        button {disabled:!snapshot.can_retire,onclick:move |_|{confirmed.set(None);runtime::retire_completed(state,expected.clone(),id.clone(),owner.clone());},"Remove local share record"}
                        button {onclick:move |_|confirmed.set(None),"Keep record"}
                    } else {
                        button {disabled:!snapshot.can_retire,onclick:move |_|confirmed.set(Some(token.clone())),"Review local removal"}
                    }
                }
            }}
        }
        if snapshot.retry_retirement {button {onclick:move |_|runtime::retry_retirement(state),"Save record removal"}}
        if snapshot.sending && snapshot.completed.is_empty() {p {class:"file-share-hint",role:"status","This file share has a saved delivery record. Check Slack before sharing again."}}
        if snapshot.retry_load {button {onclick:move |_|runtime::retry_load(state),"Load saved attachments"}}
        if snapshot.retry_result {button {onclick:move |_|runtime::retry_storage(state,true),"Save returned result"}}
        if snapshot.retry_saved {button {onclick:move |_|runtime::retry_storage(state,false),"Save attachment changes"}}
        else if snapshot.loaded && !snapshot.saved {p {class:"file-share-hint",role:"status","Attachment changes are retained on this computer and still need to be saved."}}
    }}
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn late_result_after_close_and_reopen_cannot_adopt_same_serial() {
        let first = Rc::new(());
        let reopened = Rc::new(());
        assert!(current_request(Some(&first), 1, &first, 1));
        assert!(!current_request::<()>(None, 0, &first, 1));
        assert!(!current_request(Some(&reopened), 1, &first, 1));
        assert!(current_request(Some(&reopened), 1, &reopened, 1));
        assert!(!current_request(Some(&reopened), 2, &reopened, 1));
    }
}

fn accepted_note(
    shell: &ShellState,
    team: &str,
    blocks: &[serde_json::Value],
    state: Signal<ShellState>,
) -> Element {
    let Some(workspace) = shell.core.workspaces.get(team) else {
        return rsx! {};
    };
    let context = crate::blocks::BlockCtx::new(workspace, &shell.media);
    let nodes: Vec<_> = blocks
        .iter()
        .flat_map(|b| crate::blocks::block_nodes(context, b))
        .collect();
    rsx! {div {class:"message-body","aria-label":"Accepted message",for node in &nodes {{crate::view::rich::rich_node(node,shell.media_epoch,state)}}}}
}
