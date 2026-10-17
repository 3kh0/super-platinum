//! Vote content is shared; Board intentionally supplies no mutable control.
use super::*;
use crate::model::RichNode;

pub(super) fn control(
    state: Signal<ShellState>,
    count: usize,
    voted: bool,
    tooltip: &str,
    glyph: &RichNode,
    epoch: u64,
    readonly: bool,
    disabled: bool,
    detail: bool,
    mut toggle: impl FnMut() + 'static,
) -> Element {
    let mut label = format!(
        "{count} {}{}",
        if count == 1 { "vote" } else { "votes" },
        if voted { ", including yours" } else { "" }
    );
    let fallback = super::rating::fallback(state, glyph);
    // A fixed-size action must not hide the only readable custom-emoji fallback.
    // Keep the full fallback visible beside the control and in its accessible name.
    let hint = fallback
        .as_ref()
        .map(|text| format!("{tooltip}, {text}"))
        .unwrap_or_else(|| tooltip.to_owned());
    if let Some(text) = &fallback {
        label.push_str(&format!(", {text}"));
    }
    let mark = rsx! {span{class:"slack-list-vote-mark","aria-hidden":"true",if fallback.is_none() {{super::super::rich::rich_node(glyph,epoch,state)}}}};
    let surface = if readonly {
        rsx! {span{class:"slack-list-vote",role:"img","aria-label":"{label}",title:"{hint}","data-vote-count":count,"data-vote-owned":voted,{mark}span{"{count}"}}}
    } else {
        let action = if voted {
            "Remove your vote"
        } else {
            "Add your vote"
        };
        rsx! {button{r#type:"button",class:"slack-list-vote slack-list-vote-action",disabled,tabindex:if detail{"0"}else{"-1"},
            "aria-label":"{action}, {label}","aria-pressed":voted,title:"{hint}","data-vote-count":count,"data-vote-owned":voted,
            onclick:move |_|{if !disabled{toggle();}},
            {mark}span{"{count}"}
        }}
    };
    if let Some(text) = fallback {
        rsx! {span{class:"slack-list-vote-content",{surface}span{class:"slack-list-vote-fallback","aria-hidden":"true","{text}"}}}
    } else {
        surface
    }
}
