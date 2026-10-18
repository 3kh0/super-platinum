use crate::{
    channel_tabs as tabs,
    icons::{Icon, icon},
    state::ShellState,
};
use dioxus::prelude::*;
use super_platinum_core::{
    channel_tabs::{self, Access, Action, Inventory, Permission},
    files::FileKind,
};

#[component]
pub(super) fn CustomTabs(state: Signal<ShellState>) -> Element {
    let mut opened = use_signal(|| {
        matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("channel-tabs-canvas"
                | "channel-tabs-canvas-narrow"
                | "channel-tabs-manager"
                | "channel-tabs-manager-narrow"
                | "channel-tabs-enterprise"
                | "channel-tabs-enterprise-narrow"
                | "channel-tabs-knowledge"
                | "channel-tabs-knowledge-narrow"
                | "channel-tabs-folder-manage"
                | "channel-tabs-folder-manage-narrow")
        )
    });
    // Each rendered callback retains its own immutable owner snapshot. A new
    // render can offer a current visit; old callbacks never acquire that authority.
    let visit = tabs::Visit::capture(&state.read());
    let current = visit.as_ref().is_some_and(|v| v.current(&state.read()));
    let manage_visit = visit.clone();
    let inventory = tabs::inventory(&state.read());
    let selected = match tabs::selection::current(&state.read()) {
        Some(tabs::selection::Route::Canvas(route)) => Some(route.tab),
        Some(tabs::selection::Route::Folder(route)) => Some(route.tab().into()),
        _ => None,
    };
    let scope = use_hook(|| {
        (
            state.peek().core.account_epoch,
            state.peek().core.active_team.clone(),
            state.peek().core.active_channel.clone(),
            state.peek().channel_generation,
        )
    });
    if (
        state.read().core.account_epoch,
        state.read().core.active_team.clone(),
        state.read().core.active_channel.clone(),
        state.read().channel_generation,
    ) != scope
    {
        return rsx! {};
    }
    rsx! {
        if let Some(base)=inventory {
            for row in base.rows().iter().filter(|r|r["type"]=="folder"||r.get("id").and_then(|v|v.as_str()).is_some_and(|s|s.starts_with("Ct"))) {
                if let Some(id)=channel_tabs::id(row) {
                    {let base=base.clone();let id=id.to_owned();let allowed=current&&(base.canvas_record(&id).is_ok()||channel_tabs::folder::Folder::capture(&base,&id).is_ok());let label=tab_label(row);let visit=visit.clone();let active=selected.as_deref()==Some(id.as_str());rsx!{
                        button{class:if active{"active"}else{""},"aria-pressed":active,title:if allowed{"Open channel tab"}else{"This tab type is not available yet"},disabled:!allowed,onclick:move |_|if let Some(v)=visit.as_ref(){tabs::open_canvas(state,v,base.clone(),id.clone())},"{label}"}
                    }}
                }
            }
        }
        if let Some(v)=visit.clone() {
            {let back=tabs::selection::step(&state.read(),false);let forward=tabs::selection::step(&state.read(),true);let back_visit=v.clone();rsx!{
                button{r#type:"button",disabled:back.is_none(),"aria-label":"Previous channel tab",onclick:move |_|if let Some(step)=back.clone(){tabs::selection::history(state,&back_visit,step)},"Back"}
                button{r#type:"button",disabled:forward.is_none(),"aria-label":"Next channel tab",onclick:move |_|if let Some(step)=forward.clone(){tabs::selection::history(state,&v,step)},"Forward"}
            }}
        }
        button{id:"channel-tabs-manage",class:"icon-btn",title:"Manage tabs","aria-label":"Manage channel tabs",onclick:move |_|{opened.set(true);if let Some(v)=manage_visit.as_ref(){tabs::refresh(state,v);}},{icon(Icon::Add,"icon")}}
        if opened(){Manager{state,visit:visit.clone(),onclose:move |_|{opened.set(false);dioxus::document::eval("document.getElementById('channel-tabs-manage')?.focus();");}}}
    }
}
fn tab_label(row: &serde_json::Value) -> String {
    let label = channel_tabs::label(row);
    if label.is_empty() {
        row["type"].as_str().unwrap_or("Unavailable tab").into()
    } else {
        label.into()
    }
}
#[component]
fn Manager(
    state: Signal<ShellState>,
    visit: Option<tabs::Visit>,
    onclose: EventHandler,
) -> Element {
    let snapshot = state.read();
    let e = tabs::entry(&snapshot);
    let busy = e.is_some_and(|e| e.busy);
    let cooling = e.is_some_and(|e| e.cooling);
    let locked = e.is_some_and(|e| e.pending.is_some() || e.knowledge.is_some());
    let enabled = visit.as_ref().is_some_and(|v| v.current(&snapshot))
        && e.is_some_and(|e| e.permission == Some(Permission::Allowed))
        && !busy
        && !cooling
        && !locked;
    let finish_knowledge = e
        .and_then(|e| e.knowledge.as_ref())
        .is_some_and(|op| op.stage() == channel_tabs::KnowledgeStage::Disabled)
        && visit.as_ref().is_some_and(|v| v.current(&snapshot))
        && !busy
        && !cooling;
    let finish_visit = visit.clone();
    let notice = e.map(|e| e.notice.clone()).unwrap_or_default();
    let inventory = tabs::inventory(&snapshot);
    rsx! {
        div{class:"scrim",onclick:move |_|onclose.call(())}
        section{id:"channel-tabs-dialog",class:"modal channel-details channel-tabs-manager",role:"dialog","aria-modal":"true","aria-labelledby":"channel-tabs-heading",
            onmounted:move |_|super::channel_details::focus_dialog("channel-tabs-dialog"),
            onkeydown:move |e|if e.key()==Key::Escape{e.stop_propagation();onclose.call(());},
            header{h2{id:"channel-tabs-heading","Channel tabs"}button{r#type:"button","aria-label":"Close channel tabs",onclick:move |_|onclose.call(()),{icon(Icon::Close,"icon")}}}
            div{class:"channel-details-body",
                if busy{p{role:"status","Checking current tabs..."}}
                if !notice.is_empty(){p{class:"scheduled-hint",role:"status","{notice}"}}
                if !enabled&&!busy&&!locked&&!cooling{p{class:"scheduled-hint","Tab changes need current membership and permission information. Refresh to check."}}
                if let Some(base)=inventory {
                    for (index,row) in base.rows().iter().enumerate(){
                        if let Some(id)=channel_tabs::id(row){
                            {let id=id.to_owned();rsx!{TabRow{key:"{id}",state,visit:visit.clone(),base:base.clone(),id,index,enabled}}}
                        }
                    }
                    AddFolder{state,visit:visit.clone(),base:base.clone(),enabled}
                    AddCanvas{state,visit:visit.clone(),base,enabled}
                }else{p{"Tab information is not available yet."}}
            }
            footer{class:"scheduled-actions",
                if finish_knowledge{button{r#type:"button",onclick:move |_|if let Some(v)=finish_visit.as_ref(){tabs::knowledge::start(state,v,None,true)},"Finish removing knowledge tab"}}
                button{r#type:"button",disabled:busy||cooling,onclick:move |_|if let Some(v)=visit.as_ref(){tabs::refresh(state,v)},if locked{"Review current tabs"}else{"Refresh"}}
                button{r#type:"button",onclick:move |_|onclose.call(()),"Done"}
            }
        }
    }
}
#[component]
fn TabRow(
    state: Signal<ShellState>,
    visit: Option<tabs::Visit>,
    base: Inventory,
    id: String,
    index: usize,
    enabled: bool,
) -> Element {
    let row = base.row(&id).unwrap();
    let title = tab_label(row);
    let mut label = use_signal(|| channel_tabs::label(row).to_owned());
    let mut removing = use_signal(|| false);
    let is_folder =
        channel_tabs::folder::Folder::capture(&base, &id).is_ok() && id.starts_with("Ct");
    let allowed = enabled && (base.canvas(&id).is_ok() || is_folder);
    let rename_visit = visit.clone();
    let move_visit = visit.clone();
    let knowledge_visit = visit.clone();
    let remove_visit = visit;
    let rename_id = id.clone();
    let rename_base = base.clone();
    let move_id = id.clone();
    let move_base = base.clone();
    let after = index
        .checked_sub(2)
        .and_then(|i| base.rows().get(i))
        .and_then(channel_tabs::id)
        .map(str::to_owned);
    let movable = after.as_ref().is_none_or(|a| a.starts_with("Ct"));
    let remove_id = id.clone();
    let remove_base = base.clone();
    let removable = allowed
        && !is_folder
        && tabs::selection::can_mutate(&state.read(), &Action::Remove { id: id.clone() });
    rsx! {
        section{class:"channel-detail-field","data-channel-tab":id,
            h3{"{title}"}
            if base.canvas(&id).is_ok()||is_folder{
                label{r#for:"tab-label-{id}","Tab name"}
                input{id:"tab-label-{id}",maxlength:256,value:"{label}",disabled:!allowed,oninput:move |e|label.set(e.value())}
                div{class:"scheduled-actions",
                    button{r#type:"button",disabled:!allowed||label()==channel_tabs::label(row)||(is_folder&&label().trim().is_empty()),onclick:move |_|if let Some(v)=rename_visit.as_ref(){tabs::start(state,v,Some((rename_base.clone(),Action::Rename{id:rename_id.clone(),label:if is_folder{label().trim().to_owned()}else{label()}})))},"Rename"}
                    button{r#type:"button",disabled:!allowed||index==0||!movable,onclick:move |_|if let Some(v)=move_visit.as_ref(){tabs::start(state,v,Some((move_base.clone(),Action::Move{id:move_id.clone(),after:after.clone()})))},"Move earlier"}
                    if !is_folder{button{r#type:"button",disabled:!removable,onclick:move |_|removing.set(true),"Remove"}}
                }
                if removing(){
                    p{"Remove this tab? The Canvas and existing channel access will be kept."}
                    div{class:"scheduled-actions",
                        button{r#type:"button",disabled:!removable,onclick:move |_|{if let Some(v)=remove_visit.as_ref(){tabs::start(state,v,Some((remove_base.clone(),Action::Remove{id:remove_id.clone()})));}removing.set(false);},"Remove tab, keep access"}
                        button{r#type:"button",onclick:move |_|removing.set(false),"Cancel"}
                    }
                }
            }else if channel_tabs::KnowledgeRemoval::capture(base.clone(),&id).is_ok(){
                KnowledgeRow{state,visit:knowledge_visit.clone(),base:base.clone(),id:id.clone(),enabled}
            }else{p{class:"scheduled-hint","This tab is preserved. Its management controls are not available yet."}}
        }
    }
}
#[component]
fn AddCanvas(
    state: Signal<ShellState>,
    visit: Option<tabs::Visit>,
    base: Inventory,
    enabled: bool,
) -> Element {
    let mut selected = use_signal(String::new);
    let mut label = use_signal(String::new);
    let mut query = use_signal(String::new);
    let channel = base.channel().to_owned();
    let search_visit = visit.clone();
    let snapshot = state.read();
    let files = crate::files::visible_for(&snapshot, Some(&channel));
    let candidates = files
        .filter(|f| f.query.kind == FileKind::Canvases)
        .map(|f| f.items.clone())
        .unwrap_or_default();
    let valid = candidates.iter().any(|f| {
        f.id.as_deref() == Some(selected.read().as_str())
            && matches!(f.filetype.as_deref(), Some("quip" | "canvas"))
    });
    rsx! {
        section{class:"channel-detail-field",
            h3{"Add a Canvas tab"}
            form{onsubmit:move |e|{e.prevent_default();if enabled{if let Some(v)=search_visit.as_ref(){tabs::find_canvases(state,v,query());}}},
                label{r#for:"channel-tab-search","Find a Canvas shared in this channel"}
                input{id:"channel-tab-search",value:"{query}",disabled:!enabled,oninput:move |e|query.set(e.value())}
                button{r#type:"submit",disabled:!enabled,"Find"}
            }
            label{r#for:"channel-tab-file","Canvas"}
            select{id:"channel-tab-file",value:"{selected}",disabled:!enabled,onchange:move |e|selected.set(e.value()),
                option{value:"","Choose a Canvas"}
                for file in candidates.iter().filter(|f|matches!(f.filetype.as_deref(),Some("quip"|"canvas"))){
                    if let Some(id)=&file.id{option{value:"{id}","{super_platinum_core::state::file_title(file)}"}}
                }
            }
            label{r#for:"channel-tab-label","Tab name (optional)"}
            input{id:"channel-tab-label",maxlength:256,value:"{label}",disabled:!enabled,oninput:move |e|label.set(e.value())}
            p{class:"scheduled-hint","Adding this tab shares the Canvas with this channel with Can view access."}
            button{r#type:"button",class:"primary",disabled:!enabled||!valid,onclick:move |_|if let Some(v)=visit.as_ref(){tabs::start(state,v,Some((base.clone(),Action::AddCanvas{file:selected(),label:label(),access:Access::Read,after:None})))},"Share and add tab"}
        }
    }
}

#[component]
fn KnowledgeRow(
    state: Signal<ShellState>,
    visit: Option<tabs::Visit>,
    base: Inventory,
    id: String,
    enabled: bool,
) -> Element {
    let mut confirming = use_signal(|| false);
    let allowed =
        enabled && tabs::selection::can_mutate(&state.read(), &Action::Remove { id: id.clone() });
    rsx! {
        p{class:"scheduled-hint","Channel Knowledge Canvas"}
        button{r#type:"button",disabled:!allowed,onclick:move |_|confirming.set(true),"Remove"}
        if confirming(){
            p{"Removing this tab disables automatic nightly refresh for this channel. You can re-enable it later."}
            div{class:"scheduled-actions",
                button{r#type:"button",disabled:!allowed,onclick:move |_|{
                    if let (Some(v),Ok(op))=(visit.as_ref(),channel_tabs::KnowledgeRemoval::capture(base.clone(),&id)){tabs::knowledge::start(state,v,Some(op),false);}
                    confirming.set(false);
                },"Disable refresh and remove tab"}
                button{r#type:"button",onclick:move |_|confirming.set(false),"Cancel"}
            }
        }
    }
}

#[component]
fn AddFolder(
    state: Signal<ShellState>,
    visit: Option<tabs::Visit>,
    base: Inventory,
    enabled: bool,
) -> Element {
    let mut name = use_signal(String::new);
    let pending = tabs::entry(&state.read())
        .and_then(|e| e.pending.as_ref())
        .and_then(|i| match i.action() {
            Action::AddFolder { label } => Some(label.clone()),
            _ => None,
        });
    rsx! {
        section{class:"channel-detail-field",
            h3{"Add a folder tab"}
            if let Some(pending)=pending{p{role:"status","Creation of {pending} is retained for review. It will not be sent again."}}
            form{onsubmit:move |e|{e.prevent_default();if enabled{if let Some(v)=visit.as_ref(){tabs::start(state,v,Some((base.clone(),Action::AddFolder{label:name().trim().to_owned()})));}}},
                label{r#for:"channel-folder-name","Folder name"}
                input{id:"channel-folder-name",maxlength:256,value:"{name}",disabled:!enabled,oninput:move|e|name.set(e.value())}
                button{r#type:"submit",disabled:!enabled||name().trim().is_empty(),"Create folder"}
            }
        }
    }
}
