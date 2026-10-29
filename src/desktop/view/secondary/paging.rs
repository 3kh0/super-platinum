//! Paging remains reachable when filtering removes the list's scrollbar.
use crate::{
    bootstrap::dms::{InitialSource, PageSource},
    state::ShellState,
};
use dioxus::prelude::*;
const LABEL: &str = "Load older conversations";
fn empty_message(shell: &ShellState) -> Option<&'static str> {
    if shell.core.dms.loading
        || !shell.core.dms.loaded
        || shell
            .core
            .dms
            .entries
            .iter()
            .any(|e| shell.dm_row_visible(e))
    {
        return None;
    }
    if !shell.dm_query.is_empty() || shell.dm_unread_only || !shell.core.dms.entries.is_empty() {
        Some("No matching loaded conversations.")
    } else if shell
        .core
        .dms
        .next_cursor
        .as_ref()
        .is_some_and(|c| !c.is_empty())
    {
        Some("No conversations loaded on this page.")
    } else {
        Some("No direct messages to show.")
    }
}
pub(super) fn footer(
    state: Signal<ShellState>,
    shell: &ShellState,
    source: Option<PageSource>,
    initial: Option<InitialSource>,
) -> Element {
    let empty = empty_message(shell);
    let more_id = "dm-load-older";
    let initial_id = "dm-load-initial";
    let initial_label = "Load direct messages";
    let initial_status = if shell.core.dms.loading {
        "Loading direct messages…"
    } else {
        "Direct messages haven't loaded."
    };
    rsx! {
        if let Some(message)=empty {p{class:"list-empty",role:"status","{message}"}}
        if crate::bootstrap::dms::initial_phase(shell) {
            p{class:"list-loading",role:"status","{initial_status}"}
            div{class:"dm-pagination",
                button{id:initial_id,"aria-label":initial_label,"aria-busy":shell.core.dms.loading.to_string(),disabled:initial.as_ref().is_none_or(|s|!s.current(shell)),
                    onclick:move |_|{if let Some(source)=initial.clone(){spawn(crate::bootstrap::dms::load_initial(state,source));}},
                    "{initial_label}"
                }
            }
        }
        if shell.core.dms.next_cursor.as_ref().is_some_and(|c|!c.is_empty()) {
            div {class:"dm-pagination",
                button {id:more_id,"aria-label":LABEL,"aria-busy":shell.core.dms.loading.to_string(),disabled:source.as_ref().is_none_or(|s|!s.current(shell)),
                    onclick:move |_|{if let Some(source)=source.clone(){spawn(crate::bootstrap::load_more_dms(state,source));}},
                    "{LABEL}"
                }
            }
        }
    }
}
#[cfg(test)]
mod tests;
