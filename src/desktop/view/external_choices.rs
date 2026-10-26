//! Shared app-option search and grouped rows; no directory ranking or filtering.
use crate::{
    icons::{Icon, icon},
    message_buttons::external::{self, Owner},
    state::ShellState,
};
use dioxus::prelude::*;

#[component]
pub(crate) fn ExternalChoices(
    state: Signal<ShellState>,
    owner: Owner,
    id: String,
    inline_done: bool,
    onclose: EventHandler<()>,
) -> Element {
    let shell = state.read();
    let Some(query) = owner.query(&shell) else {
        return rsx! {};
    };
    let text = query.text.clone();
    let revision = query.revision;
    let loading = query.loading;
    let action_locked = matches!(&owner, Owner::Home(t) if t.action_disabled(&shell));
    let choices_locked = action_locked || (loading && !owner.retains_choices());
    let too_long = query.text.encode_utf16().count() > 4096;
    let error = query.error.clone();
    let eligible = query.eligible();
    let minimum = query.minimum;
    let rows = query
        .rows
        .iter()
        .take(query.visible)
        .cloned()
        .collect::<Vec<_>>();
    let more = rows.len() < query.rows.len();
    let retryable = query.retryable();
    let selected = owner.selected(&shell);
    let multiple = selected.is_some();
    drop(shell);
    let input_owner = owner.clone();
    let retry_owner = owner.clone();
    let retry_text = text.clone();
    let more_owner = owner.clone();
    let focus_id = id.clone();
    rsx! {
        div {class:"app-modal-users","data-external-search":"true","data-query-revision":revision.to_string(),"data-escape-local":"true",
            onkeydown:move |event|if event.key()==Key::Escape{event.prevent_default();event.stop_propagation();onclose.call(());},
            if let Some(selected)=selected.as_ref() {
                div {class:"message-multi-selected",role:"group","aria-label":"Selected options",
                    for option in selected {
                        {let remove_owner=owner.clone();let value=option.value().to_owned();rsx!{
                            div {key:"selected-{value}",class:"message-multi-selected-option",span {"{option.text()}"}
                                button {r#type:"button",class:"icon-btn",disabled:action_locked,"aria-label":"Remove {option.text()}","data-option-value":value.clone(),
                                    onclick:move |_|external::remove(state,remove_owner.clone(),value.clone()),{icon(Icon::Close,"icon")}}
                            }
                        }}
                    }
                }
            }
            div {class:if inline_done{"app-modal-users-actions external-search-controls"}else{"external-search-controls"},
                input {id:"{id}",class:"external-option-search",r#type:"search",autocomplete:"off",disabled:action_locked,value:"{text}",placeholder:"Find an option","aria-label":"Find an option","aria-busy":loading.to_string(),
                    onmounted:move |_|{dioxus::document::eval(&format!("document.getElementById({})?.focus({{preventScroll:true}});",serde_json::to_string(&focus_id).unwrap()));},
                    oninput:move |event|external::search(state,input_owner.clone(),event.value(),false,false)
                }
                if inline_done {button {r#type:"button","data-external-done":"true",onclick:move |_|onclose.call(()),"Done"}}
            }
            div {class:"message-multi-status",role:if error.is_some(){"alert"}else{"status"},
                if let Some(error)=error.as_ref(){"{error}"}
                else if too_long{"The search is too long. Use at most 4096 characters."}
                else if !eligible{"Type at least {minimum} characters to find options"}
                else if loading{"Loading options…"}
                else if rows.is_empty(){"No matching options"}
            }
            if error.is_some(){button {r#type:"button","data-external-retry":"true",disabled:action_locked||!retryable,onclick:move |_|external::search(state,retry_owner.clone(),retry_text.clone(),true,true),"Retry search"}}
            div {class:"message-user-options",role:"group","aria-label":"Options",
                for (index,row) in rows.iter().enumerate(){
                    {let choice_owner=owner.clone();let group=row.group.as_deref().filter(|_|index==0||rows[index-1].group!=row.group);
                    rsx!{div {key:"{revision}-{index}",class:"message-multi-entry",
                        if let Some(group)=group {div {class:"message-multi-group","{group}"}}
                        if multiple {
                            {let option_id=format!("{id}-option-{revision}-{index}");let checked=vec![Attribute::new("checked",selected.as_ref().is_some_and(|items|items.iter().any(|o|o.value()==row.option.value())),None,true)];rsx!{
                                label {class:"block-radio-option",r#for:option_id.clone(),
                                    input {id:option_id,r#type:"checkbox",disabled:choices_locked,"data-option-index":index.to_string(),
                                        onchange:move |event|{event.stop_propagation();external::choose(state,choice_owner.clone(),revision,index);},..checked}
                                    span {class:"block-radio-copy",span {class:"block-radio-label","{row.option.text()}"}
                                        if let Some(description)=row.option.description(){span {class:"block-radio-description","{description}"}}
                                    }
                                }
                            }}
                        } else {button {r#type:"button",class:"message-user-option",disabled:choices_locked,"data-option-index":index.to_string(),
                            onclick:move |_|external::choose(state,choice_owner.clone(),revision,index),
                            span {class:"block-radio-copy",span {class:"block-radio-label","{row.option.text()}"}
                                if let Some(description)=row.option.description(){span {class:"block-radio-description","{description}"}}
                            }
                        }}
                    }}}
                }
                if more{button {r#type:"button",onclick:move |_|external::more(state,more_owner.clone()),"Show more options"}}
            }
        }
    }
}
