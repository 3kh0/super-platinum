//! Actual autocomplete.files results; no file ID text entry or guessed search.
use super::*;
use super_platinum_core::block_actions::home::workflow::{CanvasChoice, InputType};

pub(super) struct Picker {
    field: String,
    query: String,
    generation: u64,
    loading: bool,
    choices: Vec<CanvasChoice>,
    error: Option<String>,
}
fn current(s: &ShellState, id: u64, generation: u64) -> bool {
    dialog_current(s, id)
        && !super::busy(s, id)
        && s.app_home.workflow.as_ref().is_some_and(|d| {
            !d.busy
                && d.canvas
                    .as_ref()
                    .is_some_and(|p| p.generation == generation)
        })
}
fn open(mut state: Signal<ShellState>, id: u64, field: String) {
    {
        let mut s = state.write();
        if !dialog_current(&s, id) || super::busy(&s, id) {
            return;
        }
        let d = s.app_home.workflow.as_mut().unwrap();
        if d.busy
            || !d
                .review
                .context()
                .iter()
                .any(|f| f.name == field && f.kind == InputType::Canvas)
        {
            return;
        }
        d.entity = None;
        d.entity_query = next();
        d.canvas = Some(Picker {
            field,
            query: String::new(),
            generation: next(),
            loading: false,
            choices: Vec::new(),
            error: None,
        });
    }
    search(state, id, String::new());
}
fn search(mut state: Signal<ShellState>, id: u64, query: String) {
    let job = {
        let mut s = state.write();
        if !dialog_current(&s, id)
            || super::busy(&s, id)
            || query.len() > 1024
            || query.chars().any(char::is_control)
        {
            return;
        }
        let d = s.app_home.workflow.as_ref().unwrap();
        if d.busy || d.canvas.is_none() || cooling(&s, &d.origin.owner.ctx) {
            return;
        }
        let origin = d.origin.clone();
        let review = d.review.clone();
        let view = s.app_home.surface.as_ref().unwrap().view.as_ref().unwrap();
        let Ok(request) = review.canvas_request(
            &origin.owner.ctx.client,
            &origin.owner.ctx.session,
            view,
            &query,
        ) else {
            return;
        };
        let generation = next();
        let p = s
            .app_home
            .workflow
            .as_mut()
            .unwrap()
            .canvas
            .as_mut()
            .unwrap();
        p.query = query;
        p.generation = generation;
        p.loading = true;
        p.choices.clear();
        p.error = None;
        (origin, review, generation, request)
    };
    dioxus::core::spawn_forever(async move {
        let (origin, review, generation, request) = job;
        let result = io::read(state, &origin, request)
            .await
            .and_then(|raw| review.canvas_choices(&raw));
        let mut s = state.write();
        if let Err(error) = &result {
            failure(&mut s, &origin.owner.ctx, error);
        }
        if !current(&s, id, generation) {
            return;
        }
        let p = s
            .app_home
            .workflow
            .as_mut()
            .unwrap()
            .canvas
            .as_mut()
            .unwrap();
        p.loading = false;
        match result {
            Ok(choices) => p.choices = choices,
            Err(_) => p.error = Some("Canvases could not be loaded. Try the search again when the connection is available.".into()),
        }
    });
}
fn choose(mut state: Signal<ShellState>, id: u64, generation: u64, index: usize) {
    let mut s = state.write();
    if !current(&s, id, generation) {
        return;
    }
    let d = s.app_home.workflow.as_ref().unwrap();
    let p = d.canvas.as_ref().unwrap();
    if p.loading {
        return;
    }
    let Some(choice) = p.choices.get(index).cloned() else {
        return;
    };
    let field = p.field.clone();
    let session = d.origin.owner.ctx.session.clone();
    let view = s.app_home.surface.as_ref().unwrap().view.clone().unwrap();
    let d = s.app_home.workflow.as_mut().unwrap();
    if d.review
        .select_canvas(&field, &choice, &view, &session)
        .is_ok()
    {
        d.canvas_selected.insert(field, choice);
        d.canvas = None;
    }
}
pub(super) fn field(state: Signal<ShellState>, id: u64, name: String, title: String) -> Element {
    let s = state.read();
    let Some(d) = s
        .app_home
        .workflow
        .as_ref()
        .filter(|d| d.id == id && d.origin.current(&s))
    else {
        return rsx! {};
    };
    let label = d
        .canvas_selected
        .get(&name)
        .map(|c| c.label().to_owned())
        .unwrap_or_else(|| "Select a canvas".into());
    let busy = d.busy;
    drop(s);
    rsx! {div {class:"app-home-workflow-field",label {"{title}"}
        button {r#type:"button",disabled:busy,onclick:move |_|open(state,id,name.clone()),"{label}"}
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
    let Some(p) = &d.canvas else {
        return rsx! {};
    };
    let generation = p.generation;
    let query = p.query.clone();
    let loading = p.loading;
    let busy = d.busy;
    let error = p.error.clone();
    let choices: Vec<String> = p.choices.iter().map(|c| c.label().to_owned()).collect();
    drop(s);
    rsx! {section {class:"app-home-workflow-picker","aria-label":"Choose a canvas",
        label {r#for:"home-workflow-canvas-query","Search canvases"}
        input {id:"home-workflow-canvas-query",r#type:"search",value:query,disabled:busy,
            oninput:move |e|search(state,id,e.value())}
        if loading {p {role:"status","Loading canvases…"}}
        if let Some(error)=error {p {role:"status","{error}"}}
        div {class:"app-home-workflow-results",
            for (index,label) in choices.iter().enumerate() {
                button {r#type:"button",disabled:loading||busy,onclick:move |_|choose(state,id,generation,index),"{label}"}
            }
        }
        button {r#type:"button",onclick:move |_| {
            let mut state=state;let mut s=state.write();
            if current(&s,id,generation) {s.app_home.workflow.as_mut().unwrap().canvas=None;}
        },"Cancel canvas selection"}
    }}
}
