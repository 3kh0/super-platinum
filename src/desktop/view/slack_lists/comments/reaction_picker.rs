//! The shared callback dialog stays inside the original List overlay.
use super::*;
use crate::slack_lists::comments::reaction_actions::selection;

pub(super) fn panel(state: Signal<ShellState>) -> Element {
    let Some((generation, token, _, _)) = selection::presentation(&state.read()) else {
        return rsx! {};
    };
    rsx! {LocalPicker {key:"{generation}:{token}",state,generation,token}}
}

#[component]
fn LocalPicker(state: Signal<ShellState>, generation: u64, token: u64) -> Element {
    use_effect(move || {
        let shell = state.read();
        let ready = selection::needs_binding(&shell, generation, token);
        drop(shell);
        if ready {
            selection::bind(state, generation, token);
        }
    });
    let Some((current, active, ready, notice)) = selection::presentation(&state.read()) else {
        return rsx! {};
    };
    if current != generation || active != token {
        return rsx! {};
    }
    let ts = selection::target_ts(&state.peek(), generation, token).unwrap_or_default();
    let options = serde_json::json!({"generation":generation,"token":token,"ts":ts});
    rsx! {div {
        onmounted:move |_| {dioxus::document::eval(&format!("{}\nmountListReactionPicker({options});",include_str!("reaction_picker.js")));},
        id:"slack-list-reaction-picker",style:"display:contents","data-reaction-picker":"{token}",
        crate::reaction_picker::EmojiPicker {
            state,title:"Add a reaction".to_owned(),require_preferences:true,dialog_id:"slack-list-reaction-dialog".to_owned(),
            choice_ready:ready,choice_status:notice,
            onchoose:move |picked|selection::choose(state,generation,token,picked),
            onclose:move |_|selection::close(state,generation,token),
        }
    }}
}
