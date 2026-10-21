use super::*;
mod attribution;
mod canonical_file;
pub(in crate::canvas) use attribution::ProfileToken;
pub(crate) use attribution::clear_profile_token;
pub(crate) use attribution::{HoverToken, clear_profile_hover_token, profile_hover_token};
pub(crate) use attribution::{profile_active, profile_overlay};

#[component]
pub(in crate::canvas) fn Panel(
    state: Signal<ShellState>,
    mut model: Signal<State>,
    mut opened: Signal<bool>,
) -> Element {
    use_effect(move || file_acquisition::observe(state, model));
    let value = model.read();
    if fixture::special::requested() {
        fixture::special::observe(&state.read(), &value);
    }
    if fixture::file_fallback::requested() {
        fixture::file_fallback::observe(&state.read(), &value);
    }
    if fixture::attribution::requested() {
        fixture::attribution::observe(&state.read(), &value);
    }
    if fixture::canonical_files::requested() {
        fixture::canonical_files::remember_model(model);
        fixture::canonical_files::observe(&state.read(), &value);
    }
    let current = value
        .owner
        .as_ref()
        .is_some_and(|owner| owner.current(&state.read()));
    let disabled = value.loading || value.cooling();
    let query = value.query.clone();
    let retry = query.clone();
    let previous = query.clone();
    let stream = matches!(query, Some(Query::Stream(_)));
    let epoch = state.read().media_epoch;
    let mutation_reason = mutation::reason(&state.read(), &value);
    let read = value.serial;
    rsx! {
        section {id:"canvas-comments",class:"canvas-comments","aria-label":"Canvas comments",
            nav {class:"canvas-comments-toolbar","aria-label":"Comment navigation",
                button {id:"canvas-comments-back","data-canvas-comment-action":"back",onclick:move |_|reply::back_to_canvas(state,model,opened),"Back to canvas"}
                if !stream {button {id:"canvas-comments-all","data-canvas-comment-action":"all",disabled,onclick:move |_|open(state,model),"All threads"}}
                if stream {label {"Filter"
                    select {id:"canvas-comments-filter","data-canvas-comment-action":"filter",value:value.filter.wire(),disabled,
                        onchange:move |event|{let filter=match event.value().as_str(){"unread"=>Filter::Unread,"archived"=>Filter::Resolved,_=>Filter::Recent};start(state,model,Query::Stream(filter),false);},
                        for filter in [Filter::Recent,Filter::Unread,Filter::Resolved] {option {value:filter.wire(),"{filter.label()}"}}
                    }
                }}
                button {id:"canvas-comments-refresh","data-canvas-comment-action":"refresh",disabled,onclick:move |_|{if let Some(query)=retry.clone(){start(state,model,query,false);}},"Refresh comments"}
            }
            div {class:"canvas-comments-scroll","aria-busy":value.loading,
                if !current {p {role:"status","This comment view has changed. Reopen it from the current canvas."}}
                else {
                    if value.review.is_none() {if let Some(error)=value.error {p {class:"canvas-comments-error",role:"alert","{error}"}}}
                    if value.loading {p {role:"status","Loading comments..."}}
                    if value.selected_control.is_some() {
                        p {role:"status","Finding the selected comment. If it is not on this page, use Next page."}
                        button {id:"canvas-comments-selection-cancel","data-canvas-comment-action":"all",disabled,onclick:move |_|open(state,model),"Show all threads"}
                    }
                    if !value.loading&&value.error.is_none()&&value.rows.is_empty() {
                        p {id:"canvas-comments-empty",match value.filter {Filter::Unread=>"No unread comments",Filter::Resolved=>"There are no resolved threads",Filter::Recent=>"Nobody has added any comments"}}
                    }
                    if mutation::review::available(&state.read(), &value) && value.review.is_none() {
                        button {id:"canvas-comment-review-open","data-canvas-comment-action":"review",onclick:move |_|mutation::review::open(state,model),"Review unconfirmed change"}
                    }
                    {mutation::review::view(state, model)}
                    if stream && value.review.is_none() && value.selected_control.is_none() {annotation::Author{state,model}}
                    if value.review.is_none() {for row in &value.rows {
                        article {class:"canvas-comment",key:"{row.body.id}","data-comment-ts":row.body.ts.clone(),
                            if row.message.subtype.as_deref()==Some("document_comment_root") {
                                {attribution::header(state,model,&row.message,&row.body)}
                                {canonical_file::content(state,model,&row.message,&row.body,epoch)}
                            } else {{body(&row.body,state,epoch)}}
                            if value.mutation_ready && !disabled {
                                if let Some(file) = &value.file {
                                    if let Some(desired) = super_platinum_core::canvas::comments::archive::ArchiveIntent::action(file, &row.message) {
                                        button { class:"canvas-comment-resolve", "data-canvas-comment-action":"resolve", disabled:mutation_reason.is_some(),
                                            onclick:{let root=row.message.clone(); move |_|mutation::choose(state,model,root.clone(),read)},
                                            if desired {"Resolve thread"} else {"Re-open thread"}
                                        }
                                    }
                                }
                            }
                            if stream {
                                if let Some(preview)=&row.preview_body {div {class:"canvas-comment-preview",{body(preview,state,epoch)}}}
                                button {class:"canvas-comment-open","data-canvas-comment-action":"thread",disabled,onclick:{let root=row.body.ts.clone();move |_|start(state,model,Query::Replies(root.clone()),false)},"Open thread"}
                            }
                        }
                    }
                    }
                    if value.pages.previous().is_some() {button {id:"canvas-comments-previous","data-canvas-comment-action":"previous",disabled,onclick:move |_|{if let Some(query)=previous.clone(){begin(state,model,query,Direction::Previous);}},"Previous page"}}
                    if value.cursor.is_some() {
                        button {id:"canvas-comments-more","data-canvas-comment-action":"more",disabled,onclick:move |_|{if let Some(query)=query.clone(){start(state,model,query,true);}},"Next page"}
                    }
                    if value.review.is_none() && !stream && selection::reply_ready(&value) {reply::Composer {state,model}}
                }
            }
            {mutation::review::actions(state, model)}
            if value.review.is_none() {
            if let Some(error)=value.mutation_error {p {role:"alert",
                "data-canvas-file-refusal":error=="This file could not be opened. Your reply draft is retained here.",
                "{error}"
            }}
            if let Some(reason)=mutation_reason { if value.mutation_ready {p {class:"canvas-comments-note",role:"status","{reason}"}} }
            }
        }
    }
}
fn body(message: &crate::model::MessageVm, state: Signal<ShellState>, epoch: u64) -> Element {
    rsx! {
        header {strong {"{message.author}"}time {"{message.timestamp}"}}
        div {class:"message-body",for node in &message.body {{crate::view::rich::rich_node(node,epoch,state)}}}
    }
}
fn content(message: &crate::model::MessageVm, state: Signal<ShellState>, epoch: u64) -> Element {
    rsx! {div {class:"message-body",for node in &message.body {{crate::view::rich::rich_node(node,epoch,state)}}}}
}
