use super::*;
pub(super) fn observed(s: &ShellState, t: &Token) -> BTreeMap<String, serde_json::Value> {
    let Some(w) = s.core.workspaces.get(&t.owner.team) else {
        return BTreeMap::new();
    };
    w.notifications.global.clone()
}
fn pending_mut<'a>(s: &'a mut ShellState, a: &Attempt) -> Option<&'a mut Attempt> {
    s.global_notifications
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
    let Some(entry) = s.global_notifications.entries.get_mut(&a.token.owner) else {
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
            .global_notifications
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
    fresh: &api::NotificationPreferences,
) -> bool {
    let valid = current(s, &a.token, a.visit)
        && !cooling(s)
        && observed(s, &a.token) == a.observed
        && s.global_notifications
            .view
            .as_ref()
            .is_some_and(|v| v.linked == Some(a.linked))
        && a.intent.revalidate(fresh).is_ok();
    if !valid {
        preflight_failed(s, a, None);
        return false;
    }
    let Some(p) = pending_mut(s, a).filter(|p| p.phase == Phase::Preparing) else {
        return false;
    };
    p.phase = Phase::Saving;
    s.global_notifications
        .entries
        .get_mut(&a.token.owner)
        .unwrap()
        .message = Some("Updating notification defaults…".into());
    true
}
pub(super) fn begin_read(s: &mut ShellState, visit: u64) -> Option<Read> {
    let v = s.global_notifications.view.as_ref()?;
    let token = v.token.clone();
    if !current(s, &token, visit) || v.loading || cooling(s) {
        return None;
    }
    let context = context(s, &token);
    if context.is_none() && !fixture::enabled(s) && !cfg!(test) {
        s.global_notifications.view.as_mut()?.error =
            Some("Connect to Slack to load your notification defaults.".into());
        return None;
    }
    let id = next_id();
    let observed = observed(s, &token);
    let operation = if let Some(p) = s
        .global_notifications
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
    let v = s.global_notifications.view.as_mut()?;
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
    let Some(entry) = s.global_notifications.entries.get_mut(&a.token.owner) else {
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
            entry.message = Some("Checking updated notification defaults…".into());
        }
        Err(e) if !uncertain(&e) => {
            entry.pending = None;
            entry.message = Some(
                "Slack did not change notification defaults. Reload before trying again.".into(),
            );
            if let Some(v) = s
                .global_notifications
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
            .global_notifications
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
    result: Result<api::NotificationPreferences, Error>,
) -> bool {
    if let Err(e) = &result {
        remember(s, &r.token, e);
    }
    let owns = current(s, &r.token, r.visit)
        && s.global_notifications
            .view
            .as_ref()
            .is_some_and(|v| v.read == r.id && v.loading);
    if let Some(id) = r.operation {
        let Some(p) = s
            .global_notifications
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
    s.global_notifications.view.as_mut().unwrap().loading = false;
    let fresh = match result {
        Ok(v) => v,
        Err(_) => {
            s.global_notifications.view.as_mut().unwrap().error=Some("Notification defaults could not be loaded. Check your connection, then reload. No update was retried.".into());
            return false;
        }
    };
    if observed(s, &r.token) != r.observed {
        s.global_notifications.view.as_mut().unwrap().error = Some(
            "Notification defaults changed during the check. Reload to review the latest defaults."
                .into(),
        );
        return false;
    }
    if fresh.global_settings().is_err() {
        s.global_notifications.view.as_mut().unwrap().error = Some(
            "Slack returned an unsupported notification settings. No preferences were changed."
                .into(),
        );
        return false;
    }
    let settled = if let Some(id) = r.operation {
        let entry = s
            .global_notifications
            .entries
            .get_mut(&r.token.owner)
            .unwrap();
        let p = entry.pending.as_ref().unwrap();
        if p.id == id && p.intent.confirms(&fresh) {
            let next_linked = p.next_linked;
            let keyword_generation = p.keyword_generation;
            if let Some(d) = entry.keywords.as_mut() {
                if keyword_generation == Some(d.generation) {
                    d.dirty = false;
                }
            }
            entry.pending = None;
            entry.message = Some("Notification defaults updated.".into());
            if let Some(linked) = next_linked {
                s.global_notifications.view.as_mut().unwrap().linked = Some(linked);
            }
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
    s.global_notifications.view.as_mut().unwrap().snapshot = Some(fresh.clone());
    let settings = fresh.global_settings().unwrap();
    let v = s.global_notifications.view.as_mut().unwrap();
    if v.linked.is_none() {
        v.linked = Some(settings.linked_mobile);
    }
    let entry = s
        .global_notifications
        .entries
        .entry(r.token.owner.clone())
        .or_default();
    if entry.keywords.as_ref().is_none_or(|d| !d.dirty) {
        entry.keywords = Some(KeywordDraft {
            text: api::notification_keywords_display(&settings.keywords),
            baseline: settings.keywords,
            generation: next_id(),
            seed: next_id(),
            dirty: false,
        });
    }
    if let Some(w) = s
        .core
        .workspaces
        .get_mut(&r.token.owner.team)
        .filter(|w| w.self_user_id == r.token.owner.user)
    {
        // The named read is complete for globals; never replace channel overrides
        // or unrelated user prefs with this response.
        w.notifications.global = fresh.raw()["global"]
            .as_object()
            .into_iter()
            .flatten()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
    }
    settled
}
