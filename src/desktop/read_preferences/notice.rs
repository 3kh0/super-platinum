use crate::state::ShellState;
use dioxus::prelude::*;

pub(crate) fn unread_loading(mut state: Signal<ShellState>, snapshot: &ShellState) -> Element {
    let visible = snapshot
        .read_presentation
        .unread_loading
        .as_ref()
        .is_some_and(|(scope, _)| scope == &snapshot.timeline_scope());
    if !visible {
        return rsx! {};
    }
    let paused = snapshot.read_presentation.unread_paused;
    rsx! {
        div {class:"unread-hold",role:"status",id:"unread-opening-status",
            span {if paused {"The unread starting point is farther back."} else {"Loading unread starting point..."}}
            if paused {
                button {id:"unread-opening-continue",onclick:move |_| {spawn(crate::bootstrap::refresh_selected_channel(state));},"Continue loading"}
            }
            button {id:"unread-opening-latest",onclick:move |_| {
                {let mut shell=state.write();
                    super::opening::cancel(&mut shell);
                    if let Some(channel)=shell.core.active_channel.clone() {
                        shell.core.pending_scroll_to=Some((channel,super_platinum_core::domain::PendingScrollTarget::Latest));
                        shell.stick_to_bottom=true;shell.reset_timeline_window();
                    }
                }
                spawn(crate::bootstrap::refresh_selected_channel(state));
            },"Go to latest"}
        }
    }
}
