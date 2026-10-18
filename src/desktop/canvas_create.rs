//! Durable standalone Canvas creation. Content custody survives the dialog.
use crate::{
    icons::{Icon, icon},
    state::{Overlay, ShellState},
};
use dioxus::prelude::*;
use std::sync::Arc;
use super_platinum_core::canvas::creation as domain;

mod adapter;
mod controller;
pub(crate) mod departure;
mod editor;
pub(crate) mod fixture;
#[cfg(test)]
mod integration_tests;
mod native;
mod review;
pub(crate) use controller::{carry_pending, departure, open};
pub(crate) use editor::Root;

#[derive(Clone)]
pub struct Draft {
    owner: Option<domain::Owner>,
    visit: String,
    native_visit: Option<String>,
    native_failed: bool,
    editor: Option<Arc<editor::Editor>>,
    attempt: Option<domain::Attempt>,
    pub title: String,
    pub markdown: String,
    pub busy: bool,
    pub uncertain: bool,
    pub error: Option<String>,
    pub created: Option<String>,
    loading: bool,
    close_requested: bool,
    pending_departure: Option<departure::Pending>,
    saved: bool,
    fixture: bool,
    review: Option<review::View>,
}
impl Draft {
    fn new(owner: Option<domain::Owner>) -> Self {
        Self {
            owner,
            visit: uuid::Uuid::new_v4().to_string(),
            native_visit: None,
            native_failed: false,
            editor: None,
            attempt: None,
            title: String::new(),
            markdown: String::new(),
            busy: false,
            uncertain: false,
            error: None,
            created: None,
            loading: true,
            close_requested: false,
            pending_departure: None,
            saved: false,
            fixture: false,
            review: None,
        }
    }
    pub fn fixture(_shell: &ShellState) -> Self {
        Self { title: "Project notes \u{1f98a}".into(), markdown: "## Our next release\n\nA **clear** plan, shared with the team.\n\n- [ ] Review the design\n- [x] Draft the proposal\n\n\u{65e5}\u{672c}\u{8a9e}\u{3082}\u{3001}\u{305d}\u{306e}\u{307e}\u{307e}\u{3002}".into(),
            fixture: true, loading: false, ..Self::new(None) }
    }
    fn current(&self, shell: &ShellState) -> bool {
        self.fixture && crate::fixture::is_fixture()
            || self
                .owner
                .as_ref()
                .is_some_and(|owner| controller::owner(shell).as_ref() == Some(owner))
    }
    fn editable(&self) -> bool {
        (self.editor.is_some() || self.fixture)
            && !self.loading
            && !self.busy
            && self.review.is_none()
            && !self.uncertain
            && self.created.is_none()
            && !matches!(
                self.attempt.as_ref().map(|a| a.delivery()),
                Some(domain::Delivery::Reserved)
            )
    }
}

