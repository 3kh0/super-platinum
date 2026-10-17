//! Bounded Board lanes. Canonical records may have several distinct card instances.
use super::*;
mod lane_add;
const LANES: usize = 4;
const CARDS: usize = 10;
const HELP: &str = "Up and Down move between cards; Left and Right move between nonempty lanes. Home and End move to the first and last card. Tab and Shift+Tab move between fields within a card. Alt+Enter opens item details. F2 edits a field. Space changes a checkbox. Enter opens field controls; Escape returns to the field.";

#[component]
pub(super) fn Board(state: Signal<ShellState>) -> Element {
    let mut page = use_signal(|| (0u64, 0usize));
    let Some(view) = crate::slack_lists::presentation(&state.read()) else {
        return rsx! {};
    };
    let pages = view.lanes.len().div_ceil(LANES).max(1);
    let revision = view.find_revision.max(view.grouping_revision);
    let current = find::local_page(*page.read(), revision, pages);
    let start = current * LANES;
    let end = (start + LANES).min(view.lanes.len());
    let rendered = view.lanes[start..end]
        .iter()
        .map(|lane| {
            let lane_key = lane.key.clone();
            rsx! {Lane {key:"{lane.key}",state,lane_key}}
        })
        .collect::<Vec<_>>();
    rsx! {
        p {class:"slack-list-help",id:"slack-list-board-help","{HELP}"}
        div {class:"slack-list-board",role:"region","aria-label":"Board","aria-describedby":"slack-list-board-help","aria-busy":view.loading,
            onmounted:move |_|{dioxus::document::eval(include_str!("board_keyboard.js"));},
            {rendered.into_iter()}
        }
        if pages>1 {
            div {class:"slack-list-board-pagination",
                span {"Lanes {start+1}–{end} of {view.lanes.len()}"}
                button {id:"slack-list-lanes-previous",disabled:current==0,onclick:move |_|{page.set((revision,current.saturating_sub(1)));},"Previous lanes"}
                button {id:"slack-list-lanes-next",disabled:current+1>=pages,onclick:move |_|{page.set((revision,current+1));},"Next lanes"}
            }
        }
        details {class:"slack-list-keyboard-disclosure slack-list-board-shortcuts",summary{tabindex:"0",{icon(Icon::ChevronRight,"icon")}"Keyboard shortcuts"}p{"{HELP}"}}

    }
}
#[component]
fn Lane(state: Signal<ShellState>, lane_key: String) -> Element {
    let mut page = use_signal(|| (0u64, 0usize));
    let Some(view) = crate::slack_lists::presentation(&state.read()) else {
        return rsx! {};
    };
    let Some(lane) = view.lanes.iter().find(|lane| lane.key == lane_key) else {
        return rsx! {};
    };
    let pages = lane.rows.len().div_ceil(CARDS).max(1);
    let revision = view.find_revision.max(view.grouping_revision);
    let current = find::local_page(*page.read(), revision, pages);
    let start = current * CARDS;
    let end = (start + CARDS).min(lane.rows.len());
    let media_epoch = state.read().media_epoch;
    let cards = lane.rows[start..end]
        .iter()
        .map(|row| card(state, &view, row, media_epoch))
        .collect::<Vec<_>>();
    let title = if let Some(heading) = &lane.checkbox_heading {
        heading.accessible_label.as_str()
    } else if lane.label.is_empty() {
        "Items"
    } else {
        lane.label.as_str()
    };
    rsx! {
        section {class:"slack-list-lane","data-group-key":"{lane.key}","data-count":lane.count,"aria-label":"{title}",
            header {class:"slack-list-lane-title",
                h3 {"aria-label":lane.checkbox_heading.as_ref().map(|heading| heading.accessible_label.as_str()),
                    if let Some(heading) = &lane.checkbox_heading {
                        span {"data-checkbox-heading":if heading.checked {"checked"} else {"unchecked"},
                            {icon(if heading.checked {Icon::CheckBox} else {Icon::CheckBoxOutlineBlank},"icon slack-list-lane-checkbox")}
                        }
                    } else {"{title}"}
                }
                span {class:"slack-list-group-count","{lane.count}"}
                lane_add::LaneAdd {key:"add-{view.generation}-{view.layout_generation}-{lane.key}",state,visit:view.generation,layout:view.layout_generation,lane_key:lane.key.clone()}
            }
            div {class:"slack-list-lane-cards",{cards.into_iter()}
                if lane.rows.is_empty(){p{class:"slack-list-lane-empty","No items"}}
            }
            if pages>1 {
                footer {class:"slack-list-lane-pagination",
                    span {"{start+1}–{end} of {lane.count}"}
                    button {class:"slack-list-lane-previous",disabled:current==0,"aria-label":"Previous cards in {title}",onclick:move |_|{page.set((revision,current.saturating_sub(1)));},"Previous"}
                    button {class:"slack-list-lane-next",disabled:current+1>=pages,"aria-label":"Next cards in {title}",onclick:move |_|{page.set((revision,current+1));},"Next"}
                }
            }
        }
    }
}
fn card(
    state: Signal<ShellState>,
    view: &crate::slack_lists::Presentation,
    row: &crate::slack_lists::RowView,
    media_epoch: u64,
) -> Element {
    let primary = view
        .columns
        .iter()
        .find(|c| c.primary)
        .map(|c| c.id.as_str());
    let completion = view.completion_column.as_deref();
    let header_cells = [completion, primary]
        .into_iter()
        .flatten()
        .filter_map(|id| row.cells.iter().find(|c| c.column_id == id))
        .map(|cell| field(state, view, row, cell, media_epoch, true))
        .collect::<Vec<_>>();
    let body_cells = view
        .columns
        .iter()
        .filter(|c| Some(c.id.as_str()) != primary && Some(c.id.as_str()) != completion)
        .filter_map(|column| row.cells.iter().find(|c| c.column_id == column.id))
        .filter(|cell| cell.checkbox.is_some() || !cell.text.trim().is_empty())
        .map(|cell| field(state, view, row, cell, media_epoch, false))
        .collect::<Vec<_>>();
    let empty = header_cells.is_empty() && body_cells.is_empty();
    let instance = row.instance_id.clone();
    let visit = view.generation;
    let layout = view.layout_generation;
    let item = if row.label.is_empty() {
        "Untitled item"
    } else {
        row.label.as_str()
    };
    rsx! {article {key:"{row.instance_id}",class:"slack-list-card","data-row-id":"{row.id}","data-instance-id":"{row.instance_id}","aria-label":if row.label.is_empty(){"Untitled item"}else{row.label.as_str()},
        div{class:"slack-list-card-heading",{header_cells.into_iter()}
            super::board_move::MoveAction {state,instance:row.instance_id.clone(),row:row.id.clone(),label:item.to_owned(),visit,layout}
            button{r#type:"button",class:"slack-list-record-open",tabindex:"-1","aria-label":"Open {item}",title:"Open item details (Alt+Enter)",
                onclick:move |_|crate::slack_lists::open_record_detail(state,instance.clone(),visit,layout),{icon(Icon::Expand,"icon")}}
        }
        {body_cells.into_iter()}
        if empty {div{class:"slack-list-card-field slack-list-card-empty","data-board-cell":"true","data-column-id":"__card",tabindex:"-1",role:"group","aria-label":"Item with no visible fields","No visible fields"}}
    }}
}
fn field(
    state: Signal<ShellState>,
    view: &crate::slack_lists::Presentation,
    row: &crate::slack_lists::RowView,
    cell: &crate::slack_lists::CellView,
    media_epoch: u64,
    heading: bool,
) -> Element {
    let generation = view.generation;
    let layout = view.layout_generation;
    let instance = row.instance_id.clone();
    let edit_instance = instance.clone();
    let column = cell.column_id.clone();
    let edit_column = column.clone();
    let label = view
        .columns
        .iter()
        .find(|c| c.id == cell.column_id)
        .map(|c| c.label.as_str())
        .unwrap_or("Completed");
    let item = if row.label.is_empty() {
        "Untitled item"
    } else {
        row.label.as_str()
    };
    let mutation = view
        .mutation
        .as_ref()
        .filter(|m| m.record_id == row.id && m.column_id == cell.column_id);
    let pending = mutation.is_some_and(|m| !m.uncertain);
    let uncertain = mutation.is_some_and(|m| m.uncertain);
    let disabled = !cell.editable
        || view.mutation.is_some()
        || view.loading
        || view.stale
        || crate::slack_lists::delete::blocks_mutation(&state.read());
    let checked = vec![Attribute::new(
        "checked",
        cell.checkbox.unwrap_or(false),
        None,
        true,
    )];
    let content = if cell.checkbox.is_some() {
        rsx! {input{r#type:"checkbox",tabindex:"-1",disabled,"aria-label":"{label}, {item}",
        onchange:move |_|crate::slack_lists::toggle_checkbox(state,instance.clone(),column.clone(),generation,layout),..checked}}
    } else if heading && cell.text.trim().is_empty() {
        rsx! {span{class:"slack-list-cell-text","{item}"}}
    } else {
        match &cell.content {
            crate::slack_lists::CellContent::Vote {
                count,
                voted,
                tooltip,
                glyph,
            } => super::vote::control(
                state,
                *count,
                *voted,
                tooltip,
                glyph,
                media_epoch,
                true,
                true,
                false,
                || {},
            ),
            crate::slack_lists::CellContent::Rating { value, max, glyph } => {
                super::rating::display(state, *value, *max, glyph, false, media_epoch)
            }
            crate::slack_lists::CellContent::People(people) => {
                rsx! {super::people::PeopleAvatars{state,people:people.clone(),media_epoch}}
            }
            crate::slack_lists::CellContent::Plain => {
                rsx! {span{class:"slack-list-cell-text","{cell.text}"}}
            }
            crate::slack_lists::CellContent::Timestamp { tooltip } => {
                rsx! {span{class:"slack-list-cell-text slack-list-timestamp",title:"{tooltip}","{cell.text}"}}
            }
            crate::slack_lists::CellContent::Rich(nodes) => {
                rsx! {div{class:"slack-list-cell-rich",for node in nodes{{super::super::rich::rich_node(node,media_epoch,state)}}}}
            }
            crate::slack_lists::CellContent::Labels(labels) => {
                rsx! {div{class:"slack-list-cell-labels",for label in labels{span{class:"slack-list-option","{label}"}}}}
            }
        }
    };
    rsx! {div{key:"{cell.column_id}",class:if heading{"slack-list-card-field slack-list-card-primary"}else{"slack-list-card-field"},
        "data-board-cell":"true","data-column-id":"{cell.column_id}",tabindex:"-1",role:"group","aria-label":"{label}, {item}","aria-busy":pending,
        if !heading {span{class:"slack-list-field-label","{label}"}}
        {content}
        if cell.can_edit&&cell.checkbox.is_none(){button{r#type:"button",class:"slack-list-edit-cell",tabindex:"-1","aria-label":"Edit {label}, {item}",title:"Edit field (F2)",disabled:view.mutation.is_some()||view.loading||view.stale||crate::slack_lists::delete::blocks_mutation(&state.read()),
            onclick:move |_|crate::slack_lists::begin_cell_editor(state,edit_instance.clone(),edit_column.clone(),generation,layout),{icon(Icon::Compose,"icon")}}}
        if pending{span{class:"slack-list-cell-status","Saving…"}}
        if uncertain{span{class:"slack-list-cell-status","Save unconfirmed. Refresh to check."}}
    }}
}
