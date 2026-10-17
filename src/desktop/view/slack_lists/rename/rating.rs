use super::*;
use rename::text_create::{Choice, rating};
use super_platinum_core::slack_lists::RatingOptions;

pub(super) fn fields(
    state: Signal<ShellState>,
    serial: u64,
    visit: u64,
    selection: u64,
    choice: Choice,
    options: &RatingOptions,
    enabled: bool,
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
    let emoji = options.emoji();
    rsx! {
        if choice.kind == super_platinum_core::slack_lists::ColumnCreateKind::Rating {
        label {r#for:"slack-list-new-rating-max","Quantity"}
        select {id:"slack-list-new-rating-max",value:"{max}","data-field-choice":"{choice.revision}",disabled:!enabled,
            onchange:move |event|rating::quantity(state,serial,visit,selection,choice,event.value()),
            for n in 1..=5 {option{key:"{n}",value:"{n}",selected:n==max,"{n}"}}
        }
        }
        label {r#for:"slack-list-new-rating-emoji","Emoji"}
        button {id:"slack-list-new-rating-emoji",r#type:"button",disabled:!enabled,"data-field-choice":"{choice.revision}","aria-haspopup":"dialog",
            onclick:move |_|rating::open(state,serial,visit,selection,choice),
            span {"aria-hidden":"true",{crate::view::rich::rich_node(&glyph,epoch,state)}}
            span {class:"slack-list-rating-shortcode","{emoji}"}
            {icon(Icon::AddReaction,"icon")}
        }
    }
}
pub(super) fn picker(
    state: Signal<ShellState>,
    serial: u64,
    visit: u64,
    selection: u64,
    choice: Choice,
    token: u64,
) -> Element {
    rsx! {div{id:"slack-list-rating-picker",style:"display:contents","data-rating-picker":"{token}",crate::reaction_picker::EmojiPicker {
        key:"{serial}:{token}",state,title:if choice.kind==super_platinum_core::slack_lists::ColumnCreateKind::Vote {"Choose vote emoji".to_owned()}else{"Choose rating emoji".to_owned()},
        onchoose:move |picked|rating::choose(state,serial,visit,selection,choice,token,picked),
        onclose:move |_|rating::close(state,serial,visit,selection,choice,token),
    }}}
}
