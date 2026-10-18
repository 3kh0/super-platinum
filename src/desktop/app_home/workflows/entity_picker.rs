//! One retained picker per workflow. Search text is never a selected entity ID.
//! The result boundary owns query/owner checks; selection retains its evidence.
use super::*;
use super_platinum_core::block_actions::home::workflow::{Input, InputType};

mod results;
pub(super) use results::Selection;

pub(super) fn start(mut state: Signal<ShellState>, id: u64) {
    let work = {
        let mut s = state.write();
        if !dialog_current(&s, id) || super::busy(&s, id) {
            return;
        }
        let d = s.app_home.workflow.as_ref().unwrap();
        if !d.review.ready()
            || !ready(&s, id, &d.review)
            || salesforce_dispatch::unresolved(&s, &d.origin)
        {
            return;
        }
        if d.entity_selected.is_empty() {
            None
        } else {
            let token = next();
            let work = (
                token,
                d.origin.clone(),
                d.review.clone(),
                d.entity_selected.clone(),
            );
            let d = s.app_home.workflow.as_mut().unwrap();
            d.entity_validation = Some(token);
            d.notice = Some("Checking selected people and conversations…".into());
            Some(work)
        }
    };
    let Some((token, origin, review, selections)) = work else {
        super::start_checked(state, id);
        return;
    };
    dioxus::core::spawn_forever(async move {
        let mut refreshed = std::collections::BTreeMap::new();
        let mut failed = false;
        for (key, selection) in selections {
            match selection.refresh(state, id, token, &review).await {
                Ok(value) => {
                    refreshed.insert(key, value);
                }
                Err(_) => {
                    failed = true;
                    break;
                }
            }
        }
        let accepted = {
            let mut s = state.write();
            if !origin.current(&s)
                || !s.app_home.workflow.as_ref().is_some_and(|d| {
                    d.id == id && d.entity_validation == Some(token) && d.review == review
                })
            {
                return;
            }
            let all_current = !failed && refreshed.values().all(|v| v.current(&s));
            let d = s.app_home.workflow.as_mut().unwrap();
            d.entity_validation = None;
            if all_current {
                d.entity_selected = refreshed;
                d.notice = None;
                true
            } else {
                d.notice=Some("A selected result could not be checked. Review the inputs before starting; nothing was sent.".into());
                false
            }
        };
        if accepted {
            super::start_checked(state, id);
        }
    });
}

/// Also called after the native input lease is acquired, immediately before the
/// existing once-only Start request. A replaced value cannot borrow old proof.
pub(super) fn ready(s: &ShellState, id: u64, review: &Preflight) -> bool {
    let Some(d) = s
        .app_home
        .workflow
        .as_ref()
        .filter(|d| d.id == id && d.review == *review)
    else {
        return false;
    };
    if d.entity.is_some() {
        return false;
    }
    [(false, review.context()), (true, review.custom())]
        .into_iter()
        .all(|(custom, fields)| {
            fields
                .iter()
                .filter(|f| matches!(f.kind, InputType::User | InputType::Conversation))
                .all(|f| {
                    d.entity_selected
                        .get(&(custom, f.name.clone()))
                        .is_some_and(|selected| {
                            selected.matches(review, custom, &f.name) && selected.current(s)
                        })
                })
        })
}

pub(super) struct Picker {
    field: Input,
    custom: bool,
    query: String,
    generation: u64,
    loading: bool,
    selecting: bool,
    choices: Vec<results::Choice>,
    error: Option<String>,
}

fn current(s: &ShellState, id: u64, generation: u64) -> bool {
    dialog_current(s, id)
        && !super::busy(s, id)
        && s.app_home.workflow.as_ref().is_some_and(|d| {
            d.entity
                .as_ref()
                .is_some_and(|p| p.generation == generation)
        })
}

fn open(mut state: Signal<ShellState>, id: u64, field: Input, custom: bool) {
    {
        let mut s = state.write();
        if !dialog_current(&s, id) || super::busy(&s, id) {
            return;
        }
        let d = s.app_home.workflow.as_mut().unwrap();
        let declared = if custom {
            d.review.custom()
        } else {
            d.review.context()
        };
        if !declared.iter().any(|f| f == &field)
            || !matches!(field.kind, InputType::User | InputType::Conversation)
        {
            return;
        }
        d.canvas = None;
        d.entity = Some(Picker {
            field,
            custom,
            query: String::new(),
            generation: next(),
            loading: false,
            selecting: false,
            choices: Vec::new(),
            error: None,
        });
    }
    search(state, id, String::new());
}

fn search(mut state: Signal<ShellState>, id: u64, text: String) {
    let field = {
        let s = state.peek();
        if !dialog_current(&s, id) || super::busy(&s, id) {
            return;
        }
        let Some(p) = s.app_home.workflow.as_ref().and_then(|d| d.entity.as_ref()) else {
            return;
        };
        if p.selecting {
            return;
        }
        (p.field.name.clone(), p.custom)
    };
    let Some(query) = entity_search::Query::capture(state, id, field.0, field.1, text.clone())
    else {
        return;
    };
    {
        let mut s = state.write();
        let Some(p) = s.app_home.workflow.as_mut().and_then(|d| d.entity.as_mut()) else {
            return;
        };
        p.query = text;
        p.generation = query.generation;
        p.loading = true;
        p.error = None;
        p.choices.clear();
    }
    dioxus::core::spawn_forever(async move {
        let result = results::load(state, &query).await;
        let mut s = state.write();
        if !query.current(&s) || !current(&s, id, query.generation) {
            return;
        }
        let p = s
            .app_home
            .workflow
            .as_mut()
            .unwrap()
            .entity
            .as_mut()
            .unwrap();
        p.loading = false;
        match result {
            Ok(choices) => p.choices = choices,
            Err(_) => p.error = Some(
                "Results could not be checked. Retry this search when the connection is available."
                    .into(),
            ),
        }
    });
}

