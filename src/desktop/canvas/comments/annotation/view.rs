use super::*;
#[component]
pub(in crate::canvas::comments) fn Author(
    mut state: Signal<ShellState>,
    mut model: Signal<super::super::State>,
) -> Element {
    let mut ticket = use_signal(move || retained_ticket(&state.peek()));
    let Some(t) = ticket.read().clone() else {
        return rsx! {button{id:"canvas-comment-new",onclick:move |_|{if let Some(t)=bind(state){ticket.set(Some(t.clone()));refresh(state,t);}},"Add a comment to a section"}};
    };
    let shell = state.read();
    let Some(e) = entry(&shell, &t.id) else {
        return rsx! {};
    };
    let current = t.current(&shell);
    let busy = e.busy;
    let issue = e.error;
    let has_ack = e.acknowledged.is_some() || e.verified.is_some();
    let pending = e.pending.as_ref();
    let continuation = pending.is_some_and(|p| {
        matches!(
            p.stage(),
            AnnotationStage::AnchorConfirmed | AnnotationStage::BootstrapNotSent
        )
    });
    let start_disabled = !current
        || busy
        || e.snapshot.is_none()
        || e.selected.is_empty()
        || pending.is_some()
        || has_ack
        || e.accepted.is_some()
        || rate::current(&shell).is_some();
    let selected = e.selected.clone();
    let existing = e.selected_range.is_none() && e.existing_controls.contains_key(&selected);
    let selected_blocks = e
        .choices
        .iter()
        .find(|(id, _)| id == &selected)
        .map(|(_, v)| v.as_slice());
    let page = e.page;
    let pages = e.choices.len().div_ceil(50);
    let checked = e.reviewed;
    let review_ready = e.review.is_some();
    rsx! {section{class:"canvas-comment-author", "aria-label":"New canvas comment",
        h3{if e.selected_range.is_some(){"Comment on selected text"}else if e.selected_image.is_some(){"Comment on image"}else{"Comment on a section"}}
        if !current {p{role:"status","This canvas view changed. Reopen comments on the original canvas to continue."}}
        if busy {p{role:"status","Checking comment state..."}}
        if let Some(error)=issue {p{role:"alert","{error}"}}
        if pending.is_none() && !has_ack && e.accepted.is_none() {
            if let Some(range)=&e.selected_range {
                p{class:"canvas-review-preview","{range.text}"}
                p{"The selected text will receive a new comment thread. The paragraph's text and formatting are preserved."}
            } else if let Some(image)=&e.selected_image {
                p{class:"canvas-review-preview",if image.label.is_empty(){"Image"}else{"{image.label}"}}
                p{"This image and its original export result are retained for the new thread. The image is preserved."}
            } else {
            label{r#for:"canvas-comment-paragraph","Section"
                select{id:"canvas-comment-paragraph",value:selected.clone(),disabled:!current||busy,
                    onchange:{let t=t.clone();move|event|{let value=event.value();let mut s=state.write();if !t.current(&s){return;}if let Some(e)=entry_mut(&mut s,&t.id).filter(|e|!e.busy){if e.choices.iter().any(|(id,_)|id==&value){e.selected=value;}}}},
                    option{value:"",disabled:true,"Choose a section"}
                    for (id,blocks) in e.choices.iter().skip(page*50).take(50) {
                        option{value:id.clone(),"{super_platinum_core::canvas::comments::content::fallback(blocks).unwrap_or_else(|_|id.clone())}"}
                    }
                }
            }
            if pages>1 {div{class:"canvas-comments-toolbar",
                button{disabled:page==0||busy,onclick:{let t=t.clone();move |_|{if let Some(e)=entry_mut(&mut state.write(),&t.id){e.page=e.page.saturating_sub(1);e.selected.clear();}}},"Previous sections"}
                button{disabled:page+1>=pages||busy,onclick:{let t=t.clone();move |_|{if let Some(e)=entry_mut(&mut state.write(),&t.id){if (e.page+1)*50<e.choices.len(){e.page+=1;e.selected.clear();}}}},"Next sections"}
            }}
            if let Some(blocks)=selected_blocks {{preview(state,blocks)}}
            if e.snapshot.is_some()&&e.choices.is_empty(){p{"No supported section anchors are available in this canvas."}}
            }
            button{id:"canvas-comment-create",disabled:start_disabled,onclick:{let t=t.clone();move |_|run::create(state,model,t.clone())},if existing {"Open comment thread"} else {"Create comment thread"}}
        }
        if let Some(pending)=pending {
            p{"The original annotation and its delivery state are retained locally."}
            details{summary{"Original paragraph"},p{class:"canvas-review-preview",{pending.receipt().record().snippet()["mrkdwn"].as_str().unwrap_or("")}}}
            if e.verified.is_none() && !continuation && e.accepted.is_none(){
                if e.acknowledged.is_some(){p{"Slack acknowledged the annotation. Its canonical anchor could not be confirmed; the accepted response will remain in local history."}}
                if let Some(review)=&e.review {
                    p{"Current paragraph from the last review:"}
                    if let Ok(blocks)=review.preview_blocks() {{preview(state,&blocks)}}
                    else {p{"The current paragraph contains unsupported rich content; its exact bytes remain retained locally."}}
                    p{if review.control_bytes().is_some(){"The original annotation control is present."}else{"The original annotation control is absent in this read. This does not prove the request failed."}}
                }
                label{input{r#type:"checkbox",checked,disabled:!current||busy||!review_ready,onchange:{let t=t.clone();move|event|{let mut s=state.write();if t.current(&s){if let Some(e)=entry_mut(&mut s,&t.id){e.reviewed=event.checked();}}}}},"I reviewed the current canvas and understand this thread may already exist."}
                button{disabled:!current||busy||!checked||!review_ready,onclick:{let t=t.clone();move |_|run::dispose(state,t.clone())},"Keep outcome unknown and remove local lock"}
            }
        }
        div{class:"canvas-comments-toolbar",
            button{disabled:busy,onclick:move |_|{if let Some(next)=bind(state){ticket.set(Some(next.clone()));refresh(state,next);}},"Refresh current annotation state"}
            if e.acknowledged.is_some()||e.retained_anchor.is_some(){button{disabled:busy||!current,onclick:{let t=t.clone();move |_|run::keep_ack_history(state,t.clone())},"Keep accepted response in local history"}}
            if has_ack||continuation||e.accepted.is_some(){button{disabled:busy||!current,onclick:{let t=t.clone();move |_|run::resume(state,t.clone())},if e.accepted.is_some(){"Retry local result save"}else{"Continue confirmed annotation"}}}
            if let Some(root)=e.created.clone(){button{disabled:busy||!current,onclick:{let t=t.clone();move |_|{
                if !t.current(&state.peek()){return;}
                let Some(ts)=root.ts.clone() else{return;};
                {let mut value=model.write();value.owner=Some(t.owner.clone());value.root=Some(root.clone());value.loading=false;}
                super::super::start(state,model,Query::Replies(ts),false);
            }},"Open created thread"}}
        }
        if !e.history.is_empty() || !e.history_back.is_empty(){details{summary{"Local annotation results"},
            for row in &e.history {article{key:"{row.id}",p{"{row.status}"},p{class:"canvas-review-preview",{row.pending.receipt().record().snippet()["mrkdwn"].as_str().unwrap_or("")}},
                if let Some(root)=row.root(){button{disabled:busy||!current,onclick:{let t=t.clone();move |_|open_thread(state,model,&t,&root)},"Open observed thread"}}
            }}
            div{class:"canvas-comments-toolbar",
                button{disabled:busy||!current||e.history_back.is_empty(),onclick:{let t=t.clone();move |_|history_page(state,t.clone(),false)},"Previous local results"}
                button{disabled:busy||!current||e.history.len()!=20,onclick:{let t=t.clone();move |_|history_page(state,t.clone(),true)},"Next local results"}
            }
        }}
        button{disabled:busy||!current||pending.is_some()||has_ack||e.accepted.is_some(),onclick:{let t=t.clone();move |_|{if close(state,&t){ticket.set(None);}}},"Close annotation review"}
    }}
}
fn preview(state: Signal<ShellState>, blocks: &[serde_json::Value]) -> Element {
    let shell = state.read();
    let Some(workspace) = shell
        .core
        .active_team
        .as_ref()
        .and_then(|id| shell.core.workspaces.get(id))
    else {
        return rsx! {};
    };
    let nodes: Vec<_> = blocks
        .iter()
        .flat_map(|b| {
            crate::blocks::block_nodes(crate::blocks::BlockCtx::new(workspace, &shell.media), b)
        })
        .collect();
    let epoch = shell.media_epoch;
    rsx! {div{class:"canvas-comment-reply-version",for node in &nodes {{crate::view::rich::rich_node(node,epoch,state)}}}}
}

fn open_thread(
    state: Signal<ShellState>,
    mut model: Signal<super::super::State>,
    t: &Ticket,
    root: &Message,
) {
    if !t.current(&state.peek()) {
        return;
    }
    let Some(ts) = root.ts.clone() else {
        return;
    };
    {
        let mut value = model.write();
        value.owner = Some(t.owner.clone());
        value.root = Some(root.clone());
        value.loading = false;
    }
    // This ordinary timestamp read refreshes access/current contents. Inert
    // local history does not stand in for a new Slack reply-page observation.
    super::super::start(state, model, Query::Replies(ts), false);
}
