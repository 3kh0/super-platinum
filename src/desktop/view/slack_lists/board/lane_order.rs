use super::*;
use crate::slack_lists::rename::lane_order::{self, Direction};

#[component]
pub(super) fn Actions(
    state: Signal<ShellState>,
    visit: u64,
    layout: u64,
    lane_key: String,
) -> Element {
    let shell = state.read();
    let current = crate::slack_lists::presentation(&shell)
        .is_some_and(|v| v.generation == visit && v.layout_generation == layout);
    let left = current
        .then(|| lane_order::adjacent(&shell, &lane_key, Direction::Left))
        .flatten();
    let right = current
        .then(|| lane_order::adjacent(&shell, &lane_key, Direction::Right))
        .flatten();
    let left_disabled = left.is_none();
    let right_disabled = right.is_none();
    drop(shell);
    rsx! {
        button { r#type:"button",class:"slack-list-action",disabled:left_disabled,
            "aria-label":"Move lane left",title:"Move lane left","data-lane-move":"left","data-lane-key":"{lane_key}",
            onclick:move |_|{if let Some(action)=left.clone(){lane_order::apply(state,action)}},
            "Left"
        }
        button { r#type:"button",class:"slack-list-action",disabled:right_disabled,
            "aria-label":"Move lane right",title:"Move lane right","data-lane-move":"right","data-lane-key":"{lane_key}",
            onclick:move |_|{if let Some(action)=right.clone(){lane_order::apply(state,action)}},
            "Right"
        }
    }
}
