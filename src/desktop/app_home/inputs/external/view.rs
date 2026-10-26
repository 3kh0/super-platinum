use super::*;
use crate::icons::{Icon, icon};
use super_platinum_core::block_actions::form::FormField;

#[component]
pub(crate) fn HomeExternal(
    state: Signal<ShellState>,
    input_key: InputKey,
    field: FormField,
    id: String,
    locked: bool,
) -> Element {
    let s = state.read();
    let ticket = Ticket::capture(&s, &input_key);
    let available = ticket.is_some();
    let query_owner = owner(&s, &input_key);
    let locked = locked || dispatch::disabled(&s, &input_key);
    let selected = dispatch::display(&s, &input_key).flatten();
    let multiple = multiple::visible(&s, &input_key);
    let selection_status = multiple::presentation(&s, &input_key);
    let prop_revision = multiple::pending_revision(&s, &input_key);
    let rendered_invalid = selection_status.as_ref().map(|p| p.invalid.to_string());
    drop(s);
    // JQ84 submits changed initial selections from its mounted update lifecycle.
    use_effect(use_reactive(
        (&input_key, &prop_revision),
        move |(key, revision)| {
            multiple::flush_pending(state, &key, revision);
        },
    ));
    let expanded = query_owner.is_some();
    let mut was_open = use_signal(|| false);
    use_effect(use_reactive((&expanded, &id), move |(expanded, id)| {
        let restore = *was_open.peek() && !expanded;
        was_open.set(expanded);
        if restore {
            dioxus::document::eval(&format!(
                "(()=>{{const target=document.getElementById({}),before=document.activeElement;const allowed=before===document.body||before?.closest('[data-home-external]')===target?.closest('[data-home-external]');if(!target||!allowed)return;requestAnimationFrame(()=>{{if(target.isConnected&&document.activeElement===before)target.focus({{preventScroll:true}});}});}})();",
                serde_json::to_string(&id).unwrap()
            ));
        }
    }));
    let placeholder = field
        .placeholder
        .clone()
        .unwrap_or_else(|| "Select an option".into());
    let open_ticket = ticket.clone();
    let remove_root = ticket.clone();
    rsx! {div {class:"app-modal-users","data-home-external":input_key.block_id,"aria-invalid":rendered_invalid,
        if !expanded {if let Some(options)=multiple {div {class:"message-multi-selected",role:"group","aria-label":"Selected options",
            for option in options {{let remove_ticket=remove_root.clone();let value=option.value().to_owned();rsx!{
                div {key:"selected-{value}",class:"message-multi-selected-option",span {"{option.text()}"}
                    button {r#type:"button",class:"icon-btn",disabled:locked||!available,"aria-label":"Remove {option.text()}","data-option-value":value.clone(),
                        onclick:move |_|if let Some(t)=remove_ticket.clone(){remove(state,QueryOwner::Home(t),value.clone())},{icon(Icon::Close,"icon")}}
                }
            }}}
        }}}
        if let Some(option)=selected {div {class:"message-multi-selected",div {class:"message-multi-selected-option",
            span {"{option.text()}"}
            if field.optional {button {r#type:"button",class:"icon-btn",disabled:locked||!available,"aria-label":"Clear selected option",
                onclick:move |_|if let Some(ticket)=ticket.clone(){clear(state,ticket)}, {icon(Icon::Close,"icon")}}}
        }}}
        if let Some(owner)=query_owner {{let visit=owner.id(); rsx!{crate::view::external_choices::ExternalChoices {
            key:"external-home-{visit}",state,owner,id:id.clone(),inline_done:true,onclose:move |_|close(state,visit)
        }}}}else{button {id,r#type:"button",class:"block-select block-users-select",disabled:locked||!available,
            "aria-haspopup":"true","aria-label":placeholder.clone(),"data-external-open":"true",
            onclick:move |_|if let Some(ticket)=open_ticket.clone(){open(state,ticket)},span {"{placeholder}"}{icon(Icon::ChevronRight,"icon block-multi-chevron")}
        }}
        if let Some(status)=selection_status {if let Some(text)=status.text {p {class:"message-multi-status",role:if status.warning{"alert"}else{"status"},"data-home-multi-status":"true","data-warning":status.warning.to_string(),"{text}"}}}
        if !available {p {class:"muted","Option lookup is unavailable for this Home context. Your inputs are retained."}}
    }}
}
