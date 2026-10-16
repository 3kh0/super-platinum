//! One explicit destination modal, mounted in the existing cell-editor slot.
use super::*;
use crate::slack_lists::{EditorView, board_move};

#[component]
pub(super) fn MoveAction(
    state: Signal<ShellState>,
    instance: String,
    row: String,
    label: String,
    visit: u64,
    layout: u64,
) -> Element {
    let shell = state.read();
    if !board_move::supported(&shell) {
        return rsx! {};
    }
    let disabled = !board_move::can_open(&shell, &instance);
    rsx! {button{r#type:"button",class:"slack-list-action slack-list-board-move","data-board-move":"true",disabled,"aria-label":"Move {label} to lane",title:"Move this item to another lane",
    onclick:move |_|{
        let input=serde_json::json!({"instance":instance,"row":row,"visit":visit,"layout":layout}).to_string();let instance=instance.clone();
        spawn(async move {
            let mut eval=dioxus::document::eval(&format!("dioxus.send(({FOCUS}).capture({input}));await dioxus.recv();"));
            let captured=eval.recv::<bool>().await.unwrap_or(false);let _=eval.send(());
            if captured {board_move::open(state,instance,visit,layout);}
        });
    },"Move"}}
}
#[component]
pub(super) fn MoveEditor(state: Signal<ShellState>, editor: EditorView, visit: u64) -> Element {
    let seed = board_move::key(editor.original.as_ref())
        .unwrap_or_default()
        .to_owned();
    let mut destination = use_signal(move || seed);
    let generation = editor.generation;
    let layout = editor.layout_generation;
    let row = editor.row_id.clone();
    let value = board_move::destination(&editor.original, &destination.read());
    let no_change = value.as_ref() == Some(editor.original.as_ref());
    let disabled = editor.disabled;
    let busy = editor.saving;
    let save_disabled = disabled || editor.stale || no_change || value.is_none();
    let mount = serde_json::json!({"editor":generation,"visit":visit,"layout":layout,"row":row});
    rsx! {
        div{class:"slack-list-editor-shade"}
        section{id:"slack-list-cell-editor",class:"slack-list-cell-editor",role:"dialog","aria-modal":"true","aria-labelledby":"slack-list-editor-title","aria-busy":busy,"data-keyboard-dialog":"true","data-escape-local":"true","data-editor-generation":generation,"data-board-move-editor":"true",tabindex:"-1",
            onmounted:move |_|{dioxus::document::eval(&format!("({FOCUS}).mount({mount});"));},
            onkeydown:move |e|if e.key()==Key::Escape{e.prevent_default();e.stop_propagation();crate::slack_lists::cancel_editor(state,generation);},
            h3{id:"slack-list-editor-title","Move to lane"}
            p{class:"slack-list-editor-item","{editor.row_label}"}
            div{class:"slack-list-editor-body",
                label{r#for:"slack-list-board-destination","Destination"}
                select{id:"slack-list-board-destination",value:"{destination}",disabled,
                    onchange:move |e|destination.set(e.value()),
                    for choice in editor.choices.iter(){option{key:"{choice.id}",value:"{choice.id}",selected:*destination.read()==choice.id,"{choice.label}"}}
                }
                p{class:"slack-list-editor-hint","Changes this item's grouping value for everyone. Saved filters and sorting still apply; this does not choose a card position."}
            }
            if let Some(message)=&editor.error {p{class:"slack-list-error",role:"alert","{message}"}}
            div{class:"slack-list-editor-actions",
                if editor.stale {button{id:"slack-list-editor-refresh",r#type:"button",disabled:busy,onclick:move |_|crate::slack_lists::refresh(state),"Check current"}}
                button{id:"slack-list-editor-cancel",r#type:"button",onclick:move |_|crate::slack_lists::cancel_editor(state,generation),if editor.stale||busy{"Close"}else{"Cancel"}}
                button{id:"slack-list-editor-save",r#type:"button",class:"primary",disabled:save_disabled,
                    onclick:move |_|{if save_disabled{return;}if let Some(value)=board_move::destination(&editor.original,&destination.read()){crate::slack_lists::save_editor(state,generation,layout,value);}},
                    if busy{"Moving…"}else{"Move"}
                }
            }
        }
    }
}
#[component]
pub(super) fn MoveReturn(state: Signal<ShellState>, receipt: board_move::Return) -> Element {
    let serial = receipt.serial;
    let input = serde_json::to_string(&receipt).unwrap();
    rsx! {p{id:"slack-list-board-move-return",class:"slack-list-editor-hint",role:"status","data-origin-editor":receipt.editor,"data-return-serial":serial,
        onmounted:move |_|{
            let input=input.clone();spawn(async move {
                let mut eval=dioxus::document::eval(&format!("requestAnimationFrame(()=>dioxus.send(({FOCUS}).returned({input})));await dioxus.recv();"));
                let value=eval.recv::<Option<bool>>().await.ok().flatten();let _=eval.send(());
                if let Some(fallback)=value {board_move::focus_result(state,serial,fallback);}
            });
        },
        if receipt.fallback==Some(true){"Lane change observed. Use Board controls to find or open the card."}
        else {"Lane change observed. Current saved filters and sorting apply."}
    }}
}
const FOCUS: &str = include_str!("board_move.js");
