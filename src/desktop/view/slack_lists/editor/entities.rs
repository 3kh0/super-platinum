//! Native labelled controls keep selection local until the editor's Save.
use super::*;
use std::{collections::BTreeMap, sync::Arc};

#[component]
pub(super) fn EntityPicker(
    state: Signal<ShellState>,
    generation: u64,
    mut selected: Signal<Vec<String>>,
    initial: Arc<Vec<crate::slack_lists::entities::Choice>>,
    directory: crate::slack_lists::entities::Presentation,
    kind: crate::slack_lists::entities::Kind,
    multi: bool,
    disabled: bool,
) -> Element {
    let mut labels = use_signal(move || {
        initial
            .iter()
            .map(|choice| (choice.id.clone(), choice.label.clone()))
            .collect::<BTreeMap<_, _>>()
    });
    // Desktop event/render IPC can lag native typing. Seed once so an older
    // result render cannot assign a stale query over newer browser input.
    // CellEditor is keyed by generation, so each new editor gets its own seed.
    let seed = directory.query.clone();
    let initial_query = use_hook(move || seed);
    let users = kind == crate::slack_lists::entities::Kind::Users;
    let query_id = if users {
        "slack-list-user-query"
    } else {
        "slack-list-channel-query"
    };
    let input_name = if users {
        "slack-list-user-choice"
    } else {
        "slack-list-channel-choice"
    };
    rsx! {div {class:"slack-list-entity-picker slack-list-user-picker",
        onmounted:move |_|crate::slack_lists::entities::search(state,generation,String::new()),
        label{r#for:query_id,if users{"Find people"}else{"Find channels"}}
        input{id:query_id,r#type:"search",initial_value:initial_query,disabled,
            autocomplete:"off",oninput:move |event|crate::slack_lists::entities::search(state,generation,event.value())}
        fieldset {disabled,legend{if users{"Selected people"}else{"Selected channels"}}
            if selected.read().is_empty(){p{class:"slack-list-editor-hint",if users{"No one selected"}else{"No channels selected"}}}
            for id in selected.read().iter() {
                {
                    let id=id.clone();let remove=id.clone();let label=labels.read().get(&id).cloned().unwrap_or_else(||id.clone());
                    rsx! {div{class:"slack-list-user-selected",key:"selected-{id}","data-selected-user":users.then(||id.clone()),"data-selected-channel":(!users).then(||id.clone()),
                        span{"{label}"}button{r#type:"button","aria-label":"Remove {label}",onclick:move |_|selected.write().retain(|id|id!=&remove),"Remove"}
                    }}
                }
            }
        }
        DirectoryChoices {kind,directory,selected:selected.read().clone(),multi,disabled,input_name:input_name.to_owned(),toggle_single:false,
            activate:move |(id,label,checked):(String,String,bool)|{
                labels.write().insert(id.clone(),label);
                if multi {let mut values=selected.write();values.retain(|value|value!=&id);if checked{values.push(id);}}
                else {selected.set(vec![id]);}
            }}
        p{class:"slack-list-editor-hint","Changes are applied when you save."}
    }}
}

/// Shared labelled directory surface; owner adapters supply admission and changes.
/// Ordinary cell radio change remains unchanged. Creation can activate a selected
/// radio again because its source callback uses full stored-default equality.
#[component]
pub(crate) fn DirectoryChoices(
    kind: crate::slack_lists::entities::Kind,
    directory: crate::slack_lists::entities::Presentation,
    selected: Vec<String>,
    multi: bool,
    disabled: bool,
    input_name: String,
    toggle_single: bool,
    activate: EventHandler<(String, String, bool)>,
) -> Element {
    let users = kind == crate::slack_lists::entities::Kind::Users;
    rsx! {
        fieldset {disabled,legend{if users {if multi{"Choose people"}else{"Choose a person"}}else {if multi{"Choose channels"}else{"Choose a channel"}}}
            div{class:"slack-list-user-results","aria-busy":directory.loading,
                for choice in directory.candidates.iter() {
                    {
                        let id=choice.id.clone();let click_id=id.clone();let label=choice.label.clone();let click_label=label.clone();let checked=selected.contains(&id);
                        rsx! {label{class:"slack-list-choice",key:"candidate-{choice.id}",
                            input{r#type:if multi{"checkbox"}else{"radio"},name:input_name.clone(),value:"{choice.id}",checked,
                                onclick:move |_|{if toggle_single&&!multi{activate.call((click_id.clone(),click_label.clone(),true));}},
                                onchange:move |event|{if multi||!toggle_single{activate.call((id.clone(),label.clone(),event.checked()));}}}
                            span{"{choice.label}"}
                        }}
                    }
                }
                if directory.candidates.is_empty()&&!directory.loading {p{class:"slack-list-editor-hint",if users{"No matching people"}else{"No matching channels"}}}
            }
        }
        if directory.loading{p{class:"slack-list-editor-hint",role:"status","Searching…"}}
        if let Some(error)=directory.error{p{class:"slack-list-error",role:"status","{error}"}}
    }
}

#[cfg(test)]
mod choice_tests;