fn choose(mut state: Signal<ShellState>, id: u64, generation: u64, index: usize) {
    let choice = {
        let mut s = state.write();
        if !current(&s, id, generation) {
            return;
        }
        let p = s
            .app_home
            .workflow
            .as_mut()
            .unwrap()
            .entity
            .as_mut()
            .unwrap();
        if p.loading || p.selecting {
            return;
        }
        let Some(choice) = p.choices.get(index).cloned() else {
            return;
        };
        p.selecting = true;
        p.error = None;
        choice
    };
    dioxus::core::spawn_forever(async move {
        // A channel result is resolved against actual current raw metadata here,
        // not promoted from a directory row or a default-filled Channel model.
        let result = choice.select(state).await;
        let mut s = state.write();
        if !current(&s, id, generation) {
            return;
        }
        let valid = result.as_ref().is_ok_and(|selected| selected.current(&s));
        let d = s.app_home.workflow.as_mut().unwrap();
        let p = d.entity.as_mut().unwrap();
        p.selecting = false;
        if !valid {
            p.error = Some(
                "This result is no longer available. Search again to choose a current result."
                    .into(),
            );
            return;
        }
        let selected = result.unwrap();
        let field = p.field.name.clone();
        let custom = p.custom;
        let value = selected.id().to_owned();
        let applied = if custom {
            d.review.set_custom(&field, value)
        } else {
            d.review.set_context(&field, value)
        };
        if applied.is_ok() {
            d.entity_selected.insert((custom, field), selected);
            d.entity = None;
        }
    });
}

fn close(mut state: Signal<ShellState>, id: u64, generation: u64) {
    let mut s = state.write();
    if !current(&s, id, generation) {
        return;
    }
    let d = s.app_home.workflow.as_mut().unwrap();
    // Revoke the in-flight query as well as removing its visible picker.
    d.entity_query = next();
    d.entity = None;
}

pub(super) fn field(state: Signal<ShellState>, id: u64, field: Input, custom: bool) -> Element {
    let s = state.read();
    let Some(d) = s
        .app_home
        .workflow
        .as_ref()
        .filter(|d| d.id == id && d.origin.current(&s))
    else {
        return rsx! {};
    };
    let title = if field.title.is_empty() {
        field.name.clone()
    } else {
        field.title.clone()
    };
    let description = field.description.clone();
    let label = d
        .entity_selected
        .get(&(custom, field.name.clone()))
        .map(|v| v.label().to_owned())
        .unwrap_or_else(|| match field.kind {
            InputType::User => "Select a person".into(),
            _ => "Select a conversation".into(),
        });
    let disabled = super::busy(&s, id);
    drop(s);
    rsx! {div {class:"app-home-workflow-field",
        label {"{title}"}
        button {r#type:"button",disabled,onclick:move |_|open(state,id,field.clone(),custom),"{label}"}
        if !description.is_empty() {p {"{description}"}}
    }}
}

pub(super) fn panel(state: Signal<ShellState>, id: u64) -> Element {
    let s = state.read();
    let Some(d) = s
        .app_home
        .workflow
        .as_ref()
        .filter(|d| d.id == id && d.origin.current(&s))
    else {
        return rsx! {};
    };
    let Some(p) = &d.entity else {
        return rsx! {};
    };
    let generation = p.generation;
    let query = p.query.clone();
    let loading = p.loading;
    let selecting = p.selecting;
    let disabled = super::busy(&s, id);
    let error = p.error.clone();
    let labels: Vec<_> = p.choices.iter().map(|v| v.label().to_owned()).collect();
    let heading = if p.field.kind == InputType::User {
        "Choose a person"
    } else {
        "Choose a conversation"
    };
    drop(s);
    rsx! {section {class:"app-home-workflow-picker","aria-label":heading,
        label {r#for:"home-workflow-entity-query","{heading}"}
        input {id:"home-workflow-entity-query",r#type:"search",value:query,disabled:disabled||selecting,
            oninput:move |e|search(state,id,e.value())}
        if loading {p {role:"status","Loading results…"}}
        if selecting {p {role:"status","Checking this selection…"}}
        if let Some(error)=error {p {role:"status","{error}"}}
        if !loading && labels.is_empty() {p {"No matching results."}}
        div {class:"app-home-workflow-results",
            for (index,label) in labels.iter().enumerate() {
                button {r#type:"button",disabled:disabled||loading||selecting,
                    onclick:move |_|choose(state,id,generation,index),"{label}"}
            }
        }
        button {r#type:"button",onclick:move |_|close(state,id,generation),"Cancel selection"}
    }}
}
