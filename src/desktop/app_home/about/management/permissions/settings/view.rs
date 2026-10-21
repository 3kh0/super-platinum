use super::*;
pub(in crate::app_home::about::management) fn panel(
    state: Signal<ShellState>,
    visit: u64,
    id: u64,
) -> Element {
    let s = state.read();
    let Some(c) = owned(&s, visit, id) else {
        return rsx! {};
    };
    let Some(v) = c.settings.as_ref() else {
        return rsx! {};
    };
    let mode = v.display_mode().unwrap_or("");
    let ids = v.draft.as_ref().map(Draft::ids).unwrap_or_default();
    let external = v.draft.as_ref().is_some_and(Draft::external_enabled);
    let show_external =
        mode != "app_collaborators" && (external || v.policy.any_external() == Ok(true));
    let shortcut = c
        .original
        .editor
        .trigger()
        .is_some_and(|t| t["type"] == "shortcut");
    let team = &c.original.owner.ctx.session.name;
    let epoch = v.epoch;
    let external_disabled = v.policy.external_checkbox_enabled() != Ok(true);
    rsx! {
        div{id:"workflow-permissions-run-settings","data-settings-epoch":epoch,
            label{r#for:"workflow-permissions-run",if shortcut{"Who can find and use this workflow?"}else{"Who can find this workflow?"}}
            select{id:"workflow-permissions-run",value:mode,disabled:v.draft.is_none(),
                onmounted:move |_|actions::bind_settings(state,visit,id),
                onchange:move |e|change(state,visit,id,json!({"kind":"mode","value":e.value()})),
                option{value:"everyone","Everyone internal in {team}"}
                option{value:"named_entities","Workflow managers, plus specific people, channels, and workspaces"}
                option{value:"app_collaborators","Workflow managers only"}
            }
            if mode=="named_entities" {
                div{id:"workflow-permissions-targets",
                    for (index,target) in ids.iter().enumerate().filter(|(_,target)|v.context.is_external(target)!=Ok(true)) {
                        button{key:"{epoch}-{index}",r#type:"button","data-run-selected":target.as_str(),
                            onmounted:move |_|actions::bind_settings(state,visit,id),
                            onclick:{let target=target.clone();move |_|change(state,visit,id,json!({"kind":"remove","value":target}))},
                            {v.rows.get(target).map(|r|r.label.as_str()).unwrap_or("Unavailable reference")}
                        }
                    }
                }
                label{r#for:"workflow-permissions-target-query","Add people, channels, or workspaces"}
                input{id:"workflow-permissions-target-query",r#type:"search",value:v.query.clone(),
                    oninput:move|e|search(state,visit,id,false,e.value()),onmounted:move |_|actions::bind_settings(state,visit,id)}
                div{id:"workflow-permissions-target-results",
                    for target in &v.results {if let Some(row)=v.rows.get(target) {
                        button{key:"{epoch}-{target}",r#type:"button","data-run-result":target.as_str(),"data-kind":format!("{:?}",row.kind),
                            onmounted:move |_|actions::bind_settings(state,visit,id),
                            onclick:{let target=target.clone();move |_|change(state,visit,id,json!({"kind":"add","value":target}))},"{row.label}"}
                    }}
                }
            }
            if show_external {
                label{r#for:"workflow-permissions-external",
                    input{id:"workflow-permissions-external",r#type:"checkbox",checked:external,disabled:external_disabled,
                        onmounted:move |_|actions::bind_settings(state,visit,id),
                        onchange:move|e|change(state,visit,id,json!({"kind":"external","value":e.checked()}))}
                    "Also include connected, external organizations to find and run. "
                }
                if external {
                    div{id:"workflow-permissions-external-targets",
                        for (index,target) in ids.iter().enumerate().filter(|(_,target)|v.context.is_external(target)==Ok(true)) {
                            button{key:"{epoch}-{index}",r#type:"button","data-run-selected":target.as_str(),
                                onmounted:move |_|actions::bind_settings(state,visit,id),
                                onclick:{let target=target.clone();move |_|change(state,visit,id,json!({"kind":"remove","value":target}))},
                                {v.rows.get(target).map(|r|r.label.as_str()).unwrap_or("Unavailable reference")}
                            }
                        }
                    }
                    label{r#for:"workflow-permissions-external-query","Add external organizations"}
                    input{id:"workflow-permissions-external-query",r#type:"search",value:v.external_query.clone(),
                        onmounted:move |_|actions::bind_settings(state,visit,id),oninput:move|e|search(state,visit,id,true,e.value())}
                    div{id:"workflow-permissions-external-results",
                        for target in &v.external_results {if let Some(row)=v.rows.get(target) {
                            button{key:"{epoch}-{target}",r#type:"button","data-run-result":target.as_str(),"data-kind":format!("{:?}",row.kind),
                                onmounted:move |_|actions::bind_settings(state,visit,id),
                                onclick:{let target=target.clone();move |_|change(state,visit,id,json!({"kind":"add","value":target}))},"{row.label}"}
                        }}
                    }
                }
            }
            if v.pending {p{role:"status","Loading reference details\u{2026}"}}
            if let Some(notice)=&v.notice {p{role:"status","{notice}"}
                if hydrate::has_missing(v,&c.original){
                    button{r#type:"button",id:"workflow-permissions-target-retry",disabled:v.pending,onclick:move |_|hydrate::start(state,visit,id,true),"Retry reference details"}}}
            if v.draft.is_none(){p{role:"status","Current run-access references could not be resolved. Your existing permissions are retained."}}
            else if v.desired().is_err() && !v.pending {p{role:"status","Resolve the selected references and permitted organizations before saving."}}
        }
    }
}
