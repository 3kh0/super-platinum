use super::*;
#[component]
pub(in crate::canvas::comments) fn Composer(
    mut state: Signal<ShellState>,
    model: Signal<State>,
) -> Element {
    let mut binding = use_signal(|| None::<Binding>);
    use_effect(move || {
        let next = {
            let shell = state.read();
            let value = model.read();
            let root = match &value.query {
                Some(Query::Replies(root)) => Some(root.clone()),
                _ => None,
            };
            root.and_then(|root| {
                value
                    .owner
                    .clone()
                    .filter(|owner| owner.current(&shell))
                    .map(|owner| (owner, root, value.serial, shell.core.account_epoch))
            })
        };
        let Some((owner, root, serial, _epoch)) = next else {
            binding.set(None);
            return;
        };
        if binding.peek().as_ref().is_some_and(|old| {
            old.owner.scope.current(&state.peek())
                && old.owner.visit == owner.visit
                && old.read == serial
                && old.root == root
        }) {
            return;
        }
        binding.set(bind(state, model));
    });
    let Some(binding) = binding.read().clone() else {
        return rsx! {p{class:"canvas-comments-note","Open an available comment thread to write a reply."}};
    };
    let shell = state.read();
    let Some(e) = entry(&shell, &binding.id) else {
        return rsx! {};
    };
    let text = e.text.clone();
    let scope = binding.field_scope();
    let locked = e
        .locked
        .clone()
        .or_else(|| e.inventory.as_ref().and_then(|i| i.attempt().cloned()));
    let versions = e
        .inventory
        .as_ref()
        .map(|i| i.drafts().to_vec())
        .unwrap_or_default();
    let disabled = e.loading || e.inventory.is_none() || versions.len() > 1;
    let busy = e.sending || e.saving || e.native_busy;
    let composing = e.composing;
    let retired = e.retired_text.as_ref() == Some(&text);
    let issue = e.error;
    let checked = e.reviewed;
    let outcome = e.outcome.is_some();
    let writable = {
        let value = model.read();
        value
            .file
            .as_ref()
            .zip(value.root.as_ref())
            .is_some_and(|(file, root)| ReplyIntent::available(file, root))
    };
    let send_disabled = !writable
        || disabled
        || busy
        || composing
        || retired
        || locked.is_some()
        || text.trim().is_empty()
        || issue.is_some()
        || model.read().loading
        || model.read().cooling();
    let editable = binding.current(&shell, &model.read());
    rsx! {
        section {class:"canvas-comment-reply", "aria-label":"Reply to this comment thread",
            if disabled && e.loading {p{role:"status","Loading local reply..."}}
            if versions.len()>1 {
                p{role:"status","Several saved versions need review. Choose one before sending; nothing is selected automatically."}
                if e.serial>0 && !versions.iter().any(|d|d.capture()==e.capture&&d.text()==e.text) {
                    article{p{"Current reply retained in this app (not yet saved)"}p{class:"canvas-comment-reply-version","{text}"}}
                }
                for draft in versions {article{key:"{draft.id()}",p{class:"canvas-comment-reply-version","{draft.text()}"}
                    button{"data-canvas-comment-action":"reply-version",disabled:busy,onclick:{let b=binding.clone();move |_|choose_version(state,model,b.clone(),draft.clone())},"Use this version and discard the other shown versions"}
                }}
            } else if e.inventory.is_some() {
                textarea {id:"canvas-comment-reply",key:"{scope}",class:"canvas-comment-reply-field",initial_value:"{text}",rows:3,
                    "data-draft-scope":"{scope}","data-composing":if composing{"true"}else{"false"},"aria-label":"Reply",disabled:disabled||!editable,
                    oninput:{let b=binding.clone();move |event|change(state,model,&b,event.value())},
                    oncompositionstart:{let b=binding.clone();move |_|{let mut shell=state.write();if b.current(&shell,&model.peek()){if let Some(e)=entry_mut(&mut shell,&b.id){e.composing=true;}}}},
                    oncompositionend:{let b=binding.clone();move |_|{let mut shell=state.write();if b.current(&shell,&model.peek()){if let Some(e)=entry_mut(&mut shell,&b.id){e.composing=false;}}}}
                }
            }
            if !writable {p{role:"status","This thread is read-only or resolved. Your saved reply is retained."}}
            if retired {p{role:"status","This text belongs to an already settled or archived attempt. Edit it to write a new reply; it will not be sent again automatically."}}
            if let Some(issue)=issue {p{role:"alert","{issue}"}}
            if e.saving {p{role:"status","Saving reply locally..."}}
            if let Some(receipt)=locked {
                p{role:"status","An earlier reply may have been sent. Automatic resend is disabled."}
                details {summary{"Review the original reply"}p{class:"canvas-comment-reply-version","{receipt.record().text()}"}}
                if receipt.record().root_ts()==binding.root {
                    label{input{id:"canvas-reply-reviewed",r#type:"checkbox",checked,disabled:busy,onchange:{let b=binding.clone();move |event|{let mut shell=state.write();if b.current(&shell,&model.peek()){if let Some(e)=entry_mut(&mut shell,&b.id){e.reviewed=event.checked();}}}}},"I reviewed the current thread and understand that the reply outcome is unknown."}
                    button{"data-canvas-comment-action":"reply",id:"canvas-reply-archive",disabled:!checked||busy||model.read().loading||model.read().error.is_some()||model.read().cooling(),onclick:{let b=binding.clone();move |_|delivery::review(state,model,b.clone())},"Archive unconfirmed attempt"}
                }else{p{"Open the original thread to review this file's pending reply."}}
            }
            div {class:"canvas-comments-toolbar",
                button{"data-canvas-comment-action":"reply",id:"canvas-reply-send",disabled:send_disabled,onclick:{let b=binding.clone();move |_|delivery::send(state,model,b.clone())},"Send reply"}
                if issue.is_some(){button{"data-canvas-comment-action":"reply",id:"canvas-reply-retry",disabled:busy,onclick:{let b=binding.clone();move |_|{if outcome{delivery::retry_result(state,model,b.clone())}else{retry(state,b.clone())}}},if outcome{"Retry local result save"}else{"Retry local save"}}}
                button {"data-canvas-comment-action":"reply-history",disabled:busy,onclick:{let b=binding.clone();move |_|history(state,model,b.clone(),false)},"Earlier local attempts"}
                p{class:"canvas-comments-note","Plain text. Replies stay in this canvas thread."}
            }
            if let Some(rows)=&e.history {
                details {open:true,summary {"Earlier local attempts in this canvas"}
                    for row in rows {article{key:"{row.receipt.id()}",p{"{row.status}"}p{class:"canvas-comment-reply-version","{row.receipt.record().text()}"}}}
                    if rows.len()==20{button{"data-canvas-comment-action":"reply-version",disabled:busy,onclick:{let b=binding.clone();move |_|history(state,model,b.clone(),true)},"Next local attempts"}}
                }
            }
        }
    }
}
