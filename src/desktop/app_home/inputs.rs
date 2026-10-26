pub(crate) mod channels;
pub(crate) mod email_fixture;
pub(crate) mod external;
pub(crate) mod focus;
pub(crate) mod focus_fixture;
pub(crate) mod native;
pub(crate) mod number_fixture;
pub(crate) mod static_select;
pub(crate) mod text;
pub(crate) mod users;
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
/// Ordinary Home has no backing parent-error context. Domain validation still
/// governs state/actions; only our native-buffer safety error is shown here.
pub(crate) fn presentation_error(s: &ShellState, key: &InputKey) -> Option<String> {
    if let Some(error) = text::number::error(s, key) {
        return Some(error);
    }
    let form = form(s)?;
    form.native_error_text(key)?;
    form.fields.get(key)?.error.clone()
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
    let date_parents = native::date::parents(s);
    let time_parents = native::time::parents(s);
    let datetime_parents = native::datetime::parents(s);
    let user_parents = users::dispatch::parents(s);
    let channel_parents = channels::dispatch::parents(s);
    let conversation_parents = channels::conversation::dispatch::parents(s);
    let external_parents = external::dispatch::parents(s);
    let number_parents = text::number::parents(s);
    let email_parents = text::email::parents(s);
    let previous_emails = text::email::keys(s);
    let previous_external = external_keys(s)
        .into_iter()
        .map(|key| {
            let multiple = form(s).is_some_and(|f| {
                matches!(
                    f.fields[&key].kind,
                    super_platinum_core::block_actions::form::InputKind::MultiExternalSelect { .. }
                )
            });
            (key, multiple)
        })
        .collect::<Vec<_>>();
    let result = if let Some(form) = s.app_home.forms.get_mut(&key) {
        if form.raw_view() == &raw {
            focus::hydrate(s);
            text::email::hydrate(s, &[]);
            text::number::hydrate(s);
            native::date::hydrate(s);
            native::time::hydrate(s);
            native::datetime::hydrate(s);
            external::hydrate(s);
            users::hydrate(s);
            channels::hydrate(s);
            static_select::hydrate(s);
            return;
        }
        form.reconcile_with_email_parents(
            &raw,
            &date_parents,
            &time_parents,
            &datetime_parents,
            &user_parents,
            &channel_parents,
            &conversation_parents,
            &external_parents,
            &number_parents,
            &email_parents,
        )
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
        focus::hydrate(s);
        bump(s, &key);
        let new_emails = text::email::keys(s)
            .into_iter()
            .filter(|k| !previous_emails.contains(k))
            .collect::<Vec<_>>();
        text::email::hydrate(s, &new_emails);
        text::number::hydrate(s);
        native::date::hydrate(s);
        native::time::hydrate(s);
        native::datetime::hydrate(s);
        let initialized: Vec<_> = external_keys(s)
            .into_iter()
            .filter(|key| {
                let multiple = form(s).is_some_and(|f| matches!(f.fields[key].kind, super_platinum_core::block_actions::form::InputKind::MultiExternalSelect { .. }));
                !previous_external.contains(&(key.clone(),multiple))
            })
            .collect();
        external::hydrate_initialized(s, &initialized);
        users::hydrate(s);
        channels::hydrate(s);
        static_select::hydrate(s);
    }
    if result.is_err() {
        s.app_home.surface.as_mut().unwrap().notice=Some("Home changed while you were editing. Your inputs are retained; review or reset them before another action.".into());
    }
}
fn external_keys(s: &ShellState) -> Vec<InputKey> {
    form(s)
        .map(|f| {
            f.fields
                .iter()
                .filter(|(_, field)| {
                    matches!(
                        field.kind,
                        super_platinum_core::block_actions::form::InputKind::ExternalSelect { .. }
                            | super_platinum_core::block_actions::form::InputKind::MultiExternalSelect { .. }
                    )
                })
                .map(|(key, _)| key.clone())
                .collect()
        })
        .unwrap_or_default()
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
        f.raw_view() == v.raw()
            && f.unsupported.is_empty()
            && native::ready(s, &owner, f)
            && text::ready(s)
            && static_select::ready(s)
            && external::validated(s)
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
    text::number::reset(&mut s, &key);
    text::email::reset(&mut s, &key);
    native::reset(&mut s, &key);
    static_select::reset(&mut s, &key);
    users::dispatch::reset(&mut s, &key);
    channels::dispatch::reset(&mut s, &key);
    external::dispatch::reset(&mut s, &key);
    bump(&mut s, &key);
    s.app_home.forms.insert(key, form);
    s.app_home.surface.as_mut().unwrap().revision = next();
    focus::hydrate(&mut s);
    text::number::hydrate(&mut s);
    let new_emails = text::email::keys(&s);
    text::email::hydrate(&mut s, &new_emails);
    native::date::hydrate(&mut s);
    native::time::hydrate(&mut s);
    native::datetime::hydrate(&mut s);
    static_select::hydrate(&mut s);
    users::hydrate(&mut s);
    channels::hydrate(&mut s);
    let initialized = external_keys(&s);
    external::hydrate_initialized(&mut s, &initialized);
    s.app_home.surface.as_mut().unwrap().notice =
        Some("Inputs reset to the app's current Home view.".into());
}

fn bump(s: &mut ShellState, owner: &FormOwner) {
    s.app_home.input_revisions.insert(owner.clone(), next());
}
pub(crate) fn revision(s: &ShellState) -> Option<u64> {
    s.app_home.input_revisions.get(&owner(s)?).copied()
}
