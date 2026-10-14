use crate::{
    app_modal::external,
    icons::{Icon, icon},
    state::ShellState,
};
use dioxus::prelude::*;
use super_platinum_core::block_actions::form::{FormField, InputKey, InputValue};
#[component]
pub(crate) fn ModalExternal(
    state: Signal<ShellState>,
    input_key: InputKey,
    field: FormField,
    id: String,
    locked: bool,
) -> Element {
    let shell = state.read();
    let ticket = external::Ticket::capture(&shell, &input_key);
    let available = ticket.is_some();
    let owner = external::owner(&shell, &input_key);
    drop(shell);
    let expanded = owner.is_some();
    let mut was_open = use_signal(|| false);
    use_effect(use_reactive((&expanded, &id), move |(expanded, id)| {
        let restore = *was_open.peek() && !expanded;
        was_open.set(expanded);
        if restore {
            dioxus::document::eval(&format!(
                "(()=>{{const target=document.getElementById({}),before=document.activeElement;const allowed=before===document.body||before?.closest('[data-modal-external]')===target?.closest('[data-modal-external]');if(!target||!allowed)return;requestAnimationFrame(()=>{{if(target.isConnected&&document.activeElement===before)target.focus({{preventScroll:true}});}});}})();",
                serde_json::to_string(&id).unwrap()
            ));
        }
    }));
    let multiple = matches!(field.value, InputValue::ExternalOptions(_));
    let selected = match &field.value {
        InputValue::External(option) => option.iter().cloned().collect::<Vec<_>>(),
        InputValue::ExternalOptions(options) => options.clone(),
        _ => vec![],
    };
    let open_ticket = ticket.clone();
    let placeholder = field.placeholder.as_deref().unwrap_or(if multiple {
        "Select options"
    } else {
        "Select an option"
    });
    rsx! {div {class:"app-modal-users","data-modal-external":input_key.block_id,
        if (!expanded || !multiple) && !selected.is_empty() {div {class:"message-multi-selected",
            for option in selected { {let remove_ticket=ticket.clone(); let value=option.value().to_owned(); rsx!{div {key:"{value}",class:"message-multi-selected-option",span {"{option.text()}"}
                if multiple || field.optional {button {r#type:"button",class:"icon-btn",disabled:locked||!available,"aria-label":if multiple {format!("Remove {}",option.text())}else{"Clear selected option".to_owned()},onclick:move |_|if let Some(ticket)=remove_ticket.clone(){
                    if multiple {external::remove(state,crate::message_buttons::external::Owner::Modal(ticket),value.clone());}
                    else {external::clear(state,ticket);}
                },{icon(Icon::Close,"icon")}}}
            }}}}
        }}
        if let Some(owner)=owner {{let visit=owner.id(); rsx!{super::external_choices::ExternalChoices {key:"external-modal-{owner.id()}",state,owner,id:id.clone(),inline_done:true,onclose:move |_|external::close(state,visit)}
        }}}else{button {id, r#type:"button",class:"block-select block-users-select",disabled:locked||!available,"aria-haspopup":"true","aria-label":placeholder,"data-external-open":"true",
            onclick:move |_|if let Some(ticket)=open_ticket.clone(){external::open(state,ticket)},span {"{placeholder}"}{icon(Icon::ChevronRight,"icon block-multi-chevron")}
        }}
        if !available{p {class:"muted","This app option field is unavailable or has changed"}}
    }}
}
