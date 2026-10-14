//! hGZA input-block mode publishes local selections inline, without an action.
use super::user_choices::{SelectedUsers, UserChoices};
use crate::{app_modal::users, state::ShellState};
use dioxus::prelude::*;
use super_platinum_core::block_actions::form::{FormField, InputKey, InputKind, InputValue};

#[component]
pub(crate) fn ModalUsers(
    state: Signal<ShellState>,
    input_key: InputKey,
    field: FormField,
    id: String,
    locked: bool,
) -> Element {
    let mut suppress_focus = use_signal(|| false);
    let shell = state.read();
    let ticket = users::Ticket::capture(&shell, &input_key);
    let search = ticket.as_ref().and_then(|t| users::presentation(&shell, t));
    let channel = matches!(
        field.kind,
        InputKind::Channel | InputKind::MultiChannel { .. }
    );
    let conversation = channel
        || matches!(
            field.kind,
            InputKind::Conversation | InputKind::MultiConversation { .. }
        );
    let unavailable = users::unavailable(&shell, &field.raw);
    let multiple = matches!(
        field.kind,
        InputKind::MultiUser { .. }
            | InputKind::MultiChannel { .. }
            | InputKind::MultiConversation { .. }
    );
    let values = match &field.value {
        InputValue::Users(values)
        | InputValue::Channels(values)
        | InputValue::Conversations(values) => values.clone(),
        InputValue::User(value) | InputValue::Conversation(value) | InputValue::Channel(value) => {
            value.iter().cloned().collect()
        }
        _ => vec![],
    };
    let selected = ticket
        .as_ref()
        .map(|t| users::labels(&shell, t, &values))
        .unwrap_or_else(|| values.into_iter().map(|id| (id.clone(), id)).collect());
    let disabled = locked || ticket.as_ref().is_none_or(|t| !t.current(&shell));
    drop(shell);
    let open_ticket = ticket.clone();
    let click_ticket = ticket.clone();
    let input_ticket = ticket.clone();
    let remove_ticket = ticket.clone();
    let key_ticket = ticket.clone();
    let label_id = format!("{id}-label");
    let described_by = format!("{id}-hint {id}-error");
    let expanded = search.is_some();
    let generation = ticket.as_ref().map_or(0, users::Ticket::generation);
    let query_value = search.as_ref().map(|s| s.query.clone()).unwrap_or_default();
    let placeholder = field.placeholder.clone().unwrap_or_else(|| {
        if channel {
            if multiple {
                "Select channels"
            } else {
                "Select a channel"
            }
        } else if conversation {
            if multiple {
                "Select conversations"
            } else {
                "Select a conversation"
            }
        } else if multiple {
            "Select users"
        } else {
            "Select a user"
        }
        .into()
    });
    let focus_id = id.clone();
    let collapse_id = id.clone();
    let done_id = id.clone();
    let choice_id = id.clone();
    let remove_id = id.clone();
    rsx! {
        div {class:"app-modal-users","data-modal-users":input_key.block_id.clone(),"data-users-generation":generation.to_string(),
            // The native window Escape listener runs before Rust receives the
            // bubbling event. Mark only expanded choices as a local Escape scope.
            "data-escape-local":expanded.then_some("true"),
            onkeydown:move |event|{
                if event.key()==Key::Escape && expanded {
                    event.prevent_default();event.stop_propagation();
                    if let Some(ticket)=key_ticket.clone(){collapse_selection(state,ticket,suppress_focus,&collapse_id);}
                }else if matches!(event.key(),Key::ArrowDown|Key::ArrowUp) && expanded {
                    event.prevent_default();event.stop_propagation();
                    let delta=if event.key()==Key::ArrowDown{1}else{-1};
                    dioxus::document::eval(&format!("(()=>{{const d=document.getElementById('{}')?.closest('.app-modal-users');const es=[d?.querySelector('input'),...d?.querySelectorAll('[data-entity-id]:not(:disabled)')||[]].filter(Boolean);const i=es.indexOf(document.activeElement);es[(i+{delta}+es.length)%es.length]?.focus();}})();",focus_id));
                }
            },
            SelectedUsers {selected:selected.clone(),disabled,conversation,channel,removable:multiple||field.optional,onremove:move |id|if let Some(ticket)=remove_ticket.clone(){
                if users::remove(state,ticket.clone(),id) && !multiple {
                    collapse_selection(state,ticket,suppress_focus,&remove_id);
                }
            }}
            input {id:"{id}",r#type:"search",autocomplete:"off",disabled,placeholder:"{placeholder}",
                value:query_value,
                "aria-labelledby":"{label_id}","aria-describedby":"{described_by}","aria-invalid":field.error.is_some().to_string(),
                "aria-required":(!field.optional).to_string(),"aria-expanded":expanded.to_string(),
                onfocus:move |_|{if *suppress_focus.peek(){suppress_focus.set(false);}else if let Some(ticket)=open_ticket.clone(){users::open(state,ticket);}},
                onclick:move |_|if let Some(ticket)=click_ticket.clone(){users::open(state,ticket);},
                oninput:move |event|if let Some(ticket)=input_ticket.clone(){users::search(state,ticket,event.value());},
                onkeydown:move |event|if event.key()==Key::Enter {event.prevent_default();event.stop_propagation();}
            }
            if let Some(reason)=unavailable {p {class:"app-modal-hint",role:"status","{reason}"}}
            if let (Some(search),Some(ticket))=(search,ticket) {
                {let serial=search.serial;let revision=search.query_revision;let choice_ticket=ticket.clone();let retry_ticket=ticket.clone();let done_ticket=ticket;
                    rsx!{
                        UserChoices {choices:search.candidates,selected,multiple,conversation,channel,disabled,
                            onchoose:move |id|{
                                if users::choose(state,choice_ticket.clone(),serial,revision,id) && !multiple {
                                    collapse_selection(state,choice_ticket.clone(),suppress_focus,&choice_id);
                                }
                            }}
                        p {class:"app-modal-hint",role:"status",
                            if search.loading {"Searching…"}else if let Some(error)=&search.error {"{error}"}}
                        if search.agent_unavailable {p {class:"app-modal-hint","Some app users are unavailable in this picker."}}
                        div {class:"app-modal-users-actions",
                            if search.error.is_some() {button {r#type:"button",disabled:search.loading,onclick:move |_|users::search(state,retry_ticket.clone(),search.query.clone()),"Retry search"}}
                            button {r#type:"button",onclick:move |_|collapse_selection(state,done_ticket.clone(),suppress_focus,&done_id),"Done selecting"}
                        }
                    }
                }
            }
        }
    }
}

fn collapse_selection(
    state: Signal<ShellState>,
    ticket: users::Ticket,
    mut suppress_focus: Signal<bool>,
    id: &str,
) {
    if !ticket.current(&state.peek()) {
        return;
    }
    let generation = ticket.generation();
    suppress_focus.set(true);
    users::collapse(state, ticket);
    // Force one focus event even when Escape originated in the search field.
    // The one-shot suppression consumes it without reopening the choice list.
    let id = serde_json::to_string(id).expect("DOM id");
    dioxus::document::eval(&format!(
        "(()=>{{const e=document.getElementById({id});if(e?.closest('.app-modal-users')?.dataset.usersGeneration!=='{generation}')return;e.blur();e.focus({{preventScroll:true}});}})();"
    ));
}
