//! Creation controls have a real registry owner, distinct from cell drafts.
use super::*;
use crate::slack_lists::rename::text_create::entity::{
    self as draft, Presentation,
    settings::{self, Edit},
};
use super_platinum_core::slack_lists::{EntityCreateDomain, EntityCreateFormat};

#[component]
pub(super) fn Settings(state: Signal<ShellState>, p: Presentation, enabled: bool) -> Element {
    let stamp = p.stamp;
    let enabled = enabled && p.enabled;
    let users = p.options.domain() == EntityCreateDomain::People;
    let multiple = p.options.format() == EntityCreateFormat::Multiple;
    // This component is keyed only by the real registry/Type owner, not query or
    // configuration epochs. An old result render cannot overwrite native typing.
    let seed = p.directory.query.clone();
    let initial_query = use_hook(move || seed);
    let selected = if multiple {
        p.options.selected_ids().to_vec()
    } else {
        p.options
            .single_display_default()
            .map(str::to_owned)
            .into_iter()
            .collect()
    };
    rsx! {
        fieldset{id:"slack-list-entity-settings",class:"slack-list-entity-picker slack-list-user-picker",disabled:!enabled,
            "data-entity-visit":"{stamp.visit}","data-entity-selection":"{stamp.selection}",
            "data-entity-epoch":"{stamp.epoch}","data-entity-choice":"{stamp.choice.revision}",
            legend{"Settings"}
            label{class:"slack-list-choice",
                input{id:"slack-list-entity-multiple",r#type:"checkbox",checked:multiple,disabled:!enabled,
                    onchange:move |event|settings::edit(state,stamp,Edit::Format(if event.checked(){EntityCreateFormat::Multiple}else{EntityCreateFormat::Single}))}
                span{"Allow multiple selections"}}
            if users {label{class:"slack-list-choice",
                input{id:"slack-list-entity-show-names",r#type:"checkbox",checked:p.options.show_names(),disabled:!enabled,
                    onchange:move |event|settings::edit(state,stamp,Edit::ShowNames(event.checked()))}
                span{"Show names"}}}
            label{r#for:"slack-list-entity-default-query","Default"}
            input{id:"slack-list-entity-default-query",r#type:"search",initial_value:initial_query,autocomplete:"off",disabled:!enabled,
                placeholder:if users{"Find people"}else{"Find channels"},
                onmounted:move |_|draft::search(state,stamp,String::new()),
                oninput:move |event|draft::search(state,stamp,event.value())}
            if !multiple { if let Some(choice)=p.selected.first() {p{class:"slack-list-editor-hint","Current default: {choice.label}"}} }
            if multiple { {p.selected.into_iter().map(|choice| {
                let id=choice.id.clone(); rsx! { div{key:"default-{choice.id}",class:"slack-list-user-selected",
                    span{"{choice.label}"}
                    button{r#type:"button",disabled:!enabled,"data-entity-remove":"{choice.id}","aria-label":"Remove default {choice.label}",
                        onclick:move |_|focus::remove(state,stamp,id.clone()),"Remove"}
                }}
            })} }
            super::super::editor::entities::DirectoryChoices{
                kind:if users{crate::slack_lists::entities::Kind::Users}else{crate::slack_lists::entities::Kind::Channels},
                directory:p.directory,selected,multi:multiple,disabled:!enabled,
                input_name:"slack-list-entity-default-choice",toggle_single:true,
                activate:move |(id,_,_):(String,String,bool)|settings::edit(state,stamp,Edit::Activate(id))
            }
        }
    }
}

mod focus;

#[cfg(test)]
mod tests;
