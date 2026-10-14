//! The checkbox is an independent one-field setting, never part of bulk clear.
use super::*;
use super_platinum_core::state::CONFIRM_CLEAR_UNREADS;
pub(super) fn cooling(s: &ShellState) -> bool {
    crate::channel_mute::prefs_retry_at(s) > super_platinum_core::state::now_secs()
}
fn remember(s: &mut ShellState, c: &Context, e: &Error) {
    crate::channel_mute::remember_owned_prefs_cooldown(s, &c.rate_owner, e);
}
fn exact_false(p: &ClearUnreadsPreference) -> bool {
    p.loaded() && p.raw().get(CONFIRM_CLEAR_UNREADS) == Some(&serde_json::Value::Bool(false))
}
pub(super) fn begin(
    s: &mut ShellState,
    c: Arc<Context>,
    baseline: ClearUnreadsPreference,
) -> Option<PrefWork> {
    if !c.current(s)
        || cooling(s)
        || baseline.requires_confirmation().is_err()
        || observed(s, &c) != baseline
        || preference_pending(s, &c.owner)
    {
        return None;
    }
    let id = next_id();
    s.bulk_read
        .entries
        .entry(c.owner.clone())
        .or_default()
        .preference_message = None;
    s.bulk_read
        .entries
        .entry(c.owner.clone())
        .or_default()
        .preference = Some(PrefOperation {
        id,
        phase: PrefPhase::Preparing,
        message: Some("Checking the confirmation preference…".into()),
    });
    Some(PrefWork {
        id,
        context: c,
        baseline,
    })
}
pub(super) fn preflight_failed(s: &mut ShellState, w: &PrefWork, error: Option<&Error>) {
    if let Some(e) = error {
        remember(s, &w.context, e);
    }
    if let Some(e) = s.bulk_read.entries.get_mut(&w.context.owner).filter(|e| {
        e.preference
            .as_ref()
            .is_some_and(|p| p.id == w.id && p.phase == PrefPhase::Preparing)
    }) {
        e.preference = None;
        // This setting failure does not describe the independent bulk mutation.
        e.preference_message = Some("The confirmation preference was not changed.".into());
    }
}
pub(crate) fn preflight(
    s: &mut ShellState,
    w: &PrefWork,
    fresh: &ClearUnreadsPreference,
) -> Option<PreparedRequest> {
    if !w.context.current(s)
        || cooling(s)
        || observed(s, &w.context) != w.baseline
        || !s
            .bulk_read
            .entries
            .get(&w.context.owner)
            .and_then(|e| e.preference.as_ref())
            .is_some_and(|p| p.id == w.id && p.phase == PrefPhase::Preparing)
    {
        preflight_failed(s, w, None);
        return None;
    }
    // A complete malformed snapshot revokes the old permission too.
    s.core
        .workspaces
        .get_mut(&w.context.owner.team)?
        .clear_unreads_preference = fresh.clone();
    let request = match api::disable_clear_unreads_confirmation_request(
        &w.context.client,
        &w.context.session,
        w.context.epoch,
        &w.context.api_owner,
        &w.baseline,
        fresh,
    ) {
        Ok(r) => r,
        Err(_) => {
            preflight_failed(s, w, None);
            return None;
        }
    };
    let p = s
        .bulk_read
        .entries
        .get_mut(&w.context.owner)?
        .preference
        .as_mut()?;
    p.phase = PrefPhase::Sending;
    p.message = Some("Saving the confirmation preference…".into());
    Some(request)
}
pub(crate) fn finish_write(s: &mut ShellState, w: &PrefWork, result: Result<(), Error>) -> bool {
    if let Err(e) = &result {
        remember(s, &w.context, e);
    }
    let current = w.context.current(s);
    let Some(e) = s.bulk_read.entries.get_mut(&w.context.owner) else {
        return false;
    };
    let Some(p) = e
        .preference
        .as_mut()
        .filter(|p| p.id == w.id && p.phase == PrefPhase::Sending)
    else {
        return false;
    };
    match result {
        Err(ref error) if api::read_shortcut_definitely_rejected(error) => {
            e.preference = None;
            e.preference_message = Some("The confirmation preference was not changed.".into());
            false
        }
        _ => {
            p.phase = PrefPhase::Unknown;
            p.message = Some(
                "The confirmation preference is unconfirmed. It will not be sent again.".into(),
            );
            current
        }
    }
}
pub(crate) fn finish_check(
    s: &mut ShellState,
    w: &PrefWork,
    result: Result<ClearUnreadsPreference, Error>,
) {
    if let Err(e) = &result {
        remember(s, &w.context, e);
    }
    if !w.context.current(s) {
        return;
    }
    if !s
        .bulk_read
        .entries
        .get(&w.context.owner)
        .and_then(|e| e.preference.as_ref())
        .is_some_and(|p| p.id == w.id && p.phase == PrefPhase::Unknown)
    {
        return;
    }
    let Ok(fresh) = result else { return };
    let current = observed(s, &w.context);
    if current != w.baseline && current != fresh {
        return;
    }
    s.core
        .workspaces
        .get_mut(&w.context.owner.team)
        .unwrap()
        .clear_unreads_preference = fresh.clone();
    if exact_false(&fresh)
        && let Some(e) = s
            .bulk_read
            .entries
            .get_mut(&w.context.owner)
            .filter(|e| e.preference.as_ref().is_some_and(|p| p.id == w.id))
    {
        e.preference = None;
    }
}
pub(crate) fn finish_read(
    s: &mut ShellState,
    r: &PrefRead,
    result: Result<ClearUnreadsPreference, Error>,
) -> Option<Next> {
    if let Err(e) = &result {
        remember(s, &r.context, e);
    }
    if !r.context.visit_current(s)
        || !s
            .bulk_read
            .dialog
            .as_ref()
            .is_some_and(|d| d.id == r.id && d.phase == DialogPhase::Loading)
    {
        return None;
    }
    if observed(s, &r.context) != r.observed {
        let d = s.bulk_read.dialog.as_mut().unwrap();
        d.phase = DialogPhase::Unavailable;
        d.error = Some("The preference changed while loading. Check again.".into());
        return None;
    }
    let fresh = match result {
        Ok(p) => p,
        Err(_) => {
            let d = s.bulk_read.dialog.as_mut().unwrap();
            d.phase = DialogPhase::Unavailable;
            d.error = Some(
                "The confirmation preference could not be loaded. Check again when connected."
                    .into(),
            );
            return None;
        }
    };
    s.core
        .workspaces
        .get_mut(&r.context.owner.team)
        .unwrap()
        .clear_unreads_preference = fresh.clone();
    if exact_false(&fresh)
        && let Some(e) = s.bulk_read.entries.get_mut(&r.context.owner)
    {
        if e.preference
            .as_ref()
            .is_some_and(|p| p.phase == PrefPhase::Unknown)
        {
            e.preference = None;
        }
    }
    match fresh.requires_confirmation() {
        Err(_) => {
            let d = s.bulk_read.dialog.as_mut().unwrap();
            d.phase = DialogPhase::Unavailable;
            d.error = Some(
                "Slack returned an unsupported confirmation preference. Nothing was cleared."
                    .into(),
            );
            None
        }
        Ok(false) if !preference_pending(s, &r.context.owner) => {
            s.bulk_read.dialog = None;
            start_clear(s, r.context.clone(), false).map(Next::Bulk)
        }
        Ok(_) => {
            let d = s.bulk_read.dialog.as_mut().unwrap();
            d.phase = DialogPhase::Confirm;
            d.preference = fresh;
            d.error = None;
            None
        }
    }
}
pub(super) fn retry(mut state: Signal<ShellState>, id: u64) {
    let r = {
        let mut s = state.write();
        if cooling(&s) {
            return;
        }
        let Some(d) = s
            .bulk_read
            .dialog
            .as_ref()
            .filter(|d| d.id == id && d.context.visit_current(&s))
        else {
            return;
        };
        let c = d.context.clone();
        let preference = observed(&s, &c);
        s.bulk_read.dialog.as_mut().unwrap().phase = DialogPhase::Loading;
        PrefRead {
            id,
            context: c,
            observed: preference,
        }
    };
    dispatch(state, Next::Preference(r));
}
