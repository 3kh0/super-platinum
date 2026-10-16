use super::*;
mod select;
use crate::slack_lists::rename::{self, Stage};
pub(super) fn action(
    state: Signal<ShellState>,
    column: &crate::slack_lists::ColumnView,
    visit: u64,
) -> Element {
    if rename::precision::supports(&state.read(), &column.id) {
        let id = column.id.clone();
        let disabled = !rename::can_open(&state.read(), &id);
        return rsx! {
            div{class:"slack-list-number-header-actions",
                {super::precision::action(state,column,visit)}
                button{r#type:"button",class:"slack-list-action slack-list-number-rename",disabled,
                    title:"Rename {column.label} field","aria-label":"Rename {column.label} field",
                    "data-rename-column":"{column.id}",onclick:move |_|rename::open(state,id.clone(),visit),
                    {icon(Icon::Compose,"icon")}
                }
            }
        };
    }
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
    let text_disabled = !rename::text_create::can_open(&state.read());
    let text_view = rename::selected_view(&state.read())
        .unwrap_or_default()
        .to_owned();
    rsx! {button{id:"slack-list-new-text-field",r#type:"button",class:"slack-list-action slack-list-rename-header",disabled:text_disabled,"data-new-text-field-view":"{text_view}",title:"Requires an owned writable saved Table and verified available field capacity",onclick:move |_|rename::text_create::open(state,visit,selection),{icon(Icon::Add,"icon")}span{"Add field"}}
    button{id:"slack-list-new-view",r#type:"button",title:"Requires a current writable List and verified available view capacity",class:"slack-list-action slack-list-rename-header",disabled,onclick:move |_|rename::view_create::open(state,visit,selection),{icon(Icon::Add,"icon")}span{"New view"}}}
}
pub(super) fn panel(state: Signal<ShellState>) -> Element {
    let Some(view) = rename::presentation(&state.read()) else {
        return rsx! {};
    };
    render_panel(state, view)
}

fn render_panel(state: Signal<ShellState>, view: rename::Presentation) -> Element {
    let serial = view.serial;
    let column = view.column;
    let saved_view = view.view.unwrap_or_default();
    let noun = view.noun;
    let creation = view.creation;
    let text_creation = view.text_creation;
    let field_choice = view.field_choice;
    let field_kind = field_choice.map(|c| c.kind.as_str()).unwrap_or_default();
    let field_label = field_choice
        .map(|c| rename::text_create::label(c.kind))
        .unwrap_or("Text");
    let field_revision = field_choice.map(|c| c.revision).unwrap_or_default();
    let text_view = if text_creation {
        saved_view.clone()
    } else {
        String::new()
    };
    let selected = rename::selection(&state.read()).unwrap_or_default();
    let precision = view.precision;
    let visibility = view.visibility;
    let filter = view.filter;
    let filter_view = if filter.is_some() {
        saved_view.clone()
    } else {
        String::new()
    };
    let view_grouping = view.view_grouping;
    let grouping_view = if view_grouping.is_some() {
        saved_view.clone()
    } else {
        String::new()
    };
    let sort = view.sort;
    let sort_view = if sort.is_some() {
        saved_view.clone()
    } else {
        String::new()
    };
    let visibility_view = if visibility.is_some() {
        saved_view.clone()
    } else {
        String::new()
    };
    let precision_column = if precision.is_some() {
        column.clone()
    } else {
        String::new()
    };
    let title = if text_creation {
        "Add field".to_owned()
    } else if filter.is_some() {
        "Filter".to_owned()
    } else if view_grouping.is_some() {
        "Group by".to_owned()
    } else if sort.is_some() {
        "Sort".to_owned()
    } else if visibility.is_some() {
        "Fields".to_owned()
    } else if precision.is_some() {
        "Number format".to_owned()
    } else if creation.is_some() {
        "New view".to_owned()
    } else {
        format!("Rename {noun}")
    };
    let create_layout = creation.map(|l| l.as_str()).unwrap_or_default();
    let grouping = view.grouping;
    let grouping_options = view.grouping_options;
    let label = if noun == "view" {
        "View name"
    } else {
        "Field name"
    };
    let filter_valid = filter
        .as_ref()
        .is_none_or(|p| rename::filter::valid(&p.desired));
    let select_valid = view.select_draft.as_ref().is_none_or(|d| d.ready());
    let filter_save = filter.clone();
    let grouping_save = view_grouping.clone();
    let unchanged = if let Some(p) = &view_grouping {
        p.desired == p.original
    } else if let Some(p) = &filter {
        p.desired == p.original
    } else if let Some(p) = &sort {
        p.desired == p.original
    } else if let Some(p) = &visibility {
        p.desired == p.original
    } else if let Some(p) = &precision {
        p.desired == p.original
    } else if noun == "view" {
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
    let checkable = view.checkable;
    let closable = matches!(
        view.stage,
        Stage::Editing | Stage::NotRequested | Stage::Observed
    ) || (matches!(view.stage, Stage::Conflict | Stage::Available)
        && view.current_review);
    let closable = closable
        && (!(text_creation || view_grouping.is_some())
            || view.stage != Stage::Observed
            || view.current_review);
    let stage = format!("{:?}", view.stage);
    let current = view.current_fields;
    rsx! {
        div{class:"slack-list-editor-shade"}
        section{id:"slack-list-rename",class:"slack-list-cell-editor",role:"dialog","aria-modal":"true","aria-labelledby":"slack-list-rename-title",
            "data-keyboard-dialog":"true","data-rename-serial":"{serial}","data-rename-stage":"{stage}","data-rename-column":"{column}","data-precision-column":"{precision_column}","data-rename-view":"{saved_view}","data-view-create":"{create_layout}","data-visibility-view":"{visibility_view}","data-filter-view":"{filter_view}","data-new-text-field-view":"{text_view}","data-sort-view":"{sort_view}","data-grouping-view":"{grouping_view}",
            onmounted:move |_|{dioxus::document::eval(&format!("{}\n{}\n{}",include_str!("rename_restore.js"),include_str!("view_create_restore.js"),include_str!("rename_focus.js")));},
            h3{id:"slack-list-rename-title","{title}"}
            div{class:"slack-list-editor-body",
                if !text_creation && creation.is_none() && precision.is_none() && visibility.is_none() && sort.is_none() && filter.is_none() && view_grouping.is_none(){p{class:"slack-list-editor-item","Original name: {view.before}"}}
                if text_creation {
                    label{r#for:"slack-list-new-field-kind","Type"}
                    select{id:"slack-list-new-field-kind",value:"{field_kind}","data-field-choice":"{field_revision}",disabled:!editing||!view.can_confirm,
                        onchange:move |event|{if let Some(rendered)=field_choice{rename::text_create::choose_kind(state,serial,visit,selected,rendered,event.value());}},
                        option{value:"text",selected:field_kind=="text","Text"}
                        option{value:"number",selected:field_kind=="number","Number"}
                        option{value:"checkbox",selected:field_kind=="checkbox","Checkbox"}
                        option{value:"date",selected:field_kind=="date","Date"}
                        option{value:"select",selected:field_kind=="select","Select"}}
                    p{class:"slack-list-editor-hint",if field_kind=="number"{"Number format: 0 decimal places. Missing values stay empty. "}else if field_kind=="checkbox"{"Editable empty cells appear unchecked. Values are saved only when you change a checkbox. "}"Shared with everyone who has access to this List."}
                }
                if creation.is_some(){p{class:"slack-list-editor-hint","Views are shared with everyone who has access to this List. New views do not copy the selected view settings."}
                    label{r#for:"slack-list-new-view-layout","Layout"}
                    select{id:"slack-list-new-view-layout",value:"{create_layout}",disabled:!editing,onchange:move |event|{use super_platinum_core::slack_lists::ViewLayout;let layout=match event.value().as_str(){"table"=>ViewLayout::Table,"board"=>ViewLayout::Board,_=>return};rename::view_create::layout(state,serial,layout);},option{value:"table",selected:create_layout=="table","Table"}option{value:"board",selected:create_layout=="board","Board"}}
                    label{r#for:"slack-list-new-view-grouping","Group by"}
                    select{id:"slack-list-new-view-grouping",value:"{grouping}",disabled:!editing||!view.can_confirm,
                        onchange:move |event|rename::view_create::grouping(state,serial,visit,event.value()),
                        option{value:"",selected:grouping.is_empty(),"None"}
                        for (key, name) in grouping_options {
                            option{value:"{key}",selected:grouping==key,"{name}"}
                        }
                    }
                }
                if let Some(p)=view_grouping.clone() {{super::grouping::fields(state,serial,visit,p,editing && view.can_confirm)}} else if let Some(p)=filter.clone() {{super::filter::fields(state,serial,visit,p,editing && view.can_confirm)}} else if let Some(p)=sort.clone() {{super::sort::fields(state,serial,visit,p,editing && view.can_confirm)}} else if let Some(p)=visibility.clone() {{super::visibility::fields(state,serial,visit,p,editing && view.can_confirm)}} else if let Some(p)=precision.clone() {{super::precision::fields(state,serial,visit,p,editing && view.can_confirm)}} else {
                label{r#for:"slack-list-rename-name","{label}"}
                input{id:"slack-list-rename-name",r#type:"text","data-field-choice":"{field_revision}",value:"{view.name}",maxlength:view.limit,disabled:!editing||(text_creation&&!view.can_confirm),
                    oninput:move |event|{if text_creation {if let Some(rendered)=field_choice {rename::text_create::input(state,serial,visit,selected,rendered,event.value());}}else{rename::input(state,serial,event.value())}}}
                }
                if let (Some(draft),Some(choice))=(view.select_draft,field_choice) {{select::fields(state,serial,visit,selected,choice,draft,editing && view.can_confirm)}}
                p{id:"slack-list-rename-message",class:"slack-list-editor-hint",role:"status","{view.message}"}
                if matches!(view.stage,Stage::Observed|Stage::Conflict|Stage::Available){p{id:"slack-list-rename-current",class:"slack-list-editor-hint","Current fields: {current}"}}
                if field_kind!="select" {if let Some(observed)=view.observed_field.as_ref() {{observed_summary(observed,field_label)}}}
                if view.blocked{p{class:"slack-list-warning","Slack asked us to wait. Checking and changes are temporarily unavailable."}}
            }
            if field_kind=="select" {if let Some(observed)=view.observed_field.as_ref() {{observed_summary(observed,field_label)}}}
            div{class:"slack-list-editor-actions",
                button{id:"slack-list-rename-leave",r#type:"button",onclick:move |_|{let mut state=state;crate::slack_lists::close_shell(&mut state.write());},"Close list"}
                if closable{button{id:"slack-list-rename-close",r#type:"button",onclick:move |_|close(state,serial,visit),if editing{"Cancel"}else if matches!(view.stage,Stage::Conflict|Stage::Available){"Close review"}else{"Done"}}}
                if editing{button{id:"slack-list-rename-confirm",r#type:"button",class:"primary",disabled:!view.can_confirm||!filter_valid||!select_valid||(precision.is_none()&&visibility.is_none()&&sort.is_none()&&filter.is_none()&&view_grouping.is_none()&&view.name.trim().is_empty())||unchanged,
                    onclick:move |_|{if let Some(p)=grouping_save.clone(){rename::grouping::confirm(state,serial,visit,p.choice)}else{super::filter::confirm(state,serial,visit,filter_save.clone())}},if visibility.is_some()||sort.is_some()||filter.is_some()||view_grouping.is_some(){"Save for everyone"}else if precision.is_some(){"Save"}else if text_creation{"Create field"}else if creation.is_some(){"Create view"}else{"Rename {noun}"}}}
                if checkable{button{id:"slack-list-rename-check",r#type:"button",class:"primary",disabled:!view.can_check,
                    onclick:move |_|rename::check(state,serial),if view_grouping.is_some(){"Check current grouping"}else if sort.is_some(){"Check current sort"}else if filter.is_some(){"Check current filter"}else if visibility.is_some(){"Check current visibility"}else if precision.is_some(){"Check current format"}else if text_creation{"Check current fields"}else if creation.is_some(){"Check current views"}else{"Check current {noun}"}}}
                if view.stage==Stage::Available {button{id:"slack-list-new-view-open",r#type:"button",class:"primary",disabled:!view.can_navigate,onclick:move |_|{if let Some(opened)=rename::view_create::open_available(state,serial){let proof=serde_json::to_string(&opened).unwrap();dioxus::document::eval(&format!("window.__listViewCreateReturn?.commit({proof})"));}},"Open view"}}
                if busy{span{role:"status","Please wait..."}}
            }
        }
    }
}

// Confirmation remains readable beside Done even with eight retained options.
fn observed_summary(observed: &rename::text_create::Observed, label: &str) -> Element {
    rsx! {p{id:"slack-list-text-field-observed",class:"slack-list-editor-hint","data-column-id":"{observed.id}","data-column-key":"{observed.key}","Observed {label} field: {observed.name}"}}
}

#[cfg(test)]
mod observed_tests;

fn close(state: Signal<ShellState>, serial: u64, visit: u64) {
    if crate::slack_lists::generation(&state.peek()) != Some(visit) {
        return;
    }
    // Native click arms exact-origin restoration before this async DOM update.
    // A refused close leaves the panel mounted and never restores underneath it.
    rename::close(state, serial);
}
