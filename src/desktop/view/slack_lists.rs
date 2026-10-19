//! Cached List layouts with bounded native rows and canonical cell editing.
use crate::{
    icons::{Icon, icon},
    state::ShellState,
};
use dioxus::prelude::*;
mod board;
mod board_move;
pub(crate) mod comments;
mod create;
mod delete;
mod editor;
mod filter;
mod find;
mod grouping;
mod people;
#[cfg(test)]
mod picker_tests;
mod precision;
mod rating;
mod record;
mod refinement;
mod reminder;
mod rename;
mod row;
mod sort;
mod subitem;
mod visibility;
mod vote;
const PAGE_SIZE: usize = 50;
const KEYBOARD_HELP: &str = "Arrow keys move between cells; Right and Left expand or collapse items and groups. Space changes a checkbox. Alt+Enter opens item details. F2 edits a cell. Enter opens cell controls; Escape returns to the cell.";
pub(crate) fn dialog(state: Signal<ShellState>) -> Element {
    let Some(generation) = crate::slack_lists::generation(&state.read()) else {
        return rsx! {};
    };
    rsx! {ListDialog{key:"slack-list-{generation}",state}}
}
#[component]
fn ListDialog(mut state: Signal<ShellState>) -> Element {
    // Page state is scoped to a selected layout, without remounting the editor.
    let mut page = use_signal(|| ((None::<String>, 0u64, 0u64), 0usize));
    let Some(view) = crate::slack_lists::presentation(&state.read()) else {
        return rsx! {};
    };
    let deletion = crate::slack_lists::delete::presentation(&state.read()).operation;
    let deletion_unresolved = deletion.is_some();
    let deleting = deletion.as_ref().is_some_and(|operation| operation.pending);
    let detail_generation = view.detail.as_ref().map(|detail| detail.generation);
    let detail_open = view.detail.as_ref().map(|detail| detail.open_generation);
    let comments_generation =
        crate::slack_lists::comments::presentation(&state.read()).map(|view| view.generation);
    let comments_open = comments_generation.is_some();
    let is_board = view.layout_kind == crate::slack_lists::ProjectionLayout::Board;
    let count = view.entries.len();
    let page_count = count.div_ceil(PAGE_SIZE).max(1);
    let page_key = (
        view.selected_view.clone(),
        view.find_revision,
        view.grouping_revision,
    );
    let current_page = if page.read().0 == page_key {
        page.read().1.min(page_count - 1)
    } else {
        0
    };
    let start = current_page * PAGE_SIZE;
    let end = (start + PAGE_SIZE).min(count);
    let first = if count == 0 { 0 } else { start + 1 };
    let page_label = current_page + 1;
    let row_count = count + 1;
    let column_count = view.columns.len();
    let media_epoch = state.read().media_epoch;
    let generation = view.generation;
    let layout = view.layout_generation;
    let selection_generation =
        crate::slack_lists::rename::selection(&state.read()).unwrap_or_default();
    let selected = view.selected_view.clone().unwrap_or_default();
    let page_previous = page_key.clone();
    let page_next = page_key;
    let shown = view.rows.len();
    let item_label = if view.matched_rows == 1 {
        "item"
    } else {
        "items"
    };
    let page_status = if is_board {
        format!(
            "{} {item_label} · {} loaded",
            view.matched_rows, view.total_rows
        )
    } else if view.selected_view.is_some() {
        format!(
            "{} {item_label} · {} loaded · Page {page_label} of {page_count}",
            view.matched_rows, view.total_rows
        )
    } else {
        format!(
            "Items {first}–{end} of {shown} shown · {} loaded · Page {page_label} of {page_count}",
            view.total_rows
        )
    };
    // Return keyed row VNodes directly. An RSX for/match wrapper adds an
    // unkeyed fragment per entry and would recreate cells when rows reorder.
    let rendered_rows = if is_board {
        Vec::new()
    } else {
        view.entries[start..end].iter().enumerate().map(|(index, entry)| {
                            match entry {
                                crate::slack_lists::TableEntry::Row(item)=>row::row(state,&view,item,start+index,index==0,media_epoch),
                                crate::slack_lists::TableEntry::Group(group)=>{
                                    let key=group.key.clone();let instance=serde_json::json!(["group",group.key]).to_string();
                                    rsx!{tr{key:"group-{group.key}",class:"slack-list-group","data-group-key":"{group.key}","data-instance-id":"{instance}","aria-rowindex":start+index+2,
                                        td{colspan:column_count,role:"rowheader","data-column-id":"__group",tabindex:if index==0{"0"}else{"-1"},
                                            button{r#type:"button",class:"slack-list-group-toggle",tabindex:"-1","aria-expanded":!group.collapsed,
                                                onclick:move |_|crate::slack_lists::toggle_group(state,key.clone(),generation,layout),
                                                {icon(Icon::ChevronRight,"icon")}span{"{group.label}"}span{class:"slack-list-group-count","{group.count}"}
                                            }
                                        }
                                    }}
                                }
                            }
    }).collect::<Vec<Element>>()
    };
    rsx! {
        div{class:"scrim",onclick:move |_|comments::close_list(state,generation)}
        section{id:"shell-overlay-dialog",class:"modal slack-list-dialog",role:"dialog","aria-modal":"true","aria-labelledby":"slack-list-title",tabindex:"-1",
            "data-keyboard-dialog":"true","data-dialog-kind":"other","data-dialog-generation":generation,"data-layout-generation":layout,"data-list-id":"{view.list_id}","data-view-selection":"{selection_generation}",
            onkeydown:move |event|if event.key()==Key::Escape{event.prevent_default();event.stop_propagation();if let Some(comments)=comments_generation {comments::back(state,comments);}else if let Some(detail)=detail_generation {crate::slack_lists::close_record_detail(state,detail);}else{comments::close_list(state,generation);}},
            onmounted:move |_|{dioxus::document::eval(INSTALL_KEYBOARD);},
            header{class:"slack-list-header",
                div{class:"slack-list-heading",h2{id:"slack-list-title","{view.title}"}
                    label{class:"slack-list-view-picker",r#for:"slack-list-view-select",
                        span{"View"}
                        select{id:"slack-list-view-select",value:"{selected}",
                            onchange:move |event|{let id=event.value();crate::slack_lists::select_view(state,(!id.is_empty()).then_some(id),generation,layout);page.set(((None,0,0),0));reset_scroll();},
                            option{value:"",selected:selected.is_empty(),"All loaded items"}
                            for choice in view.view_choices.iter(){option{key:"{choice.id}",value:"{choice.id}",selected:choice.id==selected,"{choice.name}"}}
                            if view.selected_view.as_ref().is_some_and(|id|!view.view_choices.iter().any(|choice|&choice.id==id)) {
                                option{value:"{selected}",selected:true,if view.pending_refinement{"Loading saved view…"}else{"Unavailable saved view"}}
                            }
                        }
                    }
                    {visibility::action(state,generation)}
                    {filter::action(state,generation)}
                    {sort::action(state,generation)}
                    {grouping::action(state,generation)}
                    {refinement::controls(state,generation)}
                    {rename::view_action(state,generation)}
                    {rename::create_action(state,generation)}
                }
                {create::action(state,generation,layout)}
                button{id:"slack-list-refresh",r#type:"button",class:"slack-list-action",disabled:view.loading||deleting,onclick:move |_|crate::slack_lists::refresh(state),"Refresh"}
                button{id:"slack-list-close",r#type:"button",class:"icon-btn","aria-label":"Close list",onclick:move |_|comments::close_list(state,generation),{icon(Icon::Close,"icon")}}
            }
            if detail_generation.is_none()&&view.find_available {{find::bar(state,&view)}}
            div{class:"slack-list-notices","aria-live":"polite",
                {create::status(state,generation)}
                {refinement::status(state)}
                {delete::status(state)}
                {editor::assignment_recovery(state)}
                if detail_generation.is_none()&&view.selected_view.is_none(){p{class:"slack-list-view-note","Showing loaded items without saved-view filters or grouping."}}
                if view.pending_refinement{p{role:"status","Loading the saved view metadata…"}}else if let Some(reason)=&view.view_notice{p{class:"slack-list-warning",role:"status","{reason} Choose another view or All loaded items."}}
                if let Some(reason)=&view.read_only_reason{p{class:"slack-list-read-only","{reason}"}}
                if view.stale&&shown>0&&!deletion_unresolved{p{class:"slack-list-warning","This list has changed. Refresh to continue editing."}}
                if view.incomplete{p{class:"slack-list-warning","Only part of this list is available. More items may exist."}}
                if let Some(error)=&view.error{p{class:"slack-list-error",role:"alert","{error}"}}
                if view.loading{p{role:"status","Loading list…"}}
            }
            div{class:"slack-list-layout",hidden:detail_generation.map(|_|true),inert:detail_generation.map(|_|true),
            if count==0&&!view.loading&&view.error.is_none()&&view.view_notice.is_none(){
                div{class:"slack-list-empty",role:"status",if !view.find_applied.is_empty(){"No loaded items match this view and Find."}else if view.incomplete{"No items are available in this part of the list."}else if view.selected_view.is_some(){"No loaded items match this view."}else{"This list has no items."}}
            }
            if is_board {
                board::Board{key:"board-{selected}",state}
            } else if !view.columns.is_empty()&&count>0 {
                p{id:"slack-list-keyboard",class:"slack-list-help","{KEYBOARD_HELP}"}
                KeyboardHelp{id:"slack-list-shortcuts",label:"List keyboard shortcuts",help:KEYBOARD_HELP}
                div{class:"slack-list-scroll","aria-busy":view.loading,
                    table{class:"slack-list-grid",role:if view.selected_view.is_none(){"treegrid"}else{"grid"},"aria-labelledby":"slack-list-title","aria-describedby":"slack-list-keyboard","aria-rowcount":row_count,"aria-colcount":column_count,
                        thead{tr{"aria-rowindex":1,for (index,column) in view.columns.iter().enumerate(){th{key:"{column.id}",scope:"col","aria-colindex":index+1,"data-column-id":"{column.id}","data-pinned":column.pinned.to_string(),"data-column-size":column.size,{rename::action(state,column,generation)}}}}}
                        tbody{ {rendered_rows.into_iter()} }
                    }
                }
            }
            footer{class:"slack-list-pagination",
                span{role:"status","{page_status}"}
                if !is_board {div{button{id:"slack-list-previous",r#type:"button",disabled:current_page==0,onclick:move |_|{page.set((page_previous.clone(),current_page.saturating_sub(1)));reset_scroll();},"Previous"}
                    button{id:"slack-list-next",r#type:"button",disabled:current_page+1>=page_count,onclick:move |_|{page.set((page_next.clone(),current_page+1));reset_scroll();},"Next"}}}
            }
            }
            if let Some(open)=detail_open {
                div{class:"slack-list-record-host",hidden:comments_open.then_some(true),inert:comments_open.then_some(true),
                    record::RecordDetail{key:"record-detail-{open}",state}
                }
            }
            {comments::panel(state)}
            if let Some(completion)=crate::slack_lists::completion::presentation(&state.read()) {
                div{id:"slack-list-completion-status",class:"slack-list-warning",role:"status","aria-busy":completion.pending,
                    p{"{completion.message}"}
                    div{class:"slack-list-record-actions",
                        if completion.can_check {button{r#type:"button",onclick:move |_|crate::slack_lists::completion::check(state,completion.serial),"Check result"}}
                        if completion.can_continue {button{r#type:"button",onclick:move |_|crate::slack_lists::completion::continue_reference(state,completion.serial),"Update referenced item"}}
                        if completion.can_dismiss {button{r#type:"button",onclick:move |_|crate::slack_lists::completion::dismiss(state,completion.serial),"Dismiss"}}
                    }
                }
            }
            {delete::confirmation(state,generation)}
            {rename::panel(state)}
            if let Some(reminder)=crate::slack_lists::reminder::presentation(&state.read()) {reminder::Reminder{key:"list-reminder-{reminder.generation}",state,view:reminder}}
            if let Some(receipt)=view.board_move_return {board_move::MoveReturn{key:"board-return-{receipt.serial}",state,receipt}}
            if let Some(editor)=view.editor {editor::CellEditor{key:"list-cell-editor-{editor.generation}",state,editor,visit:generation}}
        }
    }
}
/// Native disclosure keeps shortcut help keyboard-accessible without taking
/// transcript-sized space from the List. The separate hidden descriptions stay
/// attached to the grid/detail through aria-describedby.
#[component]
fn KeyboardHelp(id: &'static str, label: &'static str, help: &'static str) -> Element {
    rsx! {details{id,class:"slack-list-keyboard-disclosure",
        summary{tabindex:"0","aria-label":label,{icon(Icon::ChevronRight,"icon")}"Keyboard shortcuts"}
        p{"{help}"}
    }}
}
fn reset_scroll() {
    dioxus::document::eval(
        "requestAnimationFrame(()=>{for(const s of document.querySelectorAll('.slack-list-dialog .slack-list-scroll,.slack-list-dialog .slack-list-board,.slack-list-dialog .slack-list-layout'))s.scrollTop=0;});",
    );
}
const INSTALL_KEYBOARD: &str = include_str!("slack_lists/keyboard.js");
