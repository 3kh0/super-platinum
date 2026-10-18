use super::*;
use crate::slack_lists::rename::group_delete;
pub(super) fn action(state: Signal<ShellState>, value: &str) -> Element {
    let Some(control) = rename::board_choice::control(
        &state.read(),
        Some(value),
        rename::board_choice::Kind::Rename,
    ) else {
        return rsx! {};
    };
    let action = group_delete::available(&state.read(), value);
    let disabled = action.is_none();
    rsx! {button{disabled,r#type:"button",class:"slack-list-action","data-board-group-delete":"{value}","data-board-group-delete-view":"{control.view}",
    onmounted:move |_|{dioxus::document::eval(include_str!("group_delete_return.js"));},
        onclick:move |event|{event.stop_propagation();if let Some(action)=action.clone(){group_delete::open(state,action);}},"Delete"}}
}
pub(super) fn panel(
    state: Signal<ShellState>,
    view: rename::Presentation,
    deletion: group_delete::Presentation,
) -> Element {
    let serial = view.serial;
    let visit = crate::slack_lists::generation(&state.read()).unwrap_or_default();
    let editing = view.stage == Stage::Editing;
    let busy = matches!(
        view.stage,
        Stage::Preparing | Stage::Saving | Stage::Checking
    );
    let closable = matches!(view.stage, Stage::Editing | Stage::NotRequested)
        || matches!(view.stage, Stage::Observed | Stage::Conflict) && view.current_review;
    let stage = format!("{:?}", view.stage);
    let saved = view.view.clone().unwrap_or_default();
    rsx! {{std::iter::once(rsx! {
        div{key:"group-delete-{serial}",class:"slack-list-editor-shade"}
        section{id:"slack-list-rename",class:"slack-list-cell-editor",role:"dialog","aria-modal":"true","aria-labelledby":"slack-list-rename-title","data-keyboard-dialog":"true",
            "data-group-delete":"{deletion.value}","data-rename-serial":"{serial}","data-rename-stage":"{stage}","data-rename-view":"{saved}",
            onmounted:move |_|{dioxus::document::eval(&format!("{}\n{}\n{}\nwindow.__listBoardGroupDeleteReturn?.mount(document.getElementById('slack-list-rename'));",include_str!("group_delete_return.js"),include_str!("../rename_restore.js"),include_str!("../rename_focus.js")));},
            h3{id:"slack-list-rename-title","Delete group"}
            div{class:"slack-list-editor-body",
                p{class:"slack-list-editor-item","Group: {deletion.label}"}
                p{class:"slack-list-editor-hint","Remove this group option and clear it from {deletion.matching} matching items in the current loaded view. The items themselves remain."}
                p{class:"slack-list-editor-hint","The option and item changes can finish separately. You can check both results here."}
                if let Some(status)=deletion.schema{p{id:"slack-list-group-delete-schema",class:"slack-list-editor-hint","Group option: {status}"}}
                if let Some(status)=deletion.cells{p{id:"slack-list-group-delete-cells",class:"slack-list-editor-hint","Matching items: {status}"}}
                p{id:"slack-list-rename-message",class:"slack-list-editor-hint",role:"status","{view.message}"}
                if matches!(view.stage,Stage::Observed|Stage::Conflict){p{id:"slack-list-rename-current",class:"slack-list-editor-hint","Current fields: {view.current_fields}"}}
                if view.blocked{p{class:"slack-list-warning","Slack asked us to wait. Checking and changes are temporarily unavailable."}}
            }
            div{class:"slack-list-editor-actions",
                button{id:"slack-list-rename-leave",r#type:"button",onclick:move |_|{let mut state=state;crate::slack_lists::close_shell(&mut state.write());},"Close list"}
                if closable{button{id:"slack-list-rename-close",r#type:"button",onclick:move |_|super::close(state,serial,visit),if editing{"Cancel"}else if view.stage==Stage::Conflict{"Close review"}else{"Done"}}}
                if editing{button{id:"slack-list-rename-confirm",r#type:"button",class:"primary",disabled:!view.can_confirm,onclick:move |_|group_delete::confirm(state,serial,visit),"Delete group"}}
                if view.checkable{button{id:"slack-list-rename-check",r#type:"button",class:"primary",disabled:!view.can_check,onclick:move |_|rename::check(state,serial),"Check groups and items"}}
                if busy{span{role:"status","Please wait..."}}
            }
        }
    })}}
}
