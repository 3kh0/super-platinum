use super::*;
use crate::slack_lists::rename::{self, Stage};
pub(super) fn action(
    state: Signal<ShellState>,
    column: &crate::slack_lists::ColumnView,
    visit: u64,
) -> Element {
    if !rename::supports_column(&state.read(), &column.id) {
        return rsx! {"{column.label}"};
    }
    let id = column.id.clone();
    let disabled = !rename::can_open(&state.read(), &column.id);
    rsx! {button{r#type:"button",class:"slack-list-action slack-list-rename-header",disabled,title:"{column.label}","data-rename-column":"{column.id}","aria-label":"Rename {column.label} field",
    onclick:move |_|rename::open(state,id.clone(),visit),span{"{column.label}"}{icon(Icon::Compose,"icon")}}}
}
pub(super) fn view_action(state: Signal<ShellState>, visit: u64) -> Element {
    let shell = state.read();
    if !rename::supports_view(&shell) {
        return rsx! {};
    };
    let Some(id) = rename::selected_view(&shell).map(str::to_owned) else {
        return rsx! {};
    };
    let selection = rename::selection(&shell).unwrap();
    let disabled = !rename::can_view(&shell);
    drop(shell);
    rsx! {button{id:"slack-list-rename-view",r#type:"button",class:"slack-list-action slack-list-rename-header",disabled,"data-rename-view":"{id}","aria-label":"Rename selected view",title:"Rename selected view",onclick:move |_|rename::open_view(state,visit,selection),{icon(Icon::Compose,"icon")}span{"Rename view"}}}
}
pub(super) fn create_action(state: Signal<ShellState>, visit: u64) -> Element {
    let selection = rename::selection(&state.read()).unwrap_or_default();
    let disabled = !rename::view_create::can_open(&state.read());
    rsx! {button{id:"slack-list-new-view",r#type:"button",title:"Requires a current writable List and verified available view capacity",class:"slack-list-action slack-list-rename-header",disabled,onclick:move |_|rename::view_create::open(state,visit,selection),{icon(Icon::Add,"icon")}span{"New view"}}}
}
pub(super) fn panel(state: Signal<ShellState>) -> Element {
    let Some(view) = rename::presentation(&state.read()) else {
        return rsx! {};
    };
    let serial = view.serial;
    let column = view.column;
    let saved_view = view.view.unwrap_or_default();
    let noun = view.noun;
    let creation = view.creation;
    let title = if creation.is_some() {
        "New view".to_owned()
    } else {
        format!("Rename {noun}")
    };
    let create_layout = creation.map(|l| l.as_str()).unwrap_or_default();
    let label = if noun == "view" {
        "View name"
    } else {
        "Field name"
    };
    let unchanged = if noun == "view" {
        view.name == view.before
    } else {
        view.name.trim() == view.before
    };
    let visit = crate::slack_lists::generation(&state.read()).unwrap_or_default();
    let editing = view.stage == Stage::Editing;
    let busy = matches!(
        view.stage,
        Stage::Preparing | Stage::Saving | Stage::Checking
    );
    let checkable = matches!(
        view.stage,
        Stage::Uncertain | Stage::Conflict | Stage::Available
    );
    let closable = matches!(
        view.stage,
        Stage::Editing | Stage::NotRequested | Stage::Observed
    ) || (matches!(view.stage, Stage::Conflict | Stage::Available)
        && view.current_review);
    let stage = format!("{:?}", view.stage);
    let current = view.current_fields;
    rsx! {
        div{class:"slack-list-editor-shade"}
        section{id:"slack-list-rename",class:"slack-list-cell-editor",role:"dialog","aria-modal":"true","aria-labelledby":"slack-list-rename-title",
            "data-keyboard-dialog":"true","data-rename-serial":"{serial}","data-rename-stage":"{stage}","data-rename-column":"{column}","data-rename-view":"{saved_view}","data-view-create":"{create_layout}",
            onmounted:move |_|{dioxus::document::eval(&format!("{}\n{}\n{}",include_str!("rename_restore.js"),include_str!("view_create_restore.js"),include_str!("rename_focus.js")));},
            h3{id:"slack-list-rename-title","{title}"}
            div{class:"slack-list-editor-body",
                if creation.is_none(){p{class:"slack-list-editor-item","Original name: {view.before}"}}
                if creation.is_some(){p{class:"slack-list-editor-hint","Views are shared with everyone who has access to this List. New views do not copy the selected view settings."}
                    label{r#for:"slack-list-new-view-layout","Layout"}
                    select{id:"slack-list-new-view-layout",value:"{create_layout}",disabled:!editing,onchange:move |event|{use super_platinum_core::slack_lists::ViewLayout;let layout=match event.value().as_str(){"table"=>ViewLayout::Table,"board"=>ViewLayout::Board,_=>return};rename::view_create::layout(state,serial,layout);},option{value:"table",selected:create_layout=="table","Table"}option{value:"board",selected:create_layout=="board","Board"}}
                }
                label{r#for:"slack-list-rename-name","{label}"}
                input{id:"slack-list-rename-name",r#type:"text",value:"{view.name}",maxlength:view.limit,disabled:!editing,
                    oninput:move |event|rename::input(state,serial,event.value())}
                p{id:"slack-list-rename-message",class:"slack-list-editor-hint",role:"status","{view.message}"}
                if matches!(view.stage,Stage::Observed|Stage::Conflict|Stage::Available){p{id:"slack-list-rename-current",class:"slack-list-editor-hint","Current fields: {current}"}}
                if view.blocked{p{class:"slack-list-warning","Slack asked us to wait. Checking and changes are temporarily unavailable."}}
            }
            div{class:"slack-list-editor-actions",
                button{id:"slack-list-rename-leave",r#type:"button",onclick:move |_|{let mut state=state;crate::slack_lists::close_shell(&mut state.write());},"Close list"}
                if closable{button{id:"slack-list-rename-close",r#type:"button",onclick:move |_|close(state,serial,visit),if editing{"Cancel"}else if matches!(view.stage,Stage::Conflict|Stage::Available){"Close review"}else{"Done"}}}
                if editing{button{id:"slack-list-rename-confirm",r#type:"button",class:"primary",disabled:!view.can_confirm||view.name.trim().is_empty()||unchanged,
                    onclick:move |_|rename::confirm(state,serial),if creation.is_some(){"Create view"}else{"Rename {noun}"}}}
                if checkable{button{id:"slack-list-rename-check",r#type:"button",class:"primary",disabled:!view.can_check,
                    onclick:move |_|rename::check(state,serial),if creation.is_some(){"Check current views"}else{"Check current {noun}"}}}
                if view.stage==Stage::Available {button{id:"slack-list-new-view-open",r#type:"button",class:"primary",disabled:!view.can_navigate,onclick:move |_|{if let Some(opened)=rename::view_create::open_available(state,serial){let proof=serde_json::to_string(&opened).unwrap();dioxus::document::eval(&format!("window.__listViewCreateReturn?.commit({proof})"));}},"Open view"}}
                if busy{span{role:"status","Please wait..."}}
            }
        }
    }
}

fn close(state: Signal<ShellState>, serial: u64, visit: u64) {
    if crate::slack_lists::generation(&state.peek()) != Some(visit) {
        return;
    }
    // Native click arms exact-origin restoration before this async DOM update.
    // A refused close leaves the panel mounted and never restores underneath it.
    rename::close(state, serial);
}
