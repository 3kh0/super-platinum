use super::*;
pub(super) fn observed(s: &ShellState, t: &Token) -> BTreeMap<String, serde_json::Value> {
    let Some(w) = s.core.workspaces.get(&t.owner.team) else {
        return BTreeMap::new();
    };
    let mut out = BTreeMap::from([
        ("global".into(), serde_json::json!(w.notifications.global)),
        (
            "has_vips".into(),
            serde_json::json!(!w.vip_users.is_empty()),
        ),
    ]);
    out.insert("team_plan".into(), serde_json::json!(w.team_plan));
    for key in ["mute_sounds", "mac_ssb_bounce"] {
        if let Some(value) = w.notifications.user.get(key) {
            out.insert(key.into(), value.clone());
        }
    }
    out
}
fn pending_mut<'a>(s: &'a mut ShellState, a: &Attempt) -> Option<&'a mut Attempt> {
    s.presentation_notifications
        .entries
        .get_mut(&a.token.owner)?
        .pending
        .as_mut()
        .filter(|p| p.id == a.id)
}
pub(super) fn preflight_failed(s: &mut ShellState, a: &Attempt, error: Option<&Error>) {
    if let Some(e) = error {
        remember(s, &a.token, e);
    }
    let Some(entry) = s.presentation_notifications.entries.get_mut(&a.token.owner) else {
        return;
    };
    if entry
        .pending
        .as_ref()
        .is_some_and(|p| p.id == a.id && p.phase == Phase::Preparing)
    {
        entry.pending = None;
        entry.message = Some(
            "The preferences could not be verified. Nothing was changed. Reload to review it."
                .into(),
        );
        if let Some(v) = s
            .presentation_notifications
            .view
            .as_mut()
            .filter(|v| v.visit == a.visit)
        {
            v.snapshot = None;
        }
    }
}
pub(super) fn preflight(
    s: &mut ShellState,
    a: &Attempt,
    fresh: &api::PresentationNotificationPreferences,
) -> bool {
    let valid = current(s, &a.token, a.visit)
        && !cooling(s)
        && observed(s, &a.token) == a.observed
        && a.intent.revalidate(fresh).is_ok();
    if !valid {
        preflight_failed(s, a, None);
        return false;
    }
    let Some(p) = pending_mut(s, a).filter(|p| p.phase == Phase::Preparing) else {
        return false;
    };
    p.phase = Phase::Saving;
    s.presentation_notifications
        .entries
        .get_mut(&a.token.owner)
        .unwrap()
        .message = Some("Updating sound and appearance…".into());
    true
}
pub(super) fn begin_read(s: &mut ShellState, visit: u64) -> Option<Read> {
    let v = s.presentation_notifications.view.as_ref()?;
    let token = v.token.clone();
    if !current(s, &token, visit) || v.loading || cooling(s) {
        return None;
    }
    let context = context(s, &token);
    if context.is_none() && !fixture::enabled(s) && !cfg!(test) {
        s.presentation_notifications.view.as_mut()?.error =
            Some("Connect to Slack to load your sound and appearance.".into());
        return None;
    }
    let id = next_id();
    let observed = observed(s, &token);
    let operation = if let Some(p) = s
        .presentation_notifications
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
    let v = s.presentation_notifications.view.as_mut()?;
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
    let Some(entry) = s.presentation_notifications.entries.get_mut(&a.token.owner) else {
        return None;
    };
    let Some(p) = entry
        .pending
        .as_mut()
        .filter(|p| p.id == a.id && p.phase == Phase::Saving)
    else {
        return None;
    };
    let accepted = result.is_ok();
    match result {
        Ok(()) => {
            p.phase = Phase::Uncertain;
            entry.message = Some("Checking updated sound and appearance…".into());
        }
        Err(e) if !uncertain(&e) => {
            entry.pending = None;
            entry.message = Some(
                "Slack did not change sound and appearance. Reload before trying again.".into(),
            );
            if let Some(v) = s
                .presentation_notifications
                .view
                .as_mut()
                .filter(|v| v.token.owner == a.token.owner)
            {
                v.snapshot = None;
            }
            return None;
        }
        Err(_) => {
            p.phase = Phase::Uncertain;
            entry.message=Some("The update is unconfirmed. Check status before making another change; nothing will be resent.".into());
        }
    }
    if accepted && a.token.current(s) {
        let visit = s
            .presentation_notifications
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
    result: Result<api::PresentationNotificationPreferences, Error>,
) -> bool {
    if let Err(e) = &result {
        remember(s, &r.token, e);
    }
    let owns = current(s, &r.token, r.visit)
        && s.presentation_notifications
            .view
            .as_ref()
            .is_some_and(|v| v.read == r.id && v.loading);
    if let Some(id) = r.operation {
        let Some(p) = s
            .presentation_notifications
            .entries
            .get_mut(&r.token.owner)
            .and_then(|e| e.pending.as_mut())
            .filter(|p| p.id == id && p.read == Some(r.id) && p.phase == Phase::Checking)
        else {
            return false;
        };
        p.read = None;
        p.phase = Phase::Uncertain;
    }
    if !owns {
        return false;
    }
    s.presentation_notifications.view.as_mut().unwrap().loading = false;
    let fresh = match result {
        Ok(v) => v,
        Err(_) => {
            s.presentation_notifications.view.as_mut().unwrap().error=Some("Sound and appearance could not be loaded. Check your connection, then reload. No update was retried.".into());
            return false;
        }
    };
    if observed(s, &r.token) != r.observed {
        s.presentation_notifications.view.as_mut().unwrap().error = Some(
            "Sound and appearance changed during the check. Reload to review the latest preferences.".into(),
        );
        return false;
    }
    if fresh.settings().is_err() {
        s.presentation_notifications.view.as_mut().unwrap().error = Some(
            "Slack returned an unsupported notification preferences. No preferences were changed."
                .into(),
        );
        return false;
    }
    let settled = if let Some(id) = r.operation {
        let entry = s
            .presentation_notifications
            .entries
            .get_mut(&r.token.owner)
            .unwrap();
        let p = entry.pending.as_ref().unwrap();
        if p.id == id && p.intent.confirms(&fresh) {
            entry.pending = None;
            entry.message = Some("Sound and appearance updated.".into());
            true
        } else {
            entry.message = Some(
                "Slack has not confirmed the update. Check status again; nothing will be resent."
                    .into(),
            );
            false
        }
    } else {
        true
    };
    // Fresh canonical data is previewed even if an uncertain change differs; the
    // retained operation still prevents all further mutations.
    let view = s.presentation_notifications.view.as_mut().unwrap();
    if view.bounce_duration.is_none() {
        view.bounce_duration = Some(
            fresh
                .settings()
                .unwrap()
                .mac_bounce
                .filter(|v| !v.is_empty())
                .unwrap_or_else(|| "short".into()),
        );
    }
    view.snapshot = Some(fresh.clone());
    if let Some(w) = s
        .core
        .workspaces
        .get_mut(&r.token.owner.team)
        .filter(|w| w.self_user_id == r.token.owner.user)
    {
        w.notifications.global = fresh
            .raw()
            .get("all_notifications_prefs")
            .and_then(|p| p.get("global"))
            .and_then(serde_json::Value::as_object)
            .into_iter()
            .flatten()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        for key in ["mute_sounds", "mac_ssb_bounce"] {
            w.notifications.user.remove(key);
            if let Some(value) = fresh.raw().get(key) {
                w.notifications.user.insert(key.into(), value.clone());
            }
        }
    }
    s.refresh_dock_badge_for(&r.token.owner.team);
    settled
}
