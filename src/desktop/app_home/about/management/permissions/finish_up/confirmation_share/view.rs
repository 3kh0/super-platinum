use super::*;
pub(in super::super) fn panel(state: Signal<ShellState>, visit: u64, id: u64) -> Element {
    let s = state.read();
    let a = attempt(&s, id);
    let v = a.and_then(|a| a.confirmation_share.as_ref());
    let active = v.is_some_and(|v| v.active) && a.is_some_and(|a| a.local.share && !a.closed);
    let generation = v.map(|v| v.generation).unwrap_or_default();
    let recipients = v.map(State::recipient_rows).unwrap_or_default();
    let recipient_revision = v.map(|v| v.recipient_revision).unwrap_or_default();
    let selected_recipient = v.and_then(|v| v.selected_recipient);
    let epoch = v.map(|v| v.epoch).unwrap_or_default();
    let text = v.map(|v| v.text.clone()).unwrap_or_default();
    let selected = v.map(|v| v.selected.clone()).unwrap_or_default();
    let loading = v.is_none_or(|v| v.loading);
    let bound = v.is_some_and(|v| v.bound);
    let busy = v.is_some_and(|v| v.operation.is_some());
    let searching = v.is_some_and(|v| v.search.busy());
    let query_revision = v.map(|v| v.search.revision).unwrap_or_default();
    let search_notice = v.and_then(|v| v.search.notice());
    let notice = v.and_then(|v| v.notice.clone());
    let rows = v.map(State::rows).unwrap_or_default();
    let opening =
        v.is_some_and(|v| v.loading && (v.opening.is_some() || v.group_opening.is_some()));
    let allowed = v
        .and_then(|v| v.listing.as_ref())
        .is_some_and(|l| l.current(&s) && l.find(&selected).is_some_and(|r| r.current(&s)));
    let workflow = a
        .and_then(|a| a.publication_reply.as_ref())
        .and_then(|r| match r {
            Reply::Raw(raw) => raw.get("decorated_workflow"),
            _ => None,
        });
    let title = workflow
        .and_then(|w| w["title"].as_str())
        .unwrap_or_default()
        .to_owned();
    let description = workflow
        .and_then(|w| w["description"].as_str())
        .unwrap_or_default()
        .to_owned();
    drop(s);
    rsx! {
        section{id:"workflow-finish-share-pane",class:"workflow-finish-pane",style:if active{"display: flex;"}else{"display: none;"},
            "data-share-generation":generation,"data-share-epoch":epoch,"data-share-selected":selected.clone(),
            "data-share-query-revision":query_revision,"data-recipient-revision":recipient_revision,
            div{id:"workflow-share-recipients",class:"workflow-share-recipients", "aria-label":"Selected people",
                for (index,recipient) in recipients.into_iter().enumerate() {
                    {let peer_id=recipient["id"].as_str().unwrap_or_default().to_owned(); let label=recipient["label"].as_str().unwrap_or_default().to_owned();
                    rsx!{span{key:"{peer_id}",class:"workflow-share-recipient","data-workflow-share-recipient":peer_id.clone(),"data-selected":(selected_recipient==Some(index)).to_string(),
                        "{label}"
                        button{r#type:"button",class:"icon-btn","aria-label":format!("Remove {label}"),"data-workflow-share-recipient-remove":peer_id.clone(),disabled:!bound||busy,
                            onmousedown:move|_|recipients::event(state,visit,id,"recipientRemove",peer_id.clone()),
                            {crate::icons::icon(crate::icons::Icon::Close,"icon")}
                        }
                    }}}
                }
            }
            label{r#for:"workflow-share-query","Find a conversation"}
            // The query buffer belongs to the original input while debounce
            // is pending; a controlled value would erase the new keystroke on
            // the input-serial render before the model commits it.
            input{id:"workflow-share-query",r#type:"search",placeholder:"Search channels or people",
                readonly:loading||busy,
                onkeydown:move|e|{let key=e.key().to_string();if key=="Backspace"||key=="Delete"{recipients::event(state,visit,id,"recipientKey",key);}},
                oninput:move|e|search::input(state,visit,id,e.value())}
            label{r#for:"workflow-share-destination","Choose where to send this message"}
            // The selected D option may be created by this same render. Dioxus
            // updates parent attributes before children, so selecting through
            // each option keeps the original SELECT without an early value
            // assignment to an option that does not exist yet.
            select{id:"workflow-share-destination",disabled:!bound||loading||busy||searching,
                onchange:move|e|edit(state,visit,id,"select",e.value()),
                option{value:"",selected:selected.is_empty(),"Choose a conversation"}
                for (value,label) in rows {option{value:value.clone(),selected:value==selected,"{label}"}}
            }
            p{class:"workflow-share-help","Choose a channel or add people."}
            button{id:"workflow-share-refresh",r#type:"button",disabled:!bound||loading||busy||searching,onclick:move |_|edit(state,visit,id,"refresh",String::new()),"Refresh destinations"}
            if let Some(message)=search_notice {p{id:"workflow-share-search-status",role:"status","{message}"}}
            if loading {p{role:"status",if opening {"Opening conversation\u{2026}"} else {"Loading destinations\u{2026}"}}}
            label{r#for:"workflow-share-message","Add a message, if you\u{2019}d like."}
            textarea{id:"workflow-share-message",value:text,readonly:!bound||loading||busy||searching,
                oninput:move|e|edit(state,visit,id,"text",e.value())}
            section{class:"workflow-finish-preview","aria-label":"Workflow preview",strong{"{title}"}p{"{description}"}}
            if let Some(notice)=notice {p{id:"workflow-share-notice",role:"status","{notice}"}}
            if busy {p{role:"status","Sending message\u{2026}"}}
            div{class:"workflow-finish-pane-actions",
                button{id:"workflow-share-cancel",r#type:"button",disabled:!bound,onclick:move |_|cancel(state,visit,id),"Cancel"}
                button{id:"workflow-share-send",r#type:"button",disabled:!bound||loading||busy||searching||!allowed,onclick:move |_|run::send(state,visit,id),"Send"}
            }
        }
    }
}
