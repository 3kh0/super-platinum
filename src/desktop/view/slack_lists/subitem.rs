use super::*;
#[component]
pub(super) fn Subitems(
    state: Signal<ShellState>,
    record: String,
    generation: u64,
    open: u64,
    visit: u64,
) -> Element {
    let selection = crate::slack_lists::subitem::selection_stamp(&state.read()).unwrap_or(0);
    let rows = crate::slack_lists::subitem::children(&state.read(), &record);
    let reason = crate::slack_lists::subitem::reason(&state.read(), generation);
    let operation = crate::slack_lists::subitem::presentation(&state.read());
    let incomplete = crate::slack_lists::presentation(&state.read()).is_some_and(|v| v.incomplete);
    let busy = operation.as_ref().is_some_and(|o| o.pending);
    rsx! {section{id:"slack-list-subitems",class:"slack-list-subitems","aria-label":"Subitems","data-subitem-busy":busy,"data-subitem-selection":selection,
        onmounted:move |_|{dioxus::document::eval(&format!("{FOCUS}\nmountListSubitems({open},{visit});"));},
        header{class:"slack-list-subitems-header",h4{"Subitems"}
            button{id:"slack-list-subitem-add",r#type:"button",class:"slack-list-action",disabled:reason.is_some(),title:reason.unwrap_or("Add a blank direct subitem"),onclick:move |_|crate::slack_lists::subitem::add(state,generation),{icon(Icon::Add,"icon")}span{class:"slack-list-subitem-label","Add subitem"}}}
        if incomplete{p{class:"slack-list-help","Loaded subitems only. This List is incomplete."}}
        if let Some(reason)=reason{p{class:"slack-list-help","{reason}"}}
        div{class:"slack-list-subitems-rows",for row in rows.iter() {
            button{key:"{row.id}",r#type:"button",class:"slack-list-action","data-subitem-id":"{row.id}",disabled:!row.available||operation.is_some(),onclick:{let id=row.id.clone();move |_|crate::slack_lists::open_subitem(state,generation,id.clone())},
                {icon(Icon::ChevronRight,"icon")}span{class:"slack-list-subitem-label",if row.title.trim().is_empty(){"Untitled subitem"}else{"{row.title}"}}}
        }}
        if let Some(o)=operation{div{id:"slack-list-subitem-status",class:"slack-list-subitem-status",tabindex:"-1",role:"status","data-subitem-stage":format!("{:?}",o.stage),"data-subitem-serial":o.serial,
            p{"{o.message}"}
            div{class:"slack-list-subitems-actions",
                button{id:"slack-list-subitem-check",r#type:"button",disabled:!o.can_check,onclick:move |_|crate::slack_lists::subitem::check(state,o.serial),"Check current subitems"}
                if o.stage==crate::slack_lists::subitem::Stage::Available{button{id:"slack-list-subitem-open",r#type:"button",disabled:!o.can_open,onclick:move |_|crate::slack_lists::subitem::open(state,o.serial),"Open added subitem"}}
                button{id:"slack-list-subitem-dismiss",r#type:"button",disabled:!o.can_close,onclick:move |_|crate::slack_lists::subitem::dismiss(state,o.serial),
                    if o.stage==crate::slack_lists::subitem::Stage::Preparing{"Cancel"}else if o.stage==crate::slack_lists::subitem::Stage::NotRequested{"Close"}else{"I reviewed this; close receipt"}}
            }
        }}
    }}
}
const FOCUS: &str = include_str!("subitem_focus.js");

pub(super) fn retained_status(state: Signal<ShellState>) -> Element {
    if !crate::slack_lists::subitem::retained_review_visible(&state.read()) {
        return rsx! {};
    }
    let Some(o) = crate::slack_lists::subitem::presentation(&state.read()) else {
        return rsx! {};
    };
    rsx! {div{id:"slack-list-subitem-retained",class:"slack-list-create-status",role:"status",
        p{"{o.message}"}
        p{"The original parent is not open and available. This receipt never retries the request."}
        if !o.pending && !o.stage.busy(){button{r#type:"button",class:"slack-list-action",id:"slack-list-subitem-review-retained",onclick:move |_|crate::slack_lists::subitem::review_retained(state,o.serial),"Review retained request"}}
        button{r#type:"button",class:"slack-list-action",id:"slack-list-subitem-dismiss-retained",disabled:!o.can_close,onclick:move |_|crate::slack_lists::subitem::dismiss(state,o.serial),
            if o.stage==crate::slack_lists::subitem::Stage::Preparing{"Cancel"}else if o.stage==crate::slack_lists::subitem::Stage::NotRequested{"Close"}else{"I reviewed this; dismiss without retry"}}
    }}
}
