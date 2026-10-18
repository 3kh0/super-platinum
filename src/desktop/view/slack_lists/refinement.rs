//! Local Layout and completion controls, plus dirty Save/Reset visibility.
use super::*;
use crate::slack_lists::refinement::toolbar;
pub(super) fn controls(state: Signal<ShellState>, _visit: u64) -> Element {
    let Some(p) = toolbar::presentation(&state.read()) else {
        return rsx! {};
    };
    let stamp = p.stamp;
    let layout_stamp = stamp.clone();
    let completed_stamp = stamp.clone();
    let save_stamp = stamp.clone();
    let new_stamp = stamp.clone();
    rsx! {
        label{class:"slack-list-view-picker",r#for:"slack-list-refinement-layout",span{"Layout"}
            select{id:"slack-list-refinement-layout",value:p.layout,disabled:!p.enabled,onchange:move |event|toolbar::layout(state,layout_stamp.clone(),event.value()),
                option{value:"table",selected:p.layout=="table","Table"}
                option{value:"board",selected:p.layout=="board","Board"}
            }
        }
        if p.completion {
            label{class:"slack-list-view-picker",r#for:"slack-list-refinement-completed",
                input{id:"slack-list-refinement-completed",r#type:"checkbox",checked:p.show_completed,disabled:!p.enabled,onchange:move |event|toolbar::completed(state,completed_stamp.clone(),event.checked())}
                span{"Show completed"}
            }
        }
        if p.save_visible {
            if p.save_existing {button{id:"slack-list-refinement-save",r#type:"button",class:"slack-list-action",disabled:!p.enabled,onclick:move |_|toolbar::save(state,save_stamp.clone(),false),"Save for everyone"}}
            button{id:"slack-list-refinement-save-new",r#type:"button",class:"slack-list-action",disabled:!p.enabled,onclick:move |_|toolbar::save(state,new_stamp.clone(),true),"Save as new view"}
            button{id:"slack-list-refinement-reset",r#type:"button",class:"slack-list-action",disabled:!p.enabled,onclick:move |_|toolbar::reset_all(state,stamp.clone()),"Reset"}
        }
    }
}

/// After New's positive ACK the editor is dismissed immediately. Only the
/// independent receipt remains while the selected pending ID is resolved.
pub(super) fn status(state: Signal<ShellState>) -> Element {
    use crate::slack_lists::rename::{self, Stage};
    let Some(view) = rename::presentation(&state.read()) else {
        return rsx! {};
    };
    let Some(save) = view.refinement_save.filter(|s| s.dismissed_on_ack) else {
        return rsx! {};
    };
    let serial = view.serial;
    let busy = matches!(
        view.stage,
        Stage::Preparing | Stage::Saving | Stage::Checking
    );
    let close = matches!(
        view.stage,
        Stage::Observed | Stage::Conflict | Stage::Available
    ) && view.current_review;
    rsx! {div{key:"refinement-status-{serial}",id:"slack-list-refinement-save-status",role:"status",
        "data-refinement-serial":"{serial}",
        onmounted:move |_|{dioxus::document::eval(include_str!("refinement_status_focus.js"));dioxus::document::eval(include_str!("refinement_status_check_focus.js"));},
        p{"{view.message}"}p{"{save.preference}"}
        if busy{span{"Loading saved view metadata…"}}
        if view.checkable{button{id:"slack-list-refinement-status-check",r#type:"button",class:"slack-list-action",disabled:!view.can_check,onclick:move |_|rename::check(state,serial),"Check current view"}}
        if close{button{id:"slack-list-refinement-status-close",r#type:"button",class:"slack-list-action",onclick:move |_|rename::close(state,serial),"Dismiss"}}
    }}
}
