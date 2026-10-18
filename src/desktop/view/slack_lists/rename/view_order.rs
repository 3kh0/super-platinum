use super::*;
use rename::view_order::Direction;

pub(super) fn actions(state: Signal<ShellState>, visit: u64) -> Element {
    let shell = state.read();
    let Some(view) = rename::selected_view(&shell).map(str::to_owned) else {
        return rsx! {};
    };
    let selection = rename::selection(&shell).unwrap_or_default();
    let up_disabled = !rename::view_order::can_open(&shell, Direction::Up);
    let down_disabled = !rename::view_order::can_open(&shell, Direction::Down);
    let down_view = view.clone();
    drop(shell);
    rsx! {
        button { id:"slack-list-view-move-up", r#type:"button", class:"slack-list-action", disabled:up_disabled,
            "data-reorder-view":"{view}", "data-reorder-direction":"up", "data-reorder-selection":"{selection}",
            onmounted:move |_| { dioxus::document::eval(include_str!("view_order_opener.js")); },
            onclick:move |_| rename::view_order::open(state,visit,selection,view.clone(),Direction::Up),
            "Move view up"
        }
        button { id:"slack-list-view-move-down", r#type:"button", class:"slack-list-action", disabled:down_disabled,
            "data-reorder-view":"{down_view}", "data-reorder-direction":"down", "data-reorder-selection":"{selection}",
            onclick:move |_| rename::view_order::open(state,visit,selection,down_view.clone(),Direction::Down),
            "Move view down"
        }
    }
}
pub(super) fn fields(p: rename::view_order::Presentation) -> Element {
    let hint = if p.lanes {
        "This changes the shared lane order for everyone with access."
    } else {
        "This changes the shared custom view order for everyone with access. Built-in views stay in their own section."
    };
    let heading = if p.lanes {
        "Current lanes"
    } else {
        "Current custom views"
    };
    let movement = if p.lanes {
        format!("Move {} {}.", p.name, p.direction)
    } else {
        format!("Move {} {} by one place.", p.name, p.direction)
    };
    rsx! {
        p { id:"slack-list-view-order-move", class:"slack-list-editor-item", "{movement}" }
        p { class:"slack-list-editor-hint", "{hint}" }
        p { "Before" }
        ol { id:"slack-list-view-order-before", for (id,name) in p.before { li { "data-view-id":"{id}", "{name}" } } }
        p { "After saving" }
        ol { id:"slack-list-view-order-after", for (id,name) in p.after { li { "data-view-id":"{id}", "{name}" } } }
        p { "{heading}" }
        if let Some(current)=p.current {
            ol { id:"slack-list-view-order-current", for (id,name) in current { li { "data-view-id":"{id}", "{name}" } } }
        } else { p { id:"slack-list-view-order-current", "The current order is unavailable for this operation." } }
    }
}
