//! One bookmark open control for whole-hierarchy and selected-folder views.
use crate::state::ShellState;
use dioxus::prelude::*;
use super_platinum_core::bookmarks::Bookmark;

#[component]
pub(crate) fn OpenRow(
    state: Signal<ShellState>,
    bookmark: Bookmark,
    parent: Option<String>,
    expanded: Option<bool>,
    disabled: bool,
    onopen: EventHandler,
) -> Element {
    let snapshot = state.read();
    let title = bookmark.title();
    let url = bookmark.link.as_deref().unwrap_or("");
    rsx! {
        button {class:"bookmark-open",disabled,title:url,"aria-expanded":expanded.map(|b|b.to_string()),onclick:move |_|onopen.call(()),
            {super::emoji::render(state,&snapshot,&bookmark)}
            span {class:"files-row-body",span {class:"files-title","{title}"}
                span {class:"files-meta",if let Some(parent)=parent {"{parent} · "}
                    if bookmark.folder(){"Folder"}else{"{bookmark.link.as_deref().unwrap_or(\"Open this item in Slack\")}"}
                }
            }
        }
    }
}
