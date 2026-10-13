//! Standalone canvas creation. Drafts and in-flight work outlive the dialog.
use crate::{
    icons::{Icon, icon},
    scheduled::Context,
    state::{CanvasTarget, Overlay, ShellState},
};
use dioxus::prelude::*;
use super_platinum_core::slack::{Error, api};

#[derive(Clone, Debug, Default)]
pub struct Draft {
    epoch: u64,
    team: Option<String>,
    generation: u64,
    pub title: String,
    pub markdown: String,
    pub busy: bool,
    pub uncertain: bool,
    pub error: Option<String>,
    pub created: Option<String>,
}
impl Draft {
    pub fn fixture(shell: &ShellState) -> Self {
        Self{title:"Project notes 🦊".into(),markdown:"## Our next release\n\nA **clear** plan, shared with the team.\n\n- [ ] Review the design\n- [x] Draft the proposal\n\n日本語も、そのまま。".into(),..Self::new(shell)}
    }

    fn matches(&self, shell: &ShellState) -> bool {
        self.epoch == shell.core.account_epoch && self.team == shell.core.active_team
    }
    fn new(shell: &ShellState) -> Self {
        Self {
            epoch: shell.core.account_epoch,
            team: shell.core.active_team.clone(),
            generation: shell.shortcut_generation,
            ..Default::default()
        }
    }
}

pub fn open(mut state: Signal<ShellState>) {
    let mut shell = state.write();
    shell.shortcut_generation = shell.shortcut_generation.wrapping_add(1);
    if !shell
        .canvas_create
        .as_ref()
        .is_some_and(|d| d.matches(&shell))
    {
        shell.canvas_create = Some(Draft::new(&shell));
    }
    shell.overlay = Some(Overlay::CanvasCreate);
}
fn change(mut state: Signal<ShellState>, update: impl FnOnce(&mut Draft)) {
    let mut shell = state.write();
    if let Some(draft) = shell
        .canvas_create
        .as_mut()
        .filter(|d| !d.busy && !d.uncertain && d.created.is_none())
    {
        update(draft);
        draft.error = None;
    }
}
fn open_created(mut state: Signal<ShellState>, id: String, title: String) {
    state.write().canvas_create = None;
    crate::canvas::open_target(
        state,
        CanvasTarget {
            id,
            title,
            permalink: None,
        },
    );
}

