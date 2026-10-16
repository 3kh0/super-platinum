use super::*;
use crate::slack_lists::rename::text_create::{
    Choice,
    select::{self as draft, Draft, Edit},
};
pub(super) fn fields(
    state: Signal<ShellState>,
    serial: u64,
    visit: u64,
    selection: u64,
    choice: Choice,
    draft: Draft,
    enabled: bool,
) -> Element {
    let epoch = draft.add_epoch;
    let can_add = enabled && draft.can_add();
    rsx! {
        fieldset { class:"slack-list-select-create", id:"slack-list-select-options", disabled:!enabled,
            legend {"Options"}
            p {class:"slack-list-editor-hint","Add 1-8 options with short labels. Items stay empty until you choose an option."}
            for row in draft.rows {
                div {key:"{row.generation}",class:"slack-list-select-option", "data-option-id":"{row.option.value()}","data-option-generation":"{row.generation}","data-option-color":"{row.option.color()}",
                    span {"{row.option.label()}"}
                    button {r#type:"button",class:"slack-list-action",disabled:!enabled,"aria-label":"Remove option {row.option.label()}",
                        onclick:move |_|draft::edit(state,serial,visit,selection,choice,Edit::Remove{row:row.generation}),"Remove"}
                }
            }
            label {r#for:"slack-list-select-option-label","New option"}
            div {class:"slack-list-select-option-input",
                input {id:"slack-list-select-option-label",r#type:"text",value:"{draft.pending}",maxlength:1024,disabled:!enabled,"data-add-epoch":"{epoch}",
                    oninput:move |event|draft::edit(state,serial,visit,selection,choice,Edit::Pending{epoch,value:event.value()})}
                button {id:"slack-list-select-option-add",r#type:"button",class:"slack-list-action",disabled:!can_add,
                    onclick:move |_|draft::edit(state,serial,visit,selection,choice,Edit::Add{epoch}),"Add option"}
            }
            if let Some(error)=draft.error {p{class:"slack-list-editor-hint",role:"status","{error}"}}
        }
    }
}

#[cfg(test)]
mod tests;
