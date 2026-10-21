use super::*;
pub(super) fn panel(state: Signal<ShellState>, visit: u64, id: u64) -> Element {
    let s = state.read();
    let v = attempt(&s, id).and_then(|a| a.confirmation_add.as_ref());
    let selected = v.map(|v| v.selected.clone()).unwrap_or_default();
    let epoch = v.map(|v| v.epoch).unwrap_or(0);
    let loading = v.is_none_or(|v| v.loading);
    let pending = v.is_some_and(|v| v.operation.is_some());
    let rows = v
        .and_then(|v| v.listing.as_ref())
        .map(|l| {
            l.rows()
                .iter()
                .map(|c| (c.id().to_owned(), c.label().to_owned()))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let notice = v.and_then(|v| v.notice.clone());
    let ready = selected.is_empty()
        || v.and_then(|v| v.choice.as_ref())
            .is_some_and(|c| c.current(&s));
    drop(s);
    rsx! {
      section{id:"workflow-confirmation-add","data-add-epoch":epoch,"data-add-selected":selected.clone(),onmounted:move |_|mount(state,visit,id),
       label{r#for:"workflow-confirmation-channel","Add this workflow to a channel"}
       p{"Choose from verified channels known to this client."}
       select{id:"workflow-confirmation-channel",value:selected.clone(),disabled:loading||pending,
        onmounted:move |_|bind_visible(state,visit,id),onchange:move |e|select(state,visit,id,e.value()),
        option{value:"","No channel"}
        for (channel,label) in rows {option{value:channel,"{label}"}}
       }
       if loading {p{role:"status","Loading channel choices\u{2026}"}}
       if let Some(notice)=notice {p{role:"status","{notice}"}}
       button{id:"workflow-confirmation-retry",r#type:"button",disabled:loading||pending,onclick:move |_|refresh(state,visit,id),"Refresh channel choices"}
       button{id:"workflow-finish-done",r#type:"button",disabled:pending||(!selected.is_empty()&&(loading||!ready)),onclick:move |_|run::submit(state,visit,id),
        if selected.is_empty(){"Done"}else{"Add to Channel"}
       }
      }
    }
}
