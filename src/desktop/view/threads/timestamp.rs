//! Internal-only ordinary timestamp action; modifier/tile/browser paths excluded.
use super::*;
use crate::thread_feed::timestamp;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use super_platinum_core::slack::models::ThreadViewItem;
pub(super) fn render(
    state: Signal<ShellState>,
    s: &ShellState,
    item: &ThreadViewItem,
    action: Option<timestamp::Action>,
    label: &str,
) -> Element {
    let status = timestamp::status(s, item);
    let busy = status.as_ref().is_some_and(|s| s.0);
    let notice = status.map(|s| s.1).unwrap_or_default();
    let control_key = action.as_ref().map(|a| a.control_key()).unwrap_or_default();
    rsx! {
        if let Some(action)=action{
            TimestampControl{key:"{control_key}",state,action,label:label.to_owned(),disabled:busy||timestamp::cooling(s)}
        }else if timestamp::layout(s){
            button{r#type:"button",class:"thread-feed-author thread-feed-timestamp",disabled:true,"aria-label":"Open this message in channel", "{label}"}
        }else{"{label}"}
        if busy{span{role:"status"," · Opening message…"}}
        if !notice.is_empty(){span{role:"status"," · {notice}"}}
    }
}
#[component]
fn TimestampControl(
    state: Signal<ShellState>,
    action: timestamp::Action,
    label: String,
    disabled: bool,
) -> Element {
    let alive = use_hook(|| Arc::new(AtomicBool::new(true)));
    let retirement = alive.clone();
    use_drop(move || retirement.store(false, Ordering::Release));
    rsx! {button{r#type:"button",class:"thread-feed-author thread-feed-timestamp",id:action.button_id(),
    "aria-label":"Open this message in channel",title:"Open this message in channel",disabled,
    onclick:move|event:MouseEvent|{
        let m=event.modifiers();
        if alive.load(Ordering::Acquire)&&!disabled
            &&!(m.ctrl()||m.meta()||m.alt()||m.shift())
            &&event.trigger_button().is_none_or(|b|b==dioxus::html::input_data::MouseButton::Primary){
            timestamp::start(state,action.clone());
        }
    },"{label}"}}
}
