//! A subordinate dialog; the original Search overlay/Book never changes route.
use super::*;
use crate::bootstrap::search_message_pages::reaction_actions as actions;
pub(crate) fn panel(state: Signal<ShellState>) -> Element {
    let Some(p) = actions::presentation(&state.read()) else {
        return rsx! {};
    };
    rsx! {LocalPicker{key:"{p.id}",state,id:p.id}}
}
#[component]
fn LocalPicker(state: Signal<ShellState>, id: u64) -> Element {
    let Some(p) = actions::presentation(&state.read()).filter(|p| p.id == id) else {
        return rsx! {};
    };
    let config = serde_json::json!({"id":id,"opener":p.opener});
    rsx! {div{id:"search-reaction-picker",style:"display:contents","data-search-reaction-picker":"{id}",
        onmounted:move |_|{dioxus::document::eval(&format!("{}\nmountSearchReactionPicker({config});",include_str!("reaction_picker.js")));},
        if p.terminal{
            div{class:"scrim",onclick:move |_|actions::close(state,id)}
            section{id:"search-reaction-dialog",class:"modal emoji-picker",role:"dialog","aria-modal":"true","aria-labelledby":"search-reaction-title",
                tabindex:"-1","data-keyboard-dialog":"true","data-dialog-kind":"other",
                onkeydown:move|e|if e.key()==Key::Escape{e.prevent_default();e.stop_propagation();actions::close(state,id);},
                header{h2{id:"search-reaction-title","Reaction result"}
                    button{"aria-label":"Close",onclick:move |_|actions::close(state,id),{crate::icons::icon(crate::icons::Icon::Close,"icon")}}
                }
                p{class:"results-empty",role:"status",{p.notice.unwrap_or_default()}}
                div{class:"search-footer",
                    button{class:"chip",disabled:p.checking,onclick:move |_|actions::check(state,id),
                        if p.checking{"Checking…"}else{"Check current reactions"}
                    }
                    button{class:"chip",onclick:move |_|actions::close(state,id),"Close"}
                }
            }
        }else{
            crate::reaction_picker::EmojiPicker{
                state,title:"Add or remove a reaction".to_owned(),require_preferences:true,dialog_id:"search-reaction-dialog".to_owned(),
                choice_ready:p.ready,choice_status:p.notice,
                onchoose:move|picked|actions::choose(state,id,picked),onclose:move |_|actions::close(state,id),
            }
        }
    }}
}
