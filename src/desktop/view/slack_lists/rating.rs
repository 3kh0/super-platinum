//! One bounded content renderer shared by Table, Board, details and the editor.
use super::*;
use crate::model::RichNode;
use super_platinum_core::slack_lists::RatingOptions;

pub(super) fn display(
    state: Signal<ShellState>,
    value: u8,
    max: u8,
    glyph: &RichNode,
    table: bool,
    media_epoch: u64,
) -> Element {
    let count = if table { max } else { value };
    if count == 0 {
        return rsx! {};
    }
    if let Some(label) = fallback(state, glyph) {
        return rsx! {span{class:"slack-list-rating-fallback",role:"img","aria-label":format!("{value} of {max}, {label}"),
        "data-rating-value":"{value}","data-rating-max":"{max}","{value} of {max} {label}"}};
    }
    rsx! {span {class:"slack-list-rating",role:"img","aria-label":format!("{value} of {max}"),"data-rating-value":"{value}","data-rating-max":"{max}",
        for n in 1..=count {span {key:"{n}",class:"slack-list-rating-mark","aria-hidden":"true",
            if n<=value {{super::super::rich::rich_node(glyph,media_epoch,state)}}else{{icon(Icon::CircleFilled,"icon")}}
        }}
    }}
}
pub(super) fn fields(
    state: Signal<ShellState>,
    options: &RatingOptions,
    mut selected: Signal<Option<u8>>,
    disabled: bool,
) -> Element {
    let shell = state.read();
    let workspace = shell
        .core
        .active_team
        .as_ref()
        .and_then(|id| shell.core.workspaces.get(id));
    let glyph = crate::slack_lists::rating::glyph(
        workspace,
        &shell.media,
        options.emoji(),
        options.team_id(),
    );
    let epoch = shell.media_epoch;
    drop(shell);
    let max = options.max();
    let value = selected().unwrap_or(0);
    let fallback = fallback(state, &glyph);
    rsx! {fieldset {class:"slack-list-rating-fields",disabled,
        onkeydown:move |event|{if !disabled {let current=*selected.peek();if let Some(next)=key_value(current,max,event.key()) {event.prevent_default();event.stop_propagation();selected.set(next);}}},
        legend {"Rating"}
        div {class:"slack-list-rating-choices",
            for n in 1..=max {button {key:"{n}",r#type:"button",class:"slack-list-rating-choice",disabled,
                "data-rating-choice":n,"aria-label":format!("{n} of {max}"),"aria-pressed":value==n,
                onclick:move |_|{if !disabled {let next=if *selected.peek()==Some(n){None}else{Some(n)};selected.set(next);}},
                if fallback.is_none() {span {"aria-hidden":"true",{super::super::rich::rich_node(&glyph,epoch,state)}}}
                span {"{n}"}
            }}
        }
        if let Some(label)=fallback {p{class:"slack-list-rating-fallback","{label}"}}
        p {class:"slack-list-editor-hint",role:"status",if value==0 {"Unrated"}else{"{value} of {max}"}}
        button {id:"slack-list-rating-clear",r#type:"button",disabled:disabled||value==0,
            onclick:move |_|{if !disabled {selected.set(None);}},"Clear rating"}
    }}
}

pub(super) fn fallback(state: Signal<ShellState>, glyph: &RichNode) -> Option<String> {
    match glyph {
        RichNode::Emoji { name, glyph } if glyph == &format!(":{name}:") => Some(glyph.clone()),
        RichNode::EmojiImage { id, name } if !state.read().media.is_ready(id) => {
            Some(format!(":{name}:"))
        }
        _ => None,
    }
}

fn key_value(value: Option<u8>, max: u8, key: Key) -> Option<Option<u8>> {
    let n = value.unwrap_or(0);
    let next = match key {
        Key::ArrowRight | Key::ArrowUp => n.saturating_add(1).min(max),
        Key::ArrowLeft | Key::ArrowDown => n.saturating_sub(1),
        Key::Home => 0,
        Key::End => max,
        Key::Delete | Key::Backspace => 0,
        Key::Character(s) => match s.as_str() {
            "0" => 0,
            "1" => 1,
            "2" => 2,
            "3" => 3,
            "4" => 4,
            "5" => 5,
            _ => return None,
        },
        _ => return None,
    };
    if next > max {
        return None;
    }
    Some((next > 0).then_some(next))
}

#[cfg(test)]
mod tests;
