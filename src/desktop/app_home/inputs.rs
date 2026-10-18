pub(crate) mod native;
// Quiet Home inputs preserve typed local state until an explicit owned action.
use super::*;
use super_platinum_core::block_actions::form::{FormError, InputKey, InputValue, home::HomeForm};
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct FormOwner {
    epoch: u64,
    team: String,
    app: String,
    installed_team: String,
    view: String,
}
fn owner(s: &ShellState) -> Option<FormOwner> {
    let h = s.app_home.surface.as_ref().filter(|h| h.owner.current(s))?;
    let v = h.view.as_ref()?;
    Some(FormOwner {
        epoch: h.owner.ctx.epoch,
        team: h.owner.ctx.session.team_id.clone(),
        app: v.home.app.clone(),
        installed_team: v.home.installed_team.clone(),
        view: v.home.view_id.clone()?,
    })
}
pub(super) fn form(s: &ShellState) -> Option<&HomeForm> {
    s.app_home.forms.get(&owner(s)?)
}
pub(super) fn hydrate(s: &mut ShellState) {
    let Some(key) = owner(s) else { return };
    let raw = s
        .app_home
        .surface
        .as_ref()
        .unwrap()
        .view
        .as_ref()
        .unwrap()
        .raw()
        .clone();
    s.app_home.forms.retain(|k, _| k.epoch == key.epoch);
    let result = if let Some(form) = s.app_home.forms.get_mut(&key) {
        if form.raw_view() == &raw {
            return;
        }
        form.reconcile(&raw)
    } else {
        // Only unchanged pristine inactive entries may be forgotten. Dirty Home
        // values remain owned; a new form refuses rather than evicting that input.
        if s.app_home.forms.len() >= 64 {
            s.app_home.forms.retain(|_, f| f.dirty());
        }
        if s.app_home.forms.len() >= 64 {
            Err(FormError::ConflictingUpdate)
        } else {
            HomeForm::parse(&raw).map(|form| {
                s.app_home.forms.insert(key.clone(), form);
            })
        }
    };
    s.app_home
        .input_revisions
        .retain(|k, _| s.app_home.forms.contains_key(k));
    if result.is_ok() {
        bump(s, &key);
    }
    if result.is_err() {
        s.app_home.surface.as_mut().unwrap().notice=Some("Home changed while you were editing. Your inputs are retained; review or reset them before another action.".into());
    }
}
pub(super) fn snapshot_current(s: &ShellState, key: actions::Key) -> bool {
    s.app_home.surface.as_ref().is_some_and(|h| {
        h.owner.current(s) && h.id == key.visit && h.revision == key.revision && !h.loading
    })
}
pub(super) fn current(s: &ShellState, key: actions::Key) -> bool {
    snapshot_current(s, key)
        && !workflows::locked(s)
        && s.overlay.is_none()
        && s.app_home
            .surface
            .as_ref()
            .is_some_and(|h| h.confirmation.is_none() && h.calendar_confirmation.is_none())
}
pub(crate) fn editable(s: &ShellState) -> bool {
    form(s).is_some_and(|f| f.unsupported.is_empty())
}
pub(crate) fn supported(s: &ShellState) -> bool {
    let Some(h) = s.app_home.surface.as_ref() else {
        return false;
    };
    let Some(v) = h.view.as_ref() else {
        return false;
    };
    form(s).is_some_and(|f| f.raw_view() == v.raw() && f.unsupported.is_empty())
}
pub(crate) fn ready(s: &ShellState) -> bool {
    let Some(h) = s.app_home.surface.as_ref() else {
        return false;
    };
    let Some(v) = h.view.as_ref() else {
        return false;
    };
    let Some(owner) = owner(s) else { return false };
    form(s).is_some_and(|f| {
        f.raw_view() == v.raw() && f.unsupported.is_empty() && native::ready(s, &owner, f)
    })
}
pub(crate) fn set(
    mut state: Signal<ShellState>,
    visit: actions::Key,
    key: InputKey,
    value: InputValue,
) {
    set_local(&mut state.write(), visit, &key, value);
}
pub(super) fn set_local(
    s: &mut ShellState,
    visit: actions::Key,
    key: &InputKey,
    value: InputValue,
) -> bool {
    if !current(s, visit) || !editable(s) {
        return false;
    }
    let Some(owner) = owner(s) else { return false };
    let Some(form) = s.app_home.forms.get_mut(&owner) else {
        return false;
    };
    let changed = form.set(key, value).is_ok();
    if changed {
        bump(s, &owner);
    }
    changed
}
pub(crate) fn error(
    mut state: Signal<ShellState>,
    visit: actions::Key,
    key: InputKey,
    error: String,
    text: String,
) {
    let mut s = state.write();
    if !current(&s, visit) {
        return;
    }
    let Some(owner) = owner(&s) else { return };
    if let Some(form) = s.app_home.forms.get_mut(&owner) {
        let _ = form.set_native_error(&key, text, error);
        bump(&mut s, &owner);
    }
}
#[cfg(test)]
mod tests;
pub(crate) fn reset(mut state: Signal<ShellState>, visit: actions::Key) {
    let mut s = state.write();
    if !current(&s, visit) {
        return;
    }
    let Some(key) = owner(&s) else { return };
    let raw = s
        .app_home
        .surface
        .as_ref()
        .unwrap()
        .view
        .as_ref()
        .unwrap()
        .raw();
    let Ok(form) = HomeForm::parse(raw) else {
        return;
    };
    native::reset(&mut s, &key);
    bump(&mut s, &key);
    s.app_home.forms.insert(key, form);
    s.app_home.surface.as_mut().unwrap().revision = next();
    s.app_home.surface.as_mut().unwrap().notice =
        Some("Inputs reset to the app's current Home view.".into());
}

fn bump(s: &mut ShellState, owner: &FormOwner) {
    s.app_home.input_revisions.insert(owner.clone(), next());
}
pub(crate) fn revision(s: &ShellState) -> Option<u64> {
    s.app_home.input_revisions.get(&owner(s)?).copied()
}