#[component]
pub fn CreateDialog(mut state: Signal<ShellState>) -> Element {
    let snapshot = state.read();
    let Some(draft) = snapshot
        .canvas_create
        .as_ref()
        .filter(|d| d.matches(&snapshot))
    else {
        return rsx! {};
    };
    let locked = draft.busy || draft.uncertain || draft.created.is_some();
    let title = draft.title.clone();
    let markdown = draft.markdown.clone();
    let created = draft.created.clone();
    let open_title = title.clone();
    rsx! {
        div {class:"scrim",onclick:move |_|state.write().dismiss_overlay()}
        section {id:"shell-overlay-dialog",class:"modal canvas-create-dialog",role:"dialog","aria-label":"New canvas","aria-modal":"true",tabindex:"-1","data-keyboard-dialog":"true","data-dialog-kind":"other","data-dialog-generation":snapshot.shortcut_generation,
            header {h2 {"New canvas"}button {class:"icon-btn","aria-label":"Close new canvas",onclick:move |_|state.write().dismiss_overlay(),{icon(Icon::Close,"icon")}}}
            form {onsubmit:move |event|{event.prevent_default();dioxus::core::spawn_forever(create(state));},
                div {class:"canvas-create-fields",
                    p {class:"canvas-create-note","Private to you. You can share it later in Slack."}
                    label {r#for:"canvas-create-title","Name"}
                    input {id:"canvas-create-title","data-dialog-input":"true",initial_value:"{title}",placeholder:"Untitled canvas",disabled:locked,oninput:move |e|change(state,|d|d.title=e.value())}
                    label {r#for:"canvas-create-body","Content"}
                    textarea {id:"canvas-create-body",initial_value:"{markdown}",placeholder:"Start writing…",disabled:locked,"aria-describedby":"canvas-create-hint",oninput:move |e|change(state,|d|d.markdown=e.value())}
                    p {id:"canvas-create-hint",class:"canvas-create-note","Markdown supported: headings, **bold**, links, lists and checklists."}
                    if let Some(error)=draft.error.as_ref() {p {class:"canvas-create-error",role:"alert","{error}"}}
                    if draft.busy {p {role:"status",class:"canvas-create-note","Creating your canvas… You can close this window while it finishes."}}
                    if created.is_some() {p {role:"status",class:"canvas-create-note","Your canvas is ready."}}
                }
                footer {
                    button {r#type:"button",onclick:move |_|state.write().dismiss_overlay(),"Close"}
                    button {r#type:"button",disabled:draft.busy,onclick:move |_|{let mut shell=state.write();shell.canvas_create=None;shell.dismiss_overlay();},"Discard draft"}
                    span {class:"spacer"}
                    if draft.uncertain {
                        button {r#type:"button",onclick:move |_|check_files(state),"Check Files"}
                    } else if let Some(id)=created {
                        button {r#type:"button",class:"primary",onclick:move |_|open_created(state,id.clone(),open_title.clone()),"Open canvas"}
                    } else {
                        button {r#type:"submit",class:"primary",disabled:draft.busy||draft.title.trim().is_empty(),if draft.busy {"Creating…"} else {"Create canvas"}}
                    }
                }
            }
        }
    }
}

async fn create(mut state: Signal<ShellState>) {
    let (edit, visit) = {
        let shell = state.peek();
        let Some(d) = shell.canvas_create.as_ref().filter(|d| {
            d.matches(&shell)
                && !d.busy
                && !d.uncertain
                && d.created.is_none()
                && !d.title.trim().is_empty()
        }) else {
            return;
        };
        (d.clone(), shell.shortcut_generation)
    };
    let Some(ctx) = Context::capture(&state.peek()) else {
        if let Some(d) = state.write().canvas_create.as_mut() {
            d.error = Some("Reconnect to create your canvas. Your draft is still here.".into());
        }
        return;
    };
    let request = match api::canvas_create(&ctx.client, &ctx.session, &edit.title, &edit.markdown) {
        Ok(r) => r,
        Err(_) => {
            if let Some(d) = state.write().canvas_create.as_mut() {
                d.error =
                    Some("This canvas is too large. Shorten the content and try again.".into());
            }
            return;
        }
    };
    if let Some(d) = state.write().canvas_create.as_mut() {
        d.busy = true;
        d.error = None;
    }
    let result = ctx
        .transport
        .execute(request)
        .await
        .and_then(|response| api::created_canvas_id(&response));
    let mut shell = state.write();
    // Root-scoped request survives dialog dismissal, but cannot open a document
    // over a newer visit or contaminate another account/workspace's draft.
    if !shell
        .canvas_create
        .as_ref()
        .is_some_and(|d| d.matches(&shell) && d.generation == edit.generation)
    {
        return;
    }
    let current = ctx.current(&shell);
    let auto_open = current
        && shell.overlay == Some(Overlay::CanvasCreate)
        && shell.shortcut_generation == visit;
    let d = shell.canvas_create.as_mut().unwrap();
    d.busy = false;
    match result {
        Ok(id) => {
            d.created = Some(id.clone());
            shell.show_toast("Canvas created");
            if auto_open {
                drop(shell);
                open_created(state, id, edit.title.trim().into());
            }
        }
        Err(error) => {
            // Link loss can happen after Slack accepted a mutation.
            d.uncertain = creation_uncertain(&error);
            d.error=Some(if d.uncertain {
                "Slack may have created this canvas. Check Files before creating another. Your draft is retained."
            } else if error.is_offline() {
                "Reconnect to create your canvas. Your draft is still here."
            } else {
                "Slack could not create this canvas. Your draft is still here. Try again."
            }.into());
        }
    }
}

fn creation_uncertain(error: &Error) -> bool {
    match error {
        Error::Api(code) => matches!(
            code.as_str(),
            "canvas_created_id_missing" | "internal_error" | "fatal_error" | "service_unavailable"
        ),
        Error::RateLimited { .. } => false,
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn creation_retains_draft_but_never_crosses_account_or_workspace() {
        let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
        let draft = Draft::fixture(&shell);
        assert!(draft.matches(&shell));
        shell.shortcut_generation += 1;
        assert!(draft.matches(&shell));
        shell.core.account_epoch += 1;
        assert!(!draft.matches(&shell));
        shell.core.account_epoch -= 1;
        shell.core.active_team = Some("OTHER".into());
        assert!(!draft.matches(&shell));
    }
    #[test]
    fn lost_creation_responses_cannot_offer_a_duplicate_retry() {
        assert!(creation_uncertain(&Error::Offline("socket lost".into())));
        assert!(creation_uncertain(&Error::Transport(
            "response lost".into()
        )));
        assert!(creation_uncertain(&Error::Api(
            "canvas_created_id_missing".into()
        )));
        assert!(creation_uncertain(&Error::Api("internal_error".into())));
        assert!(creation_uncertain(&Error::Api("fatal_error".into())));
        assert!(!creation_uncertain(&Error::Api("not_allowed".into())));
        assert!(!creation_uncertain(&Error::RateLimited {
            retry_after_secs: Some(2)
        }));
    }
}

fn check_files(mut state: Signal<ShellState>) {
    {
        let mut shell = state.write();
        shell.dismiss_overlay();
        if let Some(team) = shell.core.active_team.clone() {
            let epoch = shell.core.account_epoch;
            shell.core.files.ensure_scope(epoch, &team);
            shell.core.files.begin(
                super_platinum_core::files::FilesQuery {
                    kind: super_platinum_core::files::FileKind::Canvases,
                    owner: super_platinum_core::files::FileOwner::Yours,
                    sort: super_platinum_core::files::FileSort::Newest,
                    ..Default::default()
                },
                1,
            );
        }
    }
    dioxus::core::spawn_forever(crate::bootstrap::load_main_view(
        state,
        crate::state::MainView::Files,
    ));
}
