use super::*;

pub(super) fn row(
    state: Signal<ShellState>,
    view: &crate::slack_lists::Presentation,
    row: &crate::slack_lists::RowView,
    index: usize,
    first: bool,
    media_epoch: u64,
) -> Element {
    let generation = view.generation;
    let layout = view.layout_generation;
    rsx! {
        tr { key:"{row.instance_id}","data-row-id":"{row.id}","data-instance-id":"{row.instance_id}","data-depth":row.depth,
            "data-parent-id":row.parent_id.as_deref().unwrap_or(""),
            "aria-rowindex":index+2,"aria-level":row.depth+1,"aria-expanded":row.has_children.then(||row.expanded.to_string()),
            for (column_index,cell) in row.cells.iter().enumerate() {
                {
                    let instance=row.instance_id.clone();let edit_instance=instance.clone();let detail_instance=instance.clone();
                    let toggle_row=row.id.clone();let column_id=cell.column_id.clone();let edit_column=column_id.clone();
                    let label=view.columns.get(column_index).map(|column|column.label.as_str()).unwrap_or("Value");
                    let primary=view.columns.get(column_index).is_some_and(|column|column.primary);
                    let item=if row.label.is_empty(){"Untitled item"}else{row.label.as_str()};
                    let checkbox_label=format!("{label}, {item}");
                    let toggle_label=format!("{} {}",if row.expanded{"Collapse"}else{"Expand"},item);
                    let indent=format!("padding-inline-start: {}px",row.depth.min(8)*16);
                    let mutation=view.mutation.as_ref().filter(|m|m.record_id==row.id&&m.column_id==cell.column_id);
                    let pending=mutation.is_some_and(|m|!m.uncertain);let uncertain=mutation.is_some_and(|m|m.uncertain);
                    let disabled=!cell.editable||view.mutation.is_some()||view.loading||view.stale;
                    let checked_attributes=vec![Attribute::new("checked",cell.checkbox.unwrap_or(false),None,true)];
                    let content=if primary&&cell.text.trim().is_empty()&&cell.checkbox.is_none() {
                        rsx!{span{class:"slack-list-cell-text","{item}"}}
                    }else if cell.checkbox.is_some() {
                        rsx!{input{r#type:"checkbox",tabindex:"-1",disabled,"aria-label":"{checkbox_label}",
                            onchange:move |_|crate::slack_lists::toggle_checkbox(state,instance.clone(),column_id.clone(),generation,layout),..checked_attributes}}
                    }else {
                        match &cell.content {
                            crate::slack_lists::CellContent::Plain=>rsx!{span{class:"slack-list-cell-text","{cell.text}"}},
                            crate::slack_lists::CellContent::Rich(nodes)=>rsx!{div{class:"slack-list-cell-rich",for node in nodes {{super::super::rich::rich_node(node,media_epoch,state)}}}},
                            crate::slack_lists::CellContent::Labels(labels)=>rsx!{div{class:"slack-list-cell-labels",for label in labels {span{class:"slack-list-option","{label}"}}}},
                        }
                    };
                    rsx!{
                        td{key:"{cell.column_id}",role:if primary{"rowheader"}else{"gridcell"},
                            "data-column-id":"{cell.column_id}","aria-colindex":column_index+1,"data-tree-primary":primary.to_string(),
                            "aria-readonly":disabled,"aria-busy":pending,tabindex:if first&&column_index==0{"0"}else{"-1"},
                            if primary {
                                div{class:"slack-list-tree-cell",style:"{indent}",
                                    if row.has_children {
                                        button{r#type:"button",class:"slack-list-tree-toggle",tabindex:"-1","aria-label":"{toggle_label}","aria-expanded":row.expanded,
                                            onclick:move |_|crate::slack_lists::toggle_expanded(state,toggle_row.clone(),generation),{icon(Icon::ChevronRight,"icon")}}
                                    }else if view.selected_view.is_none(){span{class:"slack-list-tree-spacer","aria-hidden":"true"}}
                                    div{class:"slack-list-tree-content",{content}
                                        if let Some(notice)=&row.parent_notice{span{class:"slack-list-parent-notice","{notice}"}}
                                    }
                                }
                            }else{{content}}
                            if primary||(column_index==0&&!view.columns.iter().any(|column|column.primary)){
                                button{r#type:"button",class:"slack-list-record-open",tabindex:"-1","aria-label":"Open {item}",title:"Open item details (Alt+Enter)",
                                    onclick:move |_|crate::slack_lists::open_record_detail(state,detail_instance.clone(),generation,layout),{icon(Icon::Expand,"icon")}}
                            }
                            if cell.can_edit&&cell.checkbox.is_none(){
                                button{r#type:"button",class:"slack-list-edit-cell",tabindex:"-1","aria-label":"Edit {label}, {item}",title:"Edit cell (F2)",
                                    disabled:view.mutation.is_some()||view.loading||view.stale,
                                    onclick:move |_|crate::slack_lists::begin_cell_editor(state,edit_instance.clone(),edit_column.clone(),generation,layout),
                                    {icon(Icon::Compose,"icon")}}
                            }
                            if pending{span{class:"slack-list-cell-status","Saving…"}}
                            if uncertain{span{class:"slack-list-cell-status","Save unconfirmed. Refresh to check."}}
                        }
                    }
                }
            }
        }
    }
}
