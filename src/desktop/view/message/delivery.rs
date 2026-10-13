use super::*;
pub(super) fn row(state: Signal<ShellState>, snapshot: &ShellState, ts: &str) -> Element {
    let Some(view) = crate::messaging::delivery_check::presentation(snapshot, ts) else {
        return rsx! {};
    };
    rsx! {
        div { class:"upload-delivery", "data-upload-delivery":ts,
            ondoubleclick:|event|event.stop_propagation(),
            button {type:"button",class:"upload-delivery-check",disabled:view.disabled,
                "aria-label":"Check file delivery", title:"Read the conversation to confirm these files arrived. This never uploads again.",
                onclick:move |event|{event.stop_propagation();crate::messaging::delivery_check::check(state,view.action.clone());},
                if view.busy {"Checking…"} else {"Check delivery"}
            }
            span {class:"upload-delivery-hint",role:"status","{view.message}"}
        }
    }
}
