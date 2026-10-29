//! Staged local known-channel/known-author pickers; no remote discovery.
use super::*;
use crate::state::search_message_scopes as intent;
use super_platinum_core::search_query::message_scopes::Scope;
pub(super) fn panel(state: Signal<ShellState>, s: &ShellState) -> Element {
    if !intent::typed_panel(s) {
        return rsx! {};
    }
    rsx! {div{class:"search-scope-grid",
        {picker(state,s,Scope::Channels,"channels","In channels","Find known channels")}
        {picker(state,s,Scope::Authors,"authors","From people","Find known people")}
    }p{"Other search terms are kept. Changes apply when you choose Apply filters."}}
}
fn picker(
    mut state: Signal<ShellState>,
    s: &ShellState,
    kind: Scope,
    group_key: &str,
    label: &str,
    find_label: &str,
) -> Element {
    let source = intent::Source::capture(s);
    let clear = source.clone();
    let find = source.clone();
    let input_key = format!(
        "{group_key}-{}",
        source.as_ref().map(|s| s.key()).unwrap_or("presentation")
    );
    let count = intent::count(s, kind);
    let rows = intent::rows(s, kind);
    let empty = rows.is_empty();
    let needs_refresh = intent::has_draft(s) && source.is_none();
    rsx! {section{class:"search-scope-picker",role:"group","aria-label":label,
        div{class:"search-scope-heading",strong{"{label}"}span{"{count} selected"}
            button{r#type:"button",class:"chip","data-message-scope-clear":group_key,disabled:clear.is_none(),onclick:move |_|{if let Some(source)=clear.as_ref(){intent::clear(&mut state.write(),source,Some(kind),false);}},"Clear"}
        }
        input{key:"{input_key}","aria-label":find_label,placeholder:find_label,initial_value:intent::find_text(s,kind),disabled:find.is_none(),autocomplete:"off",spellcheck:"false",
            oninput:move|event|{if let Some(source)=find.as_ref(){intent::find(&mut state.write(),source,kind,event.value());}},
            onkeydown:move|event|{if event.key()==Key::Enter{event.prevent_default();event.stop_propagation();}}
        }
        if needs_refresh{p{role:"status","Reopen filters to refresh these choices."}}else if empty{p{role:"status","No matching known entries."}}
        div{class:"search-scope-window",
            for row in rows{
                label{class:"search-scope-row",key:"{group_key}-{row.id}",
                    input{r#type:"checkbox","data-message-scope-kind":group_key,"data-message-scope-id":row.id.clone(),"aria-label":if row.username.is_empty(){format!("{label}: {}",row.label)}else{format!("{label}: {} (@{})",row.label,row.username)},checked:row.selected,disabled:source.is_none()||!row.current,
                        onchange:{let source=source.clone();let id=row.id.clone();move|event|{if let Some(source)=source.as_ref(){intent::set(&mut state.write(),source,kind,&id,event.checked());}}}
                    }
                    span{"{row.label}"}
                    if !row.username.is_empty(){small{"@{row.username}"}}
                }
            }
        }
    }}
}
