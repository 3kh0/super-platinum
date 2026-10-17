//! Explicit, bounded owner inventory. Rendering performs no store access.
use super::{Draft, ShellState, adapter, domain, editor::Editor};
use dioxus::prelude::*;
use std::sync::Arc;
mod history;
use history::{dispose_evidence, open_result, retirable, retire_history};

#[derive(Clone)]
pub(super) struct View {
    pub inventory: Arc<domain::RetirementReview>,
    pub selected: Option<domain::LocalVersionChoice>,
    pub writer: String,
    pub busy: bool,
    pub error: Option<&'static str>,
    pub token: String,
    pub mount: String,
    pub serial: u64,
    pub expanded: Option<String>,
}
pub(super) async fn load(
    mut state: Signal<ShellState>,
    visit: &str,
    editor: &Arc<Editor>,
    mount: &str,
    token: &str,
) {
    let owner = editor.storage.owner().clone();
    let result = editor
        .storage
        .run(move |store| store.retirement_review(&owner))
        .await;
    let current = {
        let shell = state.peek();
        super::controller::owner(&shell).as_ref() == Some(editor.storage.owner())
            && shell.overlay == Some(super::Overlay::CanvasCreate)
            && shell
                .canvas_create
                .as_ref()
                .is_some_and(|d| d.visit == visit)
    };
    if !current {
        editor.release_capture(token);
        super::native::release(mount, token);
        state.peek().canvas_creation.release(editor);
        return;
    }
    if let Some(d) = state
        .write()
        .canvas_create
        .as_mut()
        .filter(|d| d.visit == visit)
    {
        d.busy = false;
        match result {
            Ok(inventory) => {
                d.review = Some(View {
                    inventory: Arc::new(inventory),
                    selected: None,
                    writer: uuid::Uuid::new_v4().to_string(),
                    busy: false,
                    error: None,
                    token: token.into(),
                    mount: mount.into(),
                    serial: editor.snapshot().serial,
                    expanded: None,
                })
            }
            Err(_) => {
                editor.release_capture(token);
                super::native::release(mount, token);
                d.error = Some(
                    "Could not read all retained versions. Your input is still retained.".into(),
                );
            }
        }
    }
}
fn choose(
    mut state: Signal<ShellState>,
    visit: &str,
    writer: &str,
    choice: domain::LocalVersionChoice,
) {
    let mut shell = state.write();
    if let Some(v) = shell
        .canvas_create
        .as_mut()
        .filter(|d| d.visit == visit)
        .and_then(|d| d.review.as_mut())
        .filter(|v| v.writer == writer && !v.busy)
    {
        v.selected = Some(choice);
        v.error = None;
    }
}
fn expand(mut state: Signal<ShellState>, visit: &str, writer: &str, id: &str) {
    if let Some(v) = state
        .write()
        .canvas_create
        .as_mut()
        .filter(|d| d.visit == visit)
        .and_then(|d| d.review.as_mut())
        .filter(|v| v.writer == writer && !v.busy)
    {
        v.expanded = Some(id.to_owned());
    }
}
fn cancel(mut state: Signal<ShellState>, visit: &str, writer: &str) {
    let mut shell = state.write();
    let Some(d) = shell.canvas_create.as_mut().filter(|d| d.visit == visit) else {
        return;
    };
    if !d
        .review
        .as_ref()
        .is_some_and(|v| v.writer == writer && !v.busy)
    {
        return;
    }
    let view = d.review.take().unwrap();
    if let Some(editor) = &d.editor {
        editor.release_capture(&view.token);
    }
    super::native::release(&view.mount, &view.token);
}
fn apply(mut state: Signal<ShellState>, visit: &str, writer: &str) {
    let (editor, view, root, visit) = {
        let mut shell = state.write();
        let root = shell.canvas_creation.clone();
        let Some(d) = shell.canvas_create.as_mut().filter(|d| d.visit == visit) else {
            return;
        };
        let Some(editor) = d.editor.clone() else {
            return;
        };
        let Some(v) = d
            .review
            .as_mut()
            .filter(|v| v.writer == writer && !v.busy && v.selected.is_some())
        else {
            return;
        };
        let snapshot = editor.snapshot();
        if snapshot.serial != v.serial
            || snapshot.capture.as_deref() != Some(&v.token)
            || !snapshot.sealed
        {
            return;
        }
        v.busy = true;
        d.busy = true;
        (editor, v.clone(), root, d.visit.clone())
    };
    adapter::retain(async move {
        let choice = view.selected.clone().unwrap();
        let expected = view.clone();
        let result = editor
            .storage
            .run(move |store| {
                store.resolve_local_review(&expected.inventory, &choice, &expected.writer)
            })
            .await;
        match result {
            Ok(_) => {
                // Every held save was terminal before this inventory was read.
                // The chosen canonical and all disposition fences are committed.
                editor.release_capture(&view.token);
                root.release(&editor);
                let current = {
                    let shell = state.peek();
                    super::controller::owner(&shell).as_ref() == Some(editor.storage.owner())
                        && shell.overlay == Some(super::Overlay::CanvasCreate)
                        && shell
                            .canvas_create
                            .as_ref()
                            .is_some_and(|d| d.visit == visit)
                };
                if current {
                    super::controller::close_exact(state, &visit);
                    super::controller::open(state);
                }
            }
            Err(_) => {
                if let Some(d) = state
                    .write()
                    .canvas_create
                    .as_mut()
                    .filter(|d| d.visit == visit)
                {
                    d.busy = false;
                    let Some(v) = d.review.as_mut() else {
                        return;
                    };
                    v.busy = false;
                    v.error = Some(
                        "Could not confirm this choice. Retry this exact choice or reopen the saved versions. No request was sent.",
                    );
                }
            }
        }
    });
}
pub(super) fn render(state: Signal<ShellState>, draft: &Draft) -> Element {
    let Some(view) = draft.review.as_ref() else {
        return rsx! {};
    };
    let visit = draft.visit.clone();
    let writer = view.writer.clone();
    let canonical = view.inventory.drafts().iter().find(|d| !d.archived());
    let active_uncertain = canonical.and_then(|d| d.attempt_id()).is_some_and(|id| {
        view.inventory.attempts().iter().any(|a| {
            a.id() == id
                && !matches!(
                    a.delivery(),
                    domain::Delivery::CanceledBeforeAdmission
                        | domain::Delivery::RateLimited { .. }
                )
        })
    });
    let choose_visit = visit.clone();
    let choose_writer = writer.clone();
    let apply_visit = visit.clone();
    let apply_writer = writer.clone();
    let read_visit = visit.clone();
    let read_writer = writer.clone();
    let cancel_visit = visit.clone();
    let cancel_writer = writer.clone();
    rsx! {
        section {class:"canvas-create-review","aria-label":"Retained canvas versions",
            h3 {"Retained versions"}
            p {"Choose explicitly. Applying keeps one local version and retires the displayed text conflicts. Delivery receipts are not discarded and nothing is sent."}
            if let Some(canonical)=canonical {
                label {input {r#type:"radio",name:"canvas-local-choice",checked:view.selected==Some(domain::LocalVersionChoice::Canonical),disabled:view.busy||active_uncertain,
                    onchange:move |_|choose(state,&choose_visit,&choose_writer,domain::LocalVersionChoice::Canonical)} "Current saved draft: {canonical.content().title()}"}
                button {r#type:"button",onclick:move |_|expand(state,&read_visit,&read_writer,"canonical"),"Read current saved content"}
                if view.expanded.as_deref()==Some("canonical"){textarea {readonly:true,value:"{canonical.content().markdown()}","aria-label":"Current saved canvas content"}}
            }
            for conflict in view.inventory.conflicts() {
                {let id=conflict.id().to_owned();let read_id=id.clone();let read_visit=visit.clone();let read_writer=writer.clone();let visit=visit.clone();let writer=writer.clone();rsx!{
                    div {key:"{id}",
                        label {input {r#type:"radio",name:"canvas-local-choice",checked:view.selected==Some(domain::LocalVersionChoice::Conflict(id.clone())),disabled:view.busy||active_uncertain,
                            onchange:move |_|choose(state,&visit,&writer,domain::LocalVersionChoice::Conflict(id.clone()))} "Retained local version: {conflict.content().title()}"}
                        button {r#type:"button",onclick:move |_|expand(state,&read_visit,&read_writer,&read_id),"Read retained content"}
                        if view.expanded.as_deref()==Some(conflict.id()){textarea {readonly:true,value:"{conflict.content().markdown()}","aria-label":"Retained local canvas content"}}
                    }
                }}
            }
            for attempt in view.inventory.attempts() {
                div {key:"{attempt.id()}",
                    p {"Creation receipt for {attempt.content().title()}"}
                    match attempt.delivery(){
                        domain::Delivery::Created{file_id}=>{let visit=visit.clone();let writer=writer.clone();let id=file_id.clone();let title=attempt.content().title().to_owned();rsx!{p {"Created canvas: {file_id}"}button {r#type:"button",disabled:view.busy,onclick:move |_|open_result(state,&visit,&writer,&id,&title),"Open created canvas"}}},
                        domain::Delivery::Started|domain::Delivery::Uncertain=>rsx!{p {"Outcome unknown. This receipt cannot be retried or removed by choosing text."}},
                        domain::Delivery::Reserved=>rsx!{p {"Reserved; no delivery has been admitted."}},
                        domain::Delivery::CanceledBeforeAdmission=>rsx!{p {"Canceled before delivery admission."}},
                        domain::Delivery::RateLimited{retry_at}=>rsx!{p {"Rate limited until {retry_at}."}},
                    }
                }
            }
            for draft in view.inventory.drafts(){
                {let visit=visit.clone();let writer=writer.clone();let id=draft.id().to_owned();let allowed=retirable(&view.inventory,&id);rsx!{
                    if allowed {button {r#type:"button",disabled:view.busy,onclick:move |_|retire_history(state,&visit,&writer,&id),"Retire displayed local history for {draft.content().title()}"}}
                }}
            }
            for evidence in view.inventory.late_results(){
                {let visit=visit.clone();let writer=writer.clone();let id=evidence.id().to_owned();rsx!{div {key:"{evidence.id()}",p {"Additional delivery evidence for {evidence.original().content().title()}"}
                match evidence.incoming(){domain::Delivery::Created{file_id}=>rsx!{p {"Additional returned canvas: {file_id}"}},_=>rsx!{p {"A different delivery outcome was retained. No automatic retry is allowed."}}}
                button {r#type:"button",disabled:view.busy,onclick:move |_|dispose_evidence(state,&visit,&writer,&id),"I reviewed this additional result; retire its local evidence"}
            }}}
            }
            if let Some(error)=view.error {p {role:"alert","{error}"}}
            button {r#type:"button",disabled:view.busy,onclick:move |_|cancel(state,&cancel_visit,&cancel_writer),"Return to editor"}
            button {r#type:"button",disabled:view.busy||view.selected.is_none()||view.inventory.conflicts().is_empty()||active_uncertain,
                onclick:move |_|apply(state,&apply_visit,&apply_writer),"Keep selected version"}
        }
    }
}
