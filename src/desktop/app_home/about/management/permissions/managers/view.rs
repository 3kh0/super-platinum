use super::*;
pub(in crate::app_home::about::management) fn panel(
    state: Signal<ShellState>,
    visit: u64,
    id: u64,
) -> Element {
    let s = state.read();
    let Some(c) = settings::owned(&s, visit, id) else {
        return rsx! {};
    };
    let Some(m) = c.managers.as_ref() else {
        return rsx! {p{role:"status","Workflow managers could not be loaded."}};
    };
    let ids = m.ids();
    let epoch = m.epoch;
    rsx! {
     div{id:"workflow-permissions-manager-settings","data-manager-epoch":epoch,
      label{r#for:"workflow-permissions-manager-query","Workflow managers"}
      div{for target in &ids {button{key:"manager-{epoch}-{target}",r#type:"button","data-manager-selected":target.as_str(),disabled:m.pending||m.speed_bump,
       onmounted:move |_|bind(state,visit,id),onclick:{let target=target.clone();move |_|change(state,visit,id,"remove",target.clone())},
       {m.rows.get(target).map(|r|r.label.as_str()).unwrap_or(target.as_str())}}}}
      input{id:"workflow-permissions-manager-query",r#type:"search",value:m.query.clone(),readonly:(m.pending&&!m.facts_loading())||m.speed_bump,"aria-label":"Add workflow managers",
       onmounted:move |_|bind(state,visit,id),oninput:move|e|search(state,visit,id,e.value())}
      div{for target in &m.results {if let Some(row)=m.rows.get(target){button{key:"manager-result-{epoch}-{target}",r#type:"button","data-manager-result":target.as_str(),disabled:m.pending||m.speed_bump,
       onmounted:move |_|bind(state,visit,id),onclick:{let target=target.clone();move |_|change(state,visit,id,"add",target.clone())},"{row.label}"}}}}
      if ids.is_empty(){p{role:"status","You must provide at least 1 workflow manager"}}
      if m.pending{p{role:"status","Loading workflow managers\u{2026}"}}
      if let Some(notice)=&m.notice{p{role:"status","{notice}"}}
      if m.facts_retry(){button{id:"workflow-permissions-manager-agent-retry",r#type:"button",disabled:m.pending||m.speed_bump,onclick:move |_|acquire::retry_facts(state,visit,id),"Retry manager eligibility"}}
      if !m.missing().is_empty(){button{id:"workflow-permissions-manager-retry",r#type:"button",disabled:m.pending||m.speed_bump,onclick:move |_|acquire::start(state,visit,id,true,true),"Retry manager details"}}
     }
     if m.speed_bump {section{id:"workflow-permissions-self-removal",role:"region","aria-labelledby":"workflow-permissions-self-heading",
      h3{id:"workflow-permissions-self-heading","Remove yourself as a workflow manager?"}
      p{"Removing yourself as a workflow manager means you\u{2019}ll immediately lose access to edit this workflow. To become a workflow manager of this workflow again, a current manager will need to add you."}
      button{id:"workflow-permissions-self-cancel",r#type:"button",onclick:move |_|bump_cancel(state,visit,id),"Cancel"}
      button{id:"workflow-permissions-self-save",r#type:"button",onclick:move |_|super::super::confirm(state,visit,id),"Save"}
     }}
    }
}
