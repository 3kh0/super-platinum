//! Full-schema record details share the list's cached cells and guarded editors.
use super::*;
const FIELD_PAGE: usize = 30;
const HELP: &str = "Up and Down move between fields. Space changes a checkbox. F2 edits a field. Enter opens field controls; Escape returns to the field or back to the list.";
#[component]
pub(super) fn RecordDetail(state: Signal<ShellState>) -> Element {
    let mut page = use_signal(|| 0usize);
    let Some(view) = crate::slack_lists::presentation(&state.read()) else {
        return rsx! {};
    };
    let Some(detail) = view.detail.as_ref() else {
        return rsx! {};
    };
    let duplicate_reason = crate::slack_lists::create::duplicate::reason(&state.read());
    let create_blocked = crate::slack_lists::create::blocks_mutation(&state.read());
    let deletion = crate::slack_lists::delete::presentation(&state.read());
    let delete_blocked = crate::slack_lists::delete::blocks_mutation(&state.read());
    let title = if detail.title.trim().is_empty() {
        "Untitled item"
    } else {
        detail.title.as_str()
    };
    let parent_return = detail.parent_return;
    let generation = detail.generation;
    let open_generation = detail.open_generation;
    let visit = view.generation;
    let record_id = serde_json::to_string(&detail.record_id).unwrap();
    let return_child = serde_json::to_string(&detail.return_child).unwrap();
    let count = detail.columns.len();
    let pages = count.div_ceil(FIELD_PAGE).max(1);
    let current = (*page.read()).min(pages - 1);
    let start = current * FIELD_PAGE;
    let end = (start + FIELD_PAGE).min(count);
    let media_epoch = state.read().media_epoch;
    let fields=detail.columns[start..end].iter().filter_map(|column|detail.row.cells.iter().find(|cell|cell.column_id==column.id).map(|cell|(column,cell)))
        .map(|(column,cell)|{
            let column_id=cell.column_id.clone();let edit_column=column_id.clone();
            let mutation=view.mutation.as_ref().filter(|m|m.record_id==detail.record_id&&m.column_id==cell.column_id);
            let pending=mutation.is_some_and(|m|!m.uncertain);let uncertain=mutation.is_some_and(|m|m.uncertain);
            let blocked=detail.unavailable||view.loading||view.stale||view.mutation.is_some()||delete_blocked||create_blocked;
            let checked=vec![Attribute::new("checked",cell.checkbox.unwrap_or(false),None,true)];
            let value=if cell.checkbox.is_some(){rsx!{input{r#type:"checkbox",tabindex:"-1",disabled:blocked||!cell.editable,"aria-label":"{column.label}, {title}",
                onchange:move |_|crate::slack_lists::toggle_detail_checkbox(state,generation,column_id.clone()),..checked}}}
            else if cell.text.is_empty(){rsx!{span{class:"slack-list-detail-empty","Empty"}}}
            else{match &cell.content{
                crate::slack_lists::CellContent::Vote{count,voted,tooltip,glyph}=> {
                    let intent=if *voted {super_platinum_core::slack_lists::VoteIntent::RemoveMyVote}else{super_platinum_core::slack_lists::VoteIntent::AddMyVote};
                    super::vote::control(state,*count,*voted,tooltip,glyph,media_epoch,false,blocked||!cell.editable,true,move ||crate::slack_lists::toggle_detail_vote(state,generation,column_id.clone(),intent))
                },
                crate::slack_lists::CellContent::Rating{value,max,glyph}=>super::rating::display(state,*value,*max,glyph,false,media_epoch),
                            crate::slack_lists::CellContent::People(people)=>rsx!{super::people::PeopleAvatars{state,people:people.clone(),media_epoch}},
                            crate::slack_lists::CellContent::Plain=>rsx!{span{class:"slack-list-cell-text","{cell.text}"}},
                crate::slack_lists::CellContent::Timestamp{tooltip}=>rsx!{span{class:"slack-list-cell-text slack-list-timestamp",title:"{tooltip}","{cell.text}"}},
                crate::slack_lists::CellContent::Rich(nodes)=>rsx!{div{class:"slack-list-cell-rich",for node in nodes{{super::super::rich::rich_node(node,media_epoch,state)}}}},
                crate::slack_lists::CellContent::Labels(labels)=>rsx!{div{class:"slack-list-cell-labels",for label in labels{span{class:"slack-list-option","{label}"}}}},
            }};
            rsx!{div{key:"{column.id}",class:"slack-list-detail-field","data-column-id":"{column.id}","data-row-id":"{detail.record_id}",role:"group","aria-label":"{column.label}","aria-busy":pending,tabindex:"-1",
                div{class:"slack-list-detail-label","{column.label}"}
                div{class:"slack-list-detail-value",{value}}
                if cell.can_edit&&cell.checkbox.is_none(){button{r#type:"button",class:"slack-list-edit-cell",tabindex:"-1",disabled:blocked,"aria-label":"Edit {column.label}, {title}",title:"Edit field (F2)",
                    onclick:move |_|crate::slack_lists::begin_detail_cell_editor(state,generation,edit_column.clone()),{icon(Icon::Compose,"icon")}}}
                if pending{span{class:"slack-list-cell-status","Saving…"}}
                if uncertain{span{class:"slack-list-cell-status","Save unconfirmed. Refresh to check."}}
            }}
        }).collect::<Vec<_>>();
    rsx! {section{id:"slack-list-record-detail",class:"slack-list-record-detail",role:"region","aria-labelledby":"slack-list-record-title","aria-describedby":"slack-list-record-help","data-record-id":"{detail.record_id}","data-detail-generation":generation,"data-detail-open-generation":open_generation,"data-escape-local":"true",tabindex:"-1",
        onmounted:move |_|{dioxus::document::eval(&format!("{MOUNT}\nmountListRecordDetail({open_generation},{visit},{record_id},{return_child});"));crate::slack_lists::track_open(state,open_generation);},
        header{class:"slack-list-record-header",button{id:"slack-list-record-back",r#type:"button",class:"slack-list-action",onclick:move |_|crate::slack_lists::close_record_detail(state,generation),{icon(Icon::ChevronRight,"icon")}if parent_return{"Back to parent"}else{"Back to list"}}
            h3{id:"slack-list-record-title","{title}"}
            div{class:"slack-list-record-actions",
            button{id:"slack-list-record-duplicate",r#type:"button",class:"slack-list-action",disabled:duplicate_reason.is_some(),title:duplicate_reason.unwrap_or("Duplicate item"),onclick:move |_|crate::slack_lists::create::duplicate::duplicate_item(state,generation),"Duplicate item"}
            button{id:"slack-list-record-delete",r#type:"button",class:"slack-list-action slack-list-delete-action",disabled:!deletion.enabled,title:deletion.reason.clone().unwrap_or_else(||"Delete item".into()),onclick:move |_|crate::slack_lists::delete::begin_delete(state,generation),{icon(Icon::Delete,"icon")}"Delete item"}
            }
        }
        p{id:"slack-list-record-help",class:"slack-list-help","{HELP}"}
        KeyboardHelp{id:"slack-list-record-shortcuts",label:"Item keyboard shortcuts",help:HELP}
        if detail.unavailable{p{class:"slack-list-warning",role:"status","This item is no longer available. Its last loaded fields are shown below."}}
        div{class:"slack-list-detail-scroll","aria-busy":view.loading,{fields.into_iter()}
            if detail.row.parent_id.is_none()&&crate::slack_lists::subitem::visible(&state.read()){super::subitem::Subitems{state,record:detail.record_id.clone(),generation,open:open_generation,visit}}
        }
        if pages>1{footer{class:"slack-list-detail-pagination",span{role:"status","Fields {start+1}–{end} of {count}"}
            button{id:"slack-list-detail-previous",r#type:"button",disabled:current==0,onclick:move |_|page.set(current.saturating_sub(1)),"Previous fields"}
            button{id:"slack-list-detail-next",r#type:"button",disabled:current+1>=pages,onclick:move |_|page.set(current+1),"Next fields"}
        }}
    }}
}
const MOUNT: &str = include_str!("record_keyboard.js");
