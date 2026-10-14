use super::*;
pub(super) fn observed(s: &ShellState, t: &Token) -> Option<ReadPreferences> {
    s.core
        .workspaces
        .get(&t.owner.team)
        .filter(|w| w.self_user_id == t.owner.user)
        .map(|w| w.read_preferences.clone())
}
pub(super) fn preflight_failed(s: &mut ShellState, a: &Attempt, error: Option<&Error>) {
    if let Some(e) = error {
        remember(s, &a.token, e);
    }
    let Some(e) = s.read_settings.entries.get_mut(&a.token.owner) else {
        return;
    };
    if e.pending
        .as_ref()
        .is_some_and(|p| p.id == a.id && p.phase == Phase::Preparing)
    {
        e.pending = None;
        e.message = Some("Reading preferences could not be verified. Nothing was changed. Reload to review them.".into());
        if let Some(v) = s.read_settings.view.as_mut().filter(|v| v.visit == a.visit) {
            v.snapshot = None;
        }
    }
}
pub(super) fn preflight(s: &mut ShellState, a: &Attempt, fresh: &ReadPreferences) -> bool {
    if !current(s, &a.token, a.visit)
        || cooling(s)
        || observed(s, &a.token) != a.observed
        || a.intent.revalidate(fresh).is_err()
    {
        preflight_failed(s, a, None);
        return false;
    }
    let Some(e) = s.read_settings.entries.get_mut(&a.token.owner) else {
        return false;
    };
    let Some(p) = e
        .pending
        .as_mut()
        .filter(|p| p.id == a.id && p.phase == Phase::Preparing)
    else {
        return false;
    };
    p.phase = Phase::Saving;
    e.message = Some("Updating reading preferences…".into());
    true
}
pub(super) fn rejected_fresh(s: &mut ShellState, a: &Attempt, fresh: &ReadPreferences) {
    // A complete fresh read that rejects our baseline can revoke an old read
    // policy, but must never replace a newer local realtime observation.
    if current(s, &a.token, a.visit) && observed(s, &a.token) == a.observed {
        s.core
            .workspaces
            .get_mut(&a.token.owner.team)
            .unwrap()
            .read_preferences = fresh.clone();
    }
    preflight_failed(s, a, None);
}
pub(super) fn begin_read(s: &mut ShellState, visit: u64) -> Option<Read> {
    let v = s.read_settings.view.as_ref()?;
    let token = v.token.clone();
    if !current(s, &token, visit) || v.loading || cooling(s) {
        return None;
    }
    let context = context(s, &token);
    if context.is_none() && !fixture::enabled(s) && !cfg!(test) {
        s.read_settings.view.as_mut()?.error =
            Some("Connect to Slack to load reading preferences.".into());
        return None;
    }
    let id = next_id();
    let observed = observed(s, &token);
    let operation = if let Some(p) = s
        .read_settings
        .entries
        .get_mut(&token.owner)
        .and_then(|e| e.pending.as_mut())
    {
        if p.phase != Phase::Uncertain {
            return None;
        }
        p.phase = Phase::Checking;
        p.read = Some(id);
        Some(p.id)
    } else {
        None
    };
    let v = s.read_settings.view.as_mut()?;
    v.loading = true;
    v.error = None;
    v.read = id;
    Some(Read {
        id,
        visit,
        token,
        operation,
        observed,
        context,
    })
}
pub(super) fn finish_write(
    s: &mut ShellState,
    a: &Attempt,
    result: Result<(), Error>,
) -> Option<Read> {
    if let Err(e) = &result {
        remember(s, &a.token, e);
    }
    let entry = s.read_settings.entries.get_mut(&a.token.owner)?;
    let p = entry
        .pending
        .as_mut()
        .filter(|p| p.id == a.id && p.phase == Phase::Saving)?;
    let accepted = result.is_ok();
    match result {
        Err(e) if definitely_rejected(&e) => {
            entry.pending = None;
            entry.message = Some(
                "Slack did not change reading preferences. Reload before trying again.".into(),
            );
            if let Some(v) = s
                .read_settings
                .view
                .as_mut()
                .filter(|v| v.token.owner == a.token.owner)
            {
                v.snapshot = None;
            }
            return None;
        }
        _ => {
            p.phase = Phase::Uncertain;
            entry.message = Some("The update is unconfirmed. Check status before making another change; nothing will be resent.".into());
        }
    }
    if accepted && a.token.current(s) {
        let visit = s
            .read_settings
            .view
            .as_ref()
            .filter(|v| v.token.owner == a.token.owner)?
            .visit;
        begin_read(s, visit)
    } else {
        None
    }
}
pub(super) fn finish_read(
    s: &mut ShellState,
    r: &Read,
    result: Result<ReadPreferences, Error>,
) -> bool {
    if let Err(e) = &result {
        remember(s, &r.token, e);
    }
    let owns = current(s, &r.token, r.visit)
        && s.read_settings
            .view
            .as_ref()
            .is_some_and(|v| v.read == r.id && v.loading);
    // Retire this check even when its pane/account disappeared; never strand a
    // retained write in Checking or allow an unrelated receipt to clear it.
    if let Some(id) = r.operation {
        let Some(p) = s
            .read_settings
            .entries
            .get_mut(&r.token.owner)
            .and_then(|e| e.pending.as_mut())
            .filter(|p| p.id == id && p.read == Some(r.id) && p.phase == Phase::Checking)
        else {
            return false;
        };
        p.phase = Phase::Uncertain;
        p.read = None;
    }
    if !owns {
        return false;
    }
    let v = s.read_settings.view.as_mut().unwrap();
    v.loading = false;
    let fresh = match result {
        Ok(fresh) => fresh,
        Err(_) => {
            v.snapshot = None;
            v.error = Some("Reading preferences could not be loaded. Check your connection, then reload. No update was retried.".into());
            return false;
        }
    };
    if observed(s, &r.token) != r.observed {
        let v = s.read_settings.view.as_mut().unwrap();
        v.snapshot = None;
        v.error = Some("Reading preferences changed during the check. Reload to review the latest preferences.".into());
        return false;
    }
    // Complete canonical data must replace even a formerly valid preference.
    // Malformed present fields disable policy instead of retaining old authority.
    s.core
        .workspaces
        .get_mut(&r.token.owner.team)
        .unwrap()
        .read_preferences = fresh.clone();
    if fresh.mode().is_err() {
        let v = s.read_settings.view.as_mut().unwrap();
        v.snapshot = None;
        v.error = Some(
            "Slack returned an unsupported reading preference. Reload to check the current value."
                .into(),
        );
        return false;
    }
    let settled = if let Some(id) = r.operation {
        let e = s.read_settings.entries.get_mut(&r.token.owner).unwrap();
        let p = e.pending.as_ref().unwrap();
        if p.id == id && p.intent.confirms(&fresh) {
            e.pending = None;
            e.message = Some("Reading preferences updated.".into());
            true
        } else {
            e.message = Some(
                "Slack has not confirmed the update. Check status again; nothing will be resent."
                    .into(),
            );
            false
        }
    } else {
        if let Some(entry) = s
            .read_settings
            .entries
            .get_mut(&r.token.owner)
            .filter(|entry| entry.pending.is_none())
        {
            entry.message = None;
        }
        true
    };
    s.read_settings.view.as_mut().unwrap().snapshot = Some(fresh);
    settled
}
