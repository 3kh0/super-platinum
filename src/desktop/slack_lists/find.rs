//! Visit-owned local Find. No calls, schema mutation, or hierarchy policy.
use super::*;
use super_platinum_core::{slack::models::User, slack_lists::find::Query, state::Workspace};

#[derive(Clone, Default)]
pub(super) struct State {
    pub input: String,
    pub applied: String,
    pub revision: u64,
    serial: u64,
}
pub(crate) fn input(mut state: Signal<ShellState>, text: String, visit: u64) {
    let Some(serial) = begin(&mut state.write(), text, visit) else {
        return;
    };
    spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        if apply(&mut state.write(), visit, serial) {
            dioxus::document::eval(&format!(
                "requestAnimationFrame(()=>{{const d=document.querySelector('.slack-list-dialog');if(d?.dataset.dialogGeneration!=='{visit}')return;for(const s of d.querySelectorAll('.slack-list-scroll,.slack-list-board,.slack-list-layout'))s.scrollTop=0;}});"
            ));
        }
    });
}
fn eligible(shell: &ShellState, visit: u64) -> bool {
    checkbox_visit_current(shell, visit)
        && shell
            .slack_lists
            .view
            .as_ref()
            .is_some_and(|view| view.detail.is_none() && available(view))
}
pub(super) fn available(view: &View) -> bool {
    view.layout.selected.as_ref().is_some_and(|id| {
        view.loaded.as_ref().is_some_and(|loaded| {
            loaded.document.views.iter().any(|v| {
                v["id"].as_str() == Some(id)
                    && matches!(v["type"].as_str(), Some("table" | "board"))
            })
        })
    })
}
fn begin(shell: &mut ShellState, text: String, visit: u64) -> Option<u64> {
    if !eligible(shell, visit) {
        return None;
    }
    let find = &mut shell.slack_lists.view.as_mut()?.layout.find;
    find.input = text;
    find.serial = next_id();
    Some(find.serial)
}
fn apply(shell: &mut ShellState, visit: u64, serial: u64) -> bool {
    if !checkbox_visit_current(shell, visit) {
        return false;
    }
    let team = shell.slack_lists.scope.as_ref().unwrap().team.clone();
    let view = shell.slack_lists.view.as_mut().unwrap();
    if view.layout.find.serial != serial || !available(view) {
        return false;
    }
    if view.layout.find.applied == view.layout.find.input {
        return true;
    }
    view.layout.find.applied = view.layout.find.input.clone();
    view.layout.find.revision = next_id();
    layout::protect_selection(view);
    layout::rebuild(view, shell.core.workspaces.get(&team));
    true
}
pub(super) fn user_name(user: &User) -> String {
    [
        user.extra
            .get("_real_name_normalized_lc")
            .and_then(Value::as_str),
        user.extra
            .get("_display_name_normalized_lc")
            .and_then(Value::as_str),
        user.profile
            .as_ref()
            .and_then(|p| p.display_name.as_deref()),
    ]
    .into_iter()
    .flatten()
    .find(|name| !name.is_empty())
    .unwrap_or("")
    .to_owned()
}
pub(super) fn query(
    loaded: &Loaded,
    workspace: Option<&Workspace>,
    text: &str,
) -> Result<Option<Query>, super_platinum_core::slack_lists::saved_views::UnsupportedView> {
    if text.is_empty() {
        return Ok(None);
    }
    let (users, channels) =
        super_platinum_core::slack_lists::find::name_keys(&loaded.document, &loaded.records)
            .map_err(
                |reason| super_platinum_core::slack_lists::saved_views::UnsupportedView {
                    reason: reason.into(),
                },
            )?;
    let users = users
        .into_iter()
        .map(|id| {
            let name = workspace
                .and_then(|w| w.users.get(&id))
                .map(user_name)
                .unwrap_or_default();
            (id, name)
        })
        .collect();
    let channels = channels
        .into_iter()
        .map(|id| {
            let name = workspace
                .and_then(|w| w.channels.get(&id))
                .and_then(|c| c.name.clone())
                .unwrap_or_default();
            (id, name)
        })
        .collect();
    Ok(Some(Query {
        text: text.into(),
        user_names: Some(users),
        channel_names: Some(channels),
    }))
}
#[cfg(test)]
mod tests;
