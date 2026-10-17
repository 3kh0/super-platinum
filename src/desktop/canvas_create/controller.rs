use super::{
    Draft, Overlay, Root, ShellState,
    adapter::{self, Phase, Source, Storage},
    domain,
    editor::Editor,
    fixture, native, review,
};
use dioxus::prelude::*;
use std::sync::Arc;
mod actions;
mod delivery;
pub(super) use actions::{archive, cancel_reserved, check_files, open_created, retry};
pub(super) use delivery::{Action, capture};

pub(super) fn owner(shell: &ShellState) -> Option<domain::Owner> {
    if crate::fixture::is_fixture() {
        return fixture::owner(shell);
    }
    let account = shell.core.active_account.as_deref()?;
    let team = shell.core.active_team.as_deref()?;
    let workspace = shell.core.workspaces.get(team)?;
    let user = &shell.self_account.user_id;
    if workspace.team_id != team || workspace.self_user_id != *user {
        return None;
    }
    domain::Owner::new(account, team, user).ok()
}
pub(crate) fn carry_pending(previous: &mut ShellState, loaded: &mut ShellState) {
    // No old UI visit/credentials/auto-navigation grant crosses replacement.
    loaded.canvas_creation = previous.canvas_creation.clone();
}
/// Cooperative Close fence. Typed user routes retain their single destination in
/// departure::request; plain dismissal has no destination or arbitrary closure.
pub(crate) fn departure(shell: &mut ShellState) -> bool {
    if shell.overlay != Some(Overlay::CanvasCreate) {
        return false;
    }
    let Some(d) = shell.canvas_create.as_mut() else {
        return false;
    };
    if d.fixture {
        shell.overlay = None;
        return false;
    }
    if d.editor.is_none()
        || d.busy
            && d.editor.as_ref().is_some_and(|e| {
                let s = e.snapshot();
                s.saved && s.capture.is_none()
            })
    {
        shell.canvas_create = None;
        shell.overlay = None;
        return false;
    }
    if !d.busy {
        if let Some(review) = d.review.take() {
            if let Some(editor) = &d.editor {
                editor.release_capture(&review.token);
            }
            native::release(&review.mount, &review.token);
        }
    }
    d.close_requested = true;
    true
}

pub(crate) fn open(mut state: Signal<ShellState>) {
    let (owner, root, visit) = {
        let mut shell = state.write();
        if shell.overlay == Some(Overlay::CanvasCreate) {
            return;
        }
        if crate::fixture::is_fixture() && !fixture::requested() {
            let mut draft = shell
                .canvas_create
                .take()
                .filter(|d| d.fixture)
                .unwrap_or_else(|| Draft::new(None));
            draft.fixture = true;
            draft.loading = false;
            draft.visit = uuid::Uuid::new_v4().to_string();
            draft.native_visit = None;
            draft.native_failed = false;
            shell.shortcut_generation = shell.shortcut_generation.wrapping_add(1);
            shell.canvas_create = Some(draft);
            shell.overlay = Some(Overlay::CanvasCreate);
            return;
        }
        let Some(owner) = owner(&shell) else {
            shell.show_toast("Sign in to open your saved canvas draft.");
            return;
        };
        let d = Draft::new(Some(owner.clone()));
        let visit = d.visit.clone();
        shell.shortcut_generation = shell.shortcut_generation.wrapping_add(1);
        shell.canvas_create = Some(d);
        shell.overlay = Some(Overlay::CanvasCreate);
        (owner, shell.canvas_creation.clone(), visit)
    };
    adapter::retain(async move {
        let result = async {
            if let Some(editor) = root.find(&owner) {
                let base = editor.snapshot().base;
                let read_owner = owner.clone();
                let attempt = editor
                    .storage
                    .run(move |store| {
                        base.attempt_id()
                            .map(|id| store.attempt(&read_owner, id))
                            .transpose()
                            .map(Option::flatten)
                    })
                    .await?;
                return Ok((editor, attempt));
            }
            let storage = if crate::fixture::is_fixture() {
                fixture::storage(&state.peek(), &owner).ok_or(domain::Fault::Invalid)?
            } else {
                Storage::application(owner.clone())?
            };
            let load_owner = owner.clone();
            let (base, attempt) = storage
                .run(move |store| {
                    let base = match store.active(&load_owner)? {
                        Some(base) => base,
                        None => store.create(
                            &load_owner,
                            &uuid::Uuid::new_v4().to_string(),
                            domain::Content::new(String::new(), String::new())?,
                        )?,
                    };
                    let attempt = base
                        .attempt_id()
                        .map(|id| store.attempt(&load_owner, id))
                        .transpose()?
                        .flatten();
                    Ok((base, attempt))
                })
                .await?;
            let editable = attempt.as_ref().is_none_or(|a| {
                matches!(
                    a.delivery(),
                    domain::Delivery::CanceledBeforeAdmission
                        | domain::Delivery::RateLimited { .. }
                )
            });
            let editor = root.install(storage, base, editable)?;
            Ok::<_, domain::Fault>((editor, attempt))
        }
        .await;
        let mut shell = state.write();
        if self::owner(&shell).as_ref() != Some(&owner) {
            return;
        }
        let Some(d) = shell.canvas_create.as_mut().filter(|d| d.visit == visit) else {
            return;
        };
        match result {
            Ok((editor, attempt)) => {
                let s = editor.snapshot();
                d.title = s.content.title().into();
                d.markdown = s.content.markdown().into();
                d.saved = s.saved;
                d.editor = Some(editor);
                d.loading = false;
                apply_attempt(d, attempt);
                if s.failed {
                    d.error = Some(
                        "Your text is retained here. Retry its local save before leaving.".into(),
                    );
                }
            }
            Err(_) => {
                d.error = Some(
                    "Could not load this account's canvas draft. No blank draft replaced it."
                        .into(),
                );
                d.loading = false;
            }
        }
    });
}

