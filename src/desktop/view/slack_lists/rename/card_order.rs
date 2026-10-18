use super::*;
pub(super) fn panel(state: Signal<ShellState>, view: rename::Presentation) -> Element {
    let serial = view.serial;
    let visit = crate::slack_lists::generation(&state.read()).unwrap_or_default();
    let stage = format!("{:?}", view.stage);
    let saved_view = view.view.clone().unwrap_or_default();
    let busy = matches!(
        view.stage,
        Stage::Preparing | Stage::Saving | Stage::Checking
    );
    let closable = view.stage == Stage::NotRequested
        || matches!(view.stage, Stage::Observed | Stage::Conflict) && view.current_review;
    rsx! {{std::iter::once(rsx!{
        div{key:"card-order-{serial}",class:"slack-list-editor-shade"}
        section{id:"slack-list-rename",class:"slack-list-cell-editor",role:"dialog","aria-modal":"true","aria-labelledby":"slack-list-rename-title","data-keyboard-dialog":"true",
            "data-card-order":"true","data-rename-serial":"{serial}","data-rename-stage":"{stage}","data-rename-view":"{saved_view}",
            onmounted:move |_|{dioxus::document::eval(&format!("{}\n{}",include_str!("../rename_restore.js"),include_str!("../rename_focus.js")));},
            h3{id:"slack-list-rename-title","Move item"}
            div{class:"slack-list-editor-body",
                p{id:"slack-list-rename-message",class:"slack-list-editor-hint",role:"status","{view.message}"}
                if matches!(view.stage,Stage::Observed|Stage::Conflict){p{id:"slack-list-rename-current",class:"slack-list-editor-hint","Current fields: {view.current_fields}"}}
                if view.blocked{p{class:"slack-list-warning","Slack asked us to wait. Checking and changes are temporarily unavailable."}}
            }
            div{class:"slack-list-editor-actions",
                button{id:"slack-list-rename-leave",r#type:"button",onclick:move |_|{let mut state=state;crate::slack_lists::close_shell(&mut state.write());},"Close list"}
                if closable {button{id:"slack-list-rename-close",r#type:"button",onclick:move |_|super::close(state,serial,visit),if view.stage==Stage::Conflict{"Close review"}else{"Done"}}}
                if view.checkable{button{id:"slack-list-rename-check",r#type:"button",class:"primary",disabled:!view.can_check,onclick:move |_|rename::check(state,serial),"Check current items"}}
                if busy{span{role:"status","Please wait..."}}
            }
        }
    })}}
}
