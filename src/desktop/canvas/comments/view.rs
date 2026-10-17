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
                    if let Some(error)=value.error {p {class:"canvas-comments-error",role:"alert","{error}"}}
                    if value.loading {p {role:"status","Loading comments..."}}
                    if !value.loading&&value.error.is_none()&&value.rows.is_empty() {
                        p {id:"canvas-comments-empty",match value.filter {Filter::Unread=>"No unread comments",Filter::Resolved=>"There are no resolved threads",Filter::Recent=>"Nobody has added any comments"}}
                    }
                    for row in &value.rows {
                        article {class:"canvas-comment",key:"{row.body.id}","data-comment-ts":row.body.ts.clone(),
                            {body(&row.body,state,epoch)}
                            if stream {
                                if let Some(preview)=&row.preview_body {div {class:"canvas-comment-preview",{body(preview,state,epoch)}}}
                                button {class:"canvas-comment-open","data-canvas-comment-action":"thread",disabled,onclick:{let root=row.body.ts.clone();move |_|start(state,model,Query::Replies(root.clone()),false)},"Open thread"}
                            }
                        }
                    }
                    if value.pages.previous().is_some() {button {id:"canvas-comments-previous","data-canvas-comment-action":"previous",disabled,onclick:move |_|{if let Some(query)=previous.clone(){begin(state,model,query,Direction::Previous);}},"Previous page"}}
                    if value.cursor.is_some() {
                        button {id:"canvas-comments-more","data-canvas-comment-action":"more",disabled,onclick:move |_|{if let Some(query)=query.clone(){start(state,model,query,true);}},"Next page"}
                    }
                }
            }
            p {class:"canvas-comments-note","Read-only comments. Reply or resolve threads in Slack."}
        }
    }
}
fn body(message: &crate::model::MessageVm, state: Signal<ShellState>, epoch: u64) -> Element {
    rsx! {
        header {strong {"{message.author}"}time {"{message.timestamp}"}}
        div {class:"message-body",for node in &message.body {{crate::view::rich::rich_node(node,epoch,state)}}}
    }
}
