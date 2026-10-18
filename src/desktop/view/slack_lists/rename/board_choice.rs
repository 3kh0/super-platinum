use super::*;
use crate::slack_lists::rename::board_choice::{self, Kind};
use super_platinum_core::slack_lists::BoardChoiceAction;

pub(in crate::view::slack_lists) fn add(state: Signal<ShellState>) -> Element {
    let Some(choice) = board_choice::control(&state.read(), None, Kind::Add) else {
        return rsx! {};
    };
    let disabled = board_choice::available(&state.read(), None, Kind::Add).is_none();
    rsx! {button{disabled,r#type:"button",class:"slack-list-action","data-board-choice-kind":"add","data-board-choice-value":"","data-board-choice-view":"{choice.view}",
    onmounted:move |_|{dioxus::document::eval(include_str!("board_choice_return.js"));},
    onclick:move |_|board_choice::open(state,choice.clone()),{icon(Icon::Add,"icon")}"Add group"}}
}
pub(in crate::view::slack_lists) fn actions(state: Signal<ShellState>, value: &str) -> Element {
    let name = board_choice::control(&state.read(), Some(value), Kind::Rename);
    let color = board_choice::control(&state.read(), Some(value), Kind::Color);
    let disabled = board_choice::available(&state.read(), Some(value), Kind::Rename).is_none();
    let value = value.to_owned();
    rsx! {if let Some(choice)=name {button{disabled,r#type:"button",class:"slack-list-action","data-board-choice-kind":"name","data-board-choice-value":"{value}","data-board-choice-view":"{choice.view}",
    onmounted:move |_|{dioxus::document::eval(include_str!("board_choice_return.js"));},
    onclick:move |event|{event.stop_propagation();board_choice::open(state,choice.clone());},"Name"}}
    if let Some(choice)=color {button{disabled,r#type:"button",class:"slack-list-action","data-board-choice-kind":"color","data-board-choice-value":"{value}","data-board-choice-view":"{choice.view}",
    onclick:move |event|{event.stop_propagation();board_choice::open(state,choice.clone());},"Color"}}{super::group_delete::action(state,&value)}}
}
pub(super) fn panel(
    state: Signal<ShellState>,
    view: rename::Presentation,
    choice: board_choice::Presentation,
) -> Element {
    let visit = crate::slack_lists::generation(&state.read()).unwrap_or_default();
    let serial = view.serial;
    let stage = format!("{:?}", view.stage);
    let editing = view.stage == Stage::Editing;
    let busy = matches!(
        view.stage,
        Stage::Preparing | Stage::Saving | Stage::Checking
    );
    let closable = matches!(view.stage, Stage::Editing | Stage::NotRequested)
        || matches!(view.stage, Stage::Observed | Stage::Conflict) && view.current_review;
    let (title, kind) = match choice.kind {
        Kind::Add => ("Add group", "add"),
        Kind::Rename => ("Group name", "name"),
        Kind::Color => ("Group color", "color"),
    };
    let (value, label, color) = match &choice.action {
        BoardChoiceAction::Add {
            value,
            label,
            color,
        } => (value.clone(), Some(label.clone()), Some(color.clone())),
        BoardChoiceAction::Rename { value, label } => (value.clone(), Some(label.clone()), None),
        BoardChoiceAction::Color { value, color } => (value.clone(), None, Some(color.clone())),
    };
    let marker = if choice.kind == Kind::Add {
        String::new()
    } else {
        value
    };
    let saved_view = view.view.clone().unwrap_or_default();
    let input_action = choice.action.clone();
    let color_action = choice.action.clone();
    let confirm_action = choice.action;
    rsx! {{std::iter::once(rsx!{
        div{key:"board-choice-{serial}",class:"slack-list-editor-shade"}
        section{id:"slack-list-rename",class:"slack-list-cell-editor",role:"dialog","aria-modal":"true","aria-labelledby":"slack-list-rename-title","data-keyboard-dialog":"true",
            "data-board-choice":"{kind}","data-board-choice-value":"{marker}","data-rename-serial":"{serial}","data-rename-stage":"{stage}","data-rename-view":"{saved_view}",
            onmounted:move |_|{dioxus::document::eval(&format!("{}\n{}\n{}\nwindow.__listBoardChoiceReturn?.mount(document.getElementById('slack-list-rename'));",include_str!("board_choice_return.js"),include_str!("../rename_restore.js"),include_str!("../rename_focus.js")));},
            h3{id:"slack-list-rename-title","{title}"}
            div{class:"slack-list-editor-body",
                if choice.kind!=Kind::Add {p{class:"slack-list-editor-item","Original group: {view.before}"}}
                if let Some(label)=label {
                    label{r#for:"slack-list-rename-name","Group name"}
                    input{id:"slack-list-rename-name",r#type:"text",value:"{label}",maxlength:256,disabled:!editing||!view.can_confirm,
                        oninput:move |event|board_choice::input(state,serial,visit,input_action.clone(),Some(event.value()),None)}
                }
                if let Some(color)=color {
                    label{r#for:"slack-list-board-choice-color","Color"}
                    select{id:"slack-list-board-choice-color",value:"{color}",disabled:!editing||!view.can_confirm,
                        onchange:move |event|board_choice::input(state,serial,visit,color_action.clone(),None,Some(event.value())),
                        for (value,name) in [("indigo","Indigo"),("blue","Lagoon"),("cyan","Cyan"),("pink","Pink"),("green","Grass"),("gray","Gray"),("purple","Purple"),("orange","Honeycomb"),("brown","Horchata")] {
                            option{value,selected:color==value,"{name}"}
                        }
                    }
                }
                p{id:"slack-list-rename-message",class:"slack-list-editor-hint",role:"status","{view.message}"}
                if matches!(view.stage,Stage::Observed|Stage::Conflict){p{id:"slack-list-rename-current",class:"slack-list-editor-hint","Current fields: {view.current_fields}"}}
                if matches!(view.stage,Stage::Observed|Stage::Conflict){
                    if let Some((name,color))=choice.current {p{id:"slack-list-board-choice-current",class:"slack-list-editor-hint","Current group option: {name}; color: {color}"}}
                    else {p{id:"slack-list-board-choice-current",class:"slack-list-editor-hint","The group option is unavailable in the current schema."}}
                }
                if view.blocked{p{class:"slack-list-warning","Slack asked us to wait. Checking and changes are temporarily unavailable."}}
            }
            div{class:"slack-list-editor-actions",
                button{id:"slack-list-rename-leave",r#type:"button",onclick:move |_|{let mut state=state;crate::slack_lists::close_shell(&mut state.write());},"Close list"}
                if closable{button{id:"slack-list-rename-close",r#type:"button",onclick:move |_|super::close(state,serial,visit),if editing{"Cancel"}else if view.stage==Stage::Conflict{"Close review"}else{"Done"}}}
                if editing{button{id:"slack-list-rename-confirm",r#type:"button",class:"primary",disabled:!view.can_confirm||!choice.ready,
                    onclick:move |_|board_choice::confirm(state,serial,visit,confirm_action.clone()),"Save for everyone"}}
                if view.checkable{button{id:"slack-list-rename-check",r#type:"button",class:"primary",disabled:!view.can_check,onclick:move |_|rename::check(state,serial),"Check current groups"}}
                if busy{span{role:"status","Please wait..."}}
            }
        }
    })}}
}