fn apply_attempt(d: &mut Draft, attempt: Option<domain::Attempt>) {
    d.uncertain = attempt.as_ref().is_some_and(|a| {
        matches!(
            a.delivery(),
            domain::Delivery::Started | domain::Delivery::Uncertain
        )
    });
    d.created = attempt.as_ref().and_then(|a| match a.delivery() {
        domain::Delivery::Created { file_id } => Some(file_id.clone()),
        _ => None,
    });
    d.error = attempt.as_ref().and_then(|a| match a.delivery() {
        domain::Delivery::Started | domain::Delivery::Uncertain => Some("Slack may have created this canvas. Its exact draft and delivery receipt are retained. Checking Files does not authorize a retry.".into()),
        domain::Delivery::Reserved => Some("A reserved creation needs local recovery before another request.".into()),
        domain::Delivery::RateLimited { .. } => Some("Slack limited this request. Your draft is retained; a new explicit Create must wait for the recorded cooldown.".into()),
        _ => None,
    });
    d.attempt = attempt;
}

pub(super) fn change(
    mut state: Signal<ShellState>,
    expected: &str,
    title: Option<String>,
    markdown: Option<String>,
) {
    let mut shell = state.write();
    let Some(d) = shell
        .canvas_create
        .as_mut()
        .filter(|d| d.visit == expected && !d.uncertain && d.created.is_none())
    else {
        return;
    };
    if d.fixture && crate::fixture::is_fixture() {
        if let Some(title) = title {
            d.title = title;
        }
        if let Some(markdown) = markdown {
            d.markdown = markdown;
        }
        d.error = None;
        return;
    }
    let Some(editor) = d.editor.clone() else {
        return;
    };
    let content = domain::Content::new(
        title.unwrap_or_else(|| d.title.clone()),
        markdown.unwrap_or_else(|| d.markdown.clone()),
    );
    match content.and_then(|content| {
        editor.accept(content.clone())?;
        Ok(content)
    }) {
        Ok(content) => {
            d.title = content.title().into();
            d.markdown = content.markdown().into();
            d.saved = editor.snapshot().saved;
            d.error = None;
        }
        Err(_) => {
            d.error = Some(
                "This edit could not be saved. Keep the editor open and shorten or retry the text."
                    .into(),
            );
        }
    }
}

#[component]
pub(super) fn Observe(mut state: Signal<ShellState>, visit: String) -> Element {
    use_future(move || {
        let visit = visit.clone();
        async move {
            let Some(editor) = state
                .peek()
                .canvas_create
                .as_ref()
                .filter(|d| d.visit == visit)
                .and_then(|d| d.editor.clone())
            else {
                return;
            };
            let mut changed = editor.changes();
            loop {
                let s = editor.snapshot();
                {
                    let mut shell = state.write();
                    let Some(d) = shell.canvas_create.as_mut().filter(|d| d.visit == visit) else {
                        return;
                    };
                    d.saved = s.saved;
                    if s.failed {
                        d.error = Some(
                            "Your text is retained here. Retry its local save before leaving."
                                .into(),
                        );
                    }
                }
                if changed.changed().await.is_err() {
                    return;
                }
            }
        }
    });
    rsx! {}
}

/// Return an owned result before projecting an error into this same Signal.
/// A WriteRef in a match scrutinee would otherwise live through its error arm.
pub(super) fn take_saved_departure(
    mut state: Signal<ShellState>,
    visit: &str,
    editor: &Editor,
    saved: &domain::Draft,
) -> Result<Option<super::departure::Continuation>, ()> {
    let result = {
        let mut shell = state.write();
        super::departure::take_saved(&mut shell, visit, editor, saved)
    };
    if result.is_err() {
        update_error(
            state,
            visit,
            "Your latest input is still retained. Finish saving before navigating.",
        );
    }
    result
}

pub(super) fn close_exact(mut state: Signal<ShellState>, visit: &str) {
    let mut shell = state.write();
    if shell.overlay == Some(Overlay::CanvasCreate)
        && shell
            .canvas_create
            .as_ref()
            .is_some_and(|d| d.visit == visit)
    {
        shell.canvas_create = None;
        shell.overlay = None;
    }
}
pub(super) fn update_error(mut state: Signal<ShellState>, visit: &str, message: &str) {
    if let Some(d) = state
        .write()
        .canvas_create
        .as_mut()
        .filter(|d| d.visit == visit)
    {
        d.busy = false;
        d.close_requested = false;
        d.pending_departure = None;
        d.error = Some(message.into());
    }
}
