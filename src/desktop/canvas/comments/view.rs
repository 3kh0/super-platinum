use super::*;

#[component]
pub(in crate::canvas) fn Panel(
    state: Signal<ShellState>,
    mut model: Signal<State>,
    mut opened: Signal<bool>,
) -> Element {
    let value = model.read();
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
                button {id:"canvas-comments-back","data-canvas-comment-action":"back",onclick:move |_|{let mut value=model.write();value.next_serial();value.loading=false;drop(value);opened.set(false);},"Back to canvas"}
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
                    if !value.loading&&value.error.is_none()&&value.rows.is_empty() {
                        p {id:"canvas-comments-empty",match value.filter {Filter::Unread=>"No unread comments",Filter::Resolved=>"There are no resolved threads",Filter::Recent=>"Nobody has added any comments"}}
                    }
                    if mutation::review::available(&state.read(), &value) && value.review.is_none() {
                        button {id:"canvas-comment-review-open","data-canvas-comment-action":"review",onclick:move |_|mutation::review::open(state,model),"Review unconfirmed change"}
                    }
                    {mutation::review::view(state, model)}
                    if value.review.is_none() {for row in &value.rows {
                        article {class:"canvas-comment",key:"{row.body.id}","data-comment-ts":row.body.ts.clone(),
                            {body(&row.body,state,epoch)}
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
                    if value.review.is_none() && !stream {reply::Composer {state,model}}
                }
            }
            {mutation::review::actions(state, model)}
            if value.review.is_none() {
            if let Some(error)=value.mutation_error {p {role:"alert","{error}"}}
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