#[component]
pub fn CreateDialog(mut state: Signal<ShellState>) -> Element {
    let mount = use_hook(|| uuid::Uuid::new_v4().to_string());
    let retire_mount = mount.clone();
    use_drop(move || native::retire(&retire_mount));
    let install_mount = mount.clone();
    use_effect(move || {
        let shell = state.read();
        let Some(d) = shell.canvas_create.as_ref().filter(|d| d.current(&shell)) else {
            return;
        };
        if d.loading || d.native_failed || d.native_visit.as_deref() == Some(install_mount.as_str())
        {
            return;
        }
        let visit = d.visit.clone();
        let mount = install_mount.clone();
        let editable = d.editable();
        adapter::retain(async move {
            let installed = native::install(&mount, editable).await;
            let mut shell = state.write();
            if let Some(d) = shell.canvas_create.as_mut().filter(|d| d.visit == visit) {
                if installed {
                    d.native_visit = Some(mount);
                } else {
                    d.native_failed = true;
                    d.error = Some("The editor could not be attached. Retry local save to attach it; your saved draft remains retained.".into());
                }
            }
        });
    });
    use_effect(move || {
        let shell = state.read();
        let close = shell.canvas_create.as_ref().is_some_and(|d| {
            d.current(&shell) && d.close_requested && !d.busy && d.native_visit.is_some()
        });
        drop(shell);
        if close {
            let visit = state.peek().canvas_create.as_ref().unwrap().visit.clone();
            controller::capture(state, &visit, controller::Action::Close);
        }
    });
    let snapshot = state.read();
    let Some(draft) = snapshot
        .canvas_create
        .as_ref()
        .filter(|d| d.current(&snapshot))
    else {
        return rsx! {};
    };
    let title = draft.title.clone();
    let markdown = draft.markdown.clone();
    let locked = !draft.editable();
    let fixture = draft.fixture;
    let title_visit = draft.visit.clone();
    let body_visit = draft.visit.clone();
    let submit_visit = draft.visit.clone();
    let discard_visit = draft.visit.clone();
    let retry_visit = draft.visit.clone();
    let review_visit = draft.visit.clone();
    let check_visit = draft.visit.clone();
    let abandon_visit = draft.visit.clone();
    let open_visit = draft.visit.clone();
    let new_visit = draft.visit.clone();
    let reserved_visit = draft.visit.clone();
    let field_locked = draft.loading || draft.uncertain || draft.created.is_some();
    let observe_key = draft
        .editor
        .as_ref()
        .map(|editor| format!("{}-{:p}", draft.visit, Arc::as_ptr(editor)))
        .unwrap_or_default();
    rsx! {
        div { class:"scrim", onclick:move |_|state.write().dismiss_overlay() }
        section { id:"shell-overlay-dialog", class:"modal canvas-create-dialog", role:"dialog", "aria-label":"New canvas", "aria-modal":"true", tabindex:"-1", "data-keyboard-dialog":"true", "data-dialog-kind":"other", "data-dialog-generation":snapshot.shortcut_generation,
            header { h2 {"New canvas"} button {class:"icon-btn", "aria-label":"Close new canvas", onclick:move |_|state.write().dismiss_overlay(), {icon(Icon::Close,"icon")}} }
            if draft.loading { p {role:"status", "Loading your saved canvas draft\u{2026}"} }
            else {
                if draft.editor.is_some() { controller::Observe { key:"{observe_key}", state, visit:draft.visit.clone() } }
                form { onsubmit:move |event| { event.prevent_default(); controller::capture(state,&submit_visit,controller::Action::Create); },
                    div {class:"canvas-create-fields",
                        p {class:"canvas-create-note", "Private to you. You can share it later in Slack."}
                        label {r#for:"canvas-create-title", "Name"}
                        input {id:"canvas-create-title", "data-canvas-visit":"{mount}", "data-dialog-input":"true", initial_value:"{title}", placeholder:"Untitled canvas", readonly:true, disabled:field_locked, oninput:move |e|controller::change(state,&title_visit,Some(e.value()),None)}
                        label {r#for:"canvas-create-body", "Content"}
                        textarea {id:"canvas-create-body", "data-canvas-visit":"{mount}", initial_value:"{markdown}", placeholder:"Start writing\u{2026}", readonly:true, disabled:field_locked, "aria-describedby":"canvas-create-hint", oninput:move |e|controller::change(state,&body_visit,None,Some(e.value()))}
                        p {id:"canvas-create-hint", class:"canvas-create-note", "Markdown supported: headings, **bold**, links, lists and checklists."}
                        {review::render(state,draft)}
                    }
                    if draft.error.is_some() || draft.busy || draft.created.is_some() || (draft.saved && !fixture) {
                        div {class:"canvas-create-feedback",
                            if let Some(error)=draft.error.as_ref() { p {class:"canvas-create-error", role:"alert", "{error}"} }
                            if draft.busy { p {role:"status", class:"canvas-create-note", "Retaining your canvas\u{2026}"} }
                            else if draft.created.is_some() { p {role:"status", class:"canvas-create-note", "Your canvas is ready."} }
                            else if draft.saved && !fixture { p {role:"status",class:"canvas-create-note", "Draft saved on this device."} }
                        }
                    }
                    footer {
                        button {r#type:"button", onclick:move |_|state.write().dismiss_overlay(), "Close"}
                        if draft.error.is_some() && !fixture { button {r#type:"button",disabled:draft.busy,onclick:move |_|controller::retry(state,&retry_visit),"Retry local save"} }
                        if !fixture {button {r#type:"button",disabled:draft.busy||draft.review.is_some(),onclick:move |_|controller::capture(state,&review_visit,controller::Action::Review),"Review retained versions"}}
                        span {class:"spacer"}
                        if draft.uncertain {
                            button {r#type:"button",onclick:move |_|controller::check_files(state,&check_visit),"Check Files"}
                            button {r#type:"button",disabled:draft.busy||fixture,onclick:move |_|controller::archive(state,&abandon_visit),"Keep unresolved draft and start another"}
                        } else if draft.created.is_some() {
                            button {r#type:"button",class:"primary",onclick:move |_|controller::open_created(state,&open_visit),"Open canvas"}
                            button {r#type:"button",disabled:draft.busy||fixture,onclick:move |_|controller::archive(state,&new_visit),"Start a new canvas"}
                        } else if matches!(draft.attempt.as_ref().map(|a|a.delivery()),Some(domain::Delivery::Reserved)) {
                            button {r#type:"button",disabled:draft.busy||fixture,onclick:move |_|controller::cancel_reserved(state,&reserved_visit),"Cancel reserved creation"}
                        } else {
                            button {r#type:"button",disabled:draft.busy,onclick:move |_|controller::capture(state,&discard_visit,controller::Action::Discard),"Discard draft"}
                            button {r#type:"submit",class:"primary",disabled:locked||title.trim().is_empty(),"Create canvas"}
                        }
                    }
                }
            }
        }
    }
}
