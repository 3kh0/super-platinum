//! Item comments share the conversation row renderer without borrowing its actions.
use super::*;
pub(crate) mod bridge;
mod compose;
pub(crate) mod own_delete;
pub(crate) mod own_edit;
mod reaction_picker;

pub(super) fn back(state: Signal<ShellState>, generation: u64) {
    if crate::slack_lists::comments::own_edit::blocks_departure(&state.peek(), generation)
        || crate::slack_lists::comments::own_delete::blocks_departure(&state.peek(), generation)
    {
        return;
    }
    dioxus::core::spawn_forever(async move {
        if crate::slack_lists::comments::draft::depart(state, generation)
            .await
            .is_ok()
        {
            crate::slack_lists::comments::close(state, generation);
        }
    });
}

pub(super) fn close_list(mut state: Signal<ShellState>, visit: u64) {
    let generation =
        crate::slack_lists::comments::presentation(&state.peek()).map(|view| view.generation);
    let Some(generation) = generation else {
        if crate::slack_lists::generation(&state.peek()) == Some(visit) {
            crate::slack_lists::close_shell(&mut state.write());
        }
        return;
    };
    if crate::slack_lists::comments::own_edit::blocks_departure(&state.peek(), generation)
        || crate::slack_lists::comments::own_delete::blocks_departure(&state.peek(), generation)
    {
        return;
    }
    dioxus::core::spawn_forever(async move {
        if crate::slack_lists::comments::draft::depart(state, generation)
            .await
            .is_err()
        {
            return;
        }
        if crate::slack_lists::generation(&state.peek()) == Some(visit)
            && crate::slack_lists::comments::presentation(&state.peek())
                .is_some_and(|view| view.generation == generation)
        {
            crate::slack_lists::close_shell(&mut state.write());
        }
    });
}

pub(super) fn panel(state: Signal<ShellState>) -> Element {
    let Some(view) = crate::slack_lists::comments::presentation(&state.read()) else {
        return rsx! {};
    };
    rsx! {Comments{key:"comments-{view.generation}",state}}
}

#[component]
fn Comments(state: Signal<ShellState>) -> Element {
    use_effect(move || {
        let generation =
            crate::slack_lists::comments::presentation(&state.read()).map(|view| view.generation);
        if let Some(generation) = generation {
            crate::slack_lists::comments::file_acquisition::demand(state, generation);
        }
    });
    let Some(view) = crate::slack_lists::comments::presentation(&state.read()) else {
        return rsx! {};
    };
    let generation = view.generation;
    let editing =
        crate::slack_lists::comments::own_edit::blocks_departure(&state.read(), generation)
            || crate::slack_lists::comments::own_delete::blocks_departure(
                &state.read(),
                generation,
            );
    let disabled = view.loading || view.retry_after > 0 || editing;
    let composing = crate::slack_lists::comments::draft::presentation(&state.read()).is_some();
    let can_write = crate::slack_lists::comments::draft::can_open(&state.read(), generation);
    let (reveal, reveal_ts) = view
        .reveal
        .as_ref()
        .map(|(serial, ts)| (serial.to_string(), ts.clone()))
        .unwrap_or_default();
    let snapshot = state.read();
    let reaction_status =
        crate::slack_lists::comments::reaction_actions::status(&snapshot, generation);
    rsx! {
        section {id:"slack-list-comments",class:"slack-list-comments",role:"region",
            "aria-labelledby":"slack-list-comments-title","data-comments-generation":generation,
            "data-comment-reveal":reveal,"data-comment-reveal-ts":reveal_ts,
            "data-escape-local":"true",
            onmounted:move |_| {dioxus::document::eval(&format!("{FOCUS}\nmountListComments({generation});"));},
            header {class:"slack-list-comments-header",
                button {id:"slack-list-comments-back",r#type:"button",class:"slack-list-action",
                    disabled:editing,
                    onclick:move |_|back(state,generation),
                    {icon(Icon::ChevronRight,"icon")}"Back to item"
                }
                h3 {id:"slack-list-comments-title","Comments"}
                if composing && view.has_more {
                    {next_page_button(state,generation,disabled)}
                }
                button {id:"slack-list-comments-refresh",r#type:"button",class:"slack-list-action",disabled,
                    onclick:move |_|crate::slack_lists::comments::refresh(state,generation),
                    if view.error.is_some(){"Retry"}else{"Refresh"}
                }
            }
            p {class:"slack-list-comments-item","{view.title}"}
            if let Some(error)=&view.error {p {class:"slack-list-error",role:"alert","{error}"}}
            if view.retry_after > 0 {p {class:"slack-list-warning",role:"status","Try again in {view.retry_after} seconds."}}
            if view.loading {p {class:"slack-list-comments-status",role:"status","Loading comments…"}}
            div {class:"slack-list-comments-scroll","aria-busy":view.loading,
                if view.rows.is_empty()&&!view.loading&&view.error.is_none() {
                    p {class:"slack-list-empty",role:"status","No comments yet"}
                }
                for message in view.rows.iter() {
                    div {key:"{message.id}","data-list-comment-ts":"{message.ts}",
                        {super::super::message::message_row(state,&snapshot,message,&view.channel,None,super::super::message::RowSurface::ListComments { generation },None)}
                        if crate::slack_lists::comments::own_edit::available(&snapshot,generation,&message.ts) {
                            button {r#type:"button",class:"slack-list-comment-edit-action",
                                "data-list-comment-edit-ts":"{message.ts}",disabled,
                                onclick:{let ts=message.ts.clone();move |_|crate::slack_lists::comments::own_edit::open(state,generation,ts.clone())},
                                "Edit"
                            }
                        }
                        if crate::slack_lists::comments::own_delete::available(&snapshot,generation,&message.ts) {
                            button {r#type:"button",class:"slack-list-comment-edit-action",
                                "data-list-comment-delete-ts":"{message.ts}",
                                disabled:view.loading || editing || (view.retry_after > 0 && !crate::slack_lists::comments::own_delete::retained_for(&snapshot,generation,&message.ts)),
                                onclick:{let ts=message.ts.clone();move |_|crate::slack_lists::comments::own_delete::open(state,generation,ts.clone())},
                                "Delete"
                            }
                        }
                    }
                }
            }
            if !composing {
                if let Some(status) = reaction_status {
                    p {id:"slack-list-comments-reaction-status",class:"slack-list-comments-status",role:"status","{status}"}
                }
            }
            if !composing && !editing {footer {class:"slack-list-comments-footer",
                button {id:"slack-list-comment-write",r#type:"button",class:"slack-list-action",disabled:disabled||!can_write,
                    onclick:move |_|crate::slack_lists::comments::draft::open(state,generation),"Write a comment"}
                if view.has_more {
                    {next_page_button(state,generation,disabled)}
                }
            }}
            {compose::panel(state)}
            {own_edit::panel(state)}
            {reaction_picker::panel(state)}
            {own_delete::panel(state)}
        }
    }
}

fn next_page_button(state: Signal<ShellState>, generation: u64, disabled: bool) -> Element {
    rsx! {button {id:"slack-list-comments-next",r#type:"button",class:"slack-list-action",disabled,
    onclick:move |_|crate::slack_lists::comments::next_page(state,generation),"Next page"}}
}

const FOCUS: &str = include_str!("comments.js");
