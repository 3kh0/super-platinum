use super::*;
pub(super) fn observed(s: &ShellState, t: &Token) -> BTreeMap<String, serde_json::Value> {
    let Some(w) = s.core.workspaces.get(&t.owner.team) else {
        return BTreeMap::new();
    };
    let mut out: BTreeMap<_, _> = w
        .notifications
        .user
        .iter()
        .filter(|(k, _)| api::is_notification_schedule_pref(k) && k.as_str() != "time24")
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    out.insert(
        "__observed_user_timezone".into(),
        serde_json::json!(w.users.get(&t.owner.user).and_then(|u| u.tz.as_deref())),
    );
    out
}
fn pending_mut<'a>(s: &'a mut ShellState, a: &Attempt) -> Option<&'a mut Attempt> {
    s.notification_schedule
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
    let Some(entry) = s.notification_schedule.entries.get_mut(&a.token.owner) else {
        return;
    };
    if entry
        .pending
        .as_ref()
        .is_some_and(|p| p.id == a.id && p.phase == Phase::Preparing)
    {
        entry.pending = None;
        entry.message = Some(
            "The schedule could not be verified. Nothing was changed. Reload to review it.".into(),
        );
        if let Some(v) = s
            .notification_schedule
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
    fresh: &api::NotificationSchedule,
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
    s.notification_schedule
        .entries
        .get_mut(&a.token.owner)
        .unwrap()
        .message = Some("Updating working hours…".into());
    true
}
pub(super) fn begin_read(s: &mut ShellState, visit: u64) -> Option<Read> {
    let v = s.notification_schedule.view.as_ref()?;
    let token = v.token.clone();
    if !current(s, &token, visit) || v.loading || cooling(s) {
        return None;
    }
    let context = context(s, &token);
    if context.is_none() && !fixture::enabled(s) && !cfg!(test) {
        s.notification_schedule.view.as_mut()?.error =
            Some("Connect to Slack to load your working hours.".into());
        return None;
    }
    let id = next_id();
    let observed = observed(s, &token);
    let operation = if let Some(p) = s
        .notification_schedule
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
    let v = s.notification_schedule.view.as_mut()?;
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
    let Some(entry) = s.notification_schedule.entries.get_mut(&a.token.owner) else {
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
            entry.message = Some("Checking updated working hours…".into());
        }
        Err(e) if !uncertain(&e) => {
            entry.pending = None;
            entry.message =
                Some("Slack did not change working hours. Reload before trying again.".into());
            if let Some(v) = s
                .notification_schedule
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
            .notification_schedule
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
    result: Result<api::NotificationSchedule, Error>,
) -> bool {
    if let Err(e) = &result {
        remember(s, &r.token, e);
    }
    let owns = current(s, &r.token, r.visit)
        && s.notification_schedule
            .view
            .as_ref()
            .is_some_and(|v| v.read == r.id && v.loading);
    if let Some(id) = r.operation {
        let Some(p) = s
            .notification_schedule
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
    s.notification_schedule.view.as_mut().unwrap().loading = false;
    let fresh = match result {
        Ok(v) => v,
        Err(_) => {
            s.notification_schedule.view.as_mut().unwrap().error=Some("Working hours could not be loaded. Check your connection, then reload. No update was retried.".into());
            return false;
        }
    };
    if observed(s, &r.token) != r.observed {
        s.notification_schedule.view.as_mut().unwrap().error = Some(
            "Working hours changed during the check. Reload to review the latest schedule.".into(),
        );
        return false;
    }
    if fresh.view().is_err() {
        s.notification_schedule.view.as_mut().unwrap().error =
            Some("Slack returned an unsupported schedule. No preferences were changed.".into());
        return false;
    }
    let settled = if let Some(id) = r.operation {
        let entry = s
            .notification_schedule
            .entries
            .get_mut(&r.token.owner)
            .unwrap();
        let p = entry.pending.as_ref().unwrap();
        if p.id == id && p.intent.confirms(&fresh) {
            entry.pending = None;
            entry.message = Some("Working hours updated.".into());
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
    s.notification_schedule.view.as_mut().unwrap().snapshot = Some(fresh.clone());
    if let Some(w) = s
        .core
        .workspaces
        .get_mut(&r.token.owner.team)
        .filter(|w| w.self_user_id == r.token.owner.user)
    {
        w.notifications
            .user
            .retain(|k, _| !api::is_notification_schedule_pref(k));
        for (k, v) in fresh.raw() {
            w.notifications.user.insert(k.clone(), v.clone());
        }
        w.notifications.temporary_schedule_loaded = true;
        w.notifications.schedule_loaded = true;
    }
    settled
}
