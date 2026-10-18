use super::*;

pub(super) async fn load(mut state: Signal<ShellState>, token: Token, visit: u64, serial: u64) {
    if !current(&state.peek(), &token, visit, serial) {
        return;
    }
    let ctx = context(&state.peek(), &token);
    let result = match ctx {
        Some(c) => api::fetch_own_profile_sections(&c.transport, &c.client, &c.session).await,
        None => Err(Error::TransportNotConfigured),
    };
    finish_load(&mut state.write(), &token, visit, serial, result);
}
pub(super) fn finish_load(
    shell: &mut ShellState,
    token: &Token,
    visit: u64,
    serial: u64,
    result: Result<api::OwnProfileSections, Error>,
) {
    if !current(shell, token, visit, serial) {
        return;
    }
    let d = shell.profile_edit.sections.draft.as_mut().unwrap();
    d.loading = false;
    match result {
        Ok(snapshot)
            if snapshot.team_id() == token.owner.team && snapshot.user_id() == token.owner.user =>
        {
            d.snapshot = Some(snapshot);
            d.message = None;
        }
        result => {
            d.message = Some("Could not load your profile fields. Close and try again.".into());
            let e = shell
                .profile_edit
                .sections
                .entries
                .entry(token.owner.clone())
                .or_default();
            e.retry_at = e.retry_at.max(result.as_ref().err().map_or(0, cooldown));
        }
    }
}
pub(super) async fn save(mut state: Signal<ShellState>, a: SectionAttempt) {
    if !current(&state.peek(), &a.token, a.visit, a.serial) {
        preflight_error(&mut state.write(), &a, None);
        return;
    }
    let Some(c) = context(&state.peek(), &a.token) else {
        preflight_error(&mut state.write(), &a, None);
        return;
    };
    let fresh = match api::fetch_own_profile_sections(&c.transport, &c.client, &c.session).await {
        Ok(v) => v,
        Err(error) => {
            preflight_error(&mut state.write(), &a, Some(&error));
            return;
        }
    };
    let ids = a.intent.draft().changed_person_ids();
    if !ids.is_empty() {
        if !current(&state.peek(), &a.token, a.visit, a.serial) {
            preflight_error(&mut state.write(), &a, None);
            return;
        }
        let result = match super_platinum_core::slack::edge::users_info(&c.client, &c.session, &ids)
        {
            Ok(request) => match c.transport.execute(request).await {
                Ok(raw) => decode_people(&ids, raw),
                Err(error) => Err(error),
            },
            Err(_) => Err(Error::Api("invalid_profile_people_request".into())),
        };
        match result {
            Ok(users) if directory::validates(&ids, &users) => {}
            result => {
                preflight_error(&mut state.write(), &a, result.as_ref().err());
                return;
            }
        }
    }
    if !preflight(&mut state.write(), &a, &fresh) {
        return;
    }
    let result =
        protocol::set_section(&c.transport, &c.client, &c.session, &a.intent, &fresh).await;
    let accepted = finish_write(&mut state.write(), &a, result);
    if accepted {
        let next = { begin_check(&mut state.write(), a.visit, a.serial) };
        if let Some(read) = next {
            check(state, read).await;
        }
    }
}
// This response authorizes a write. The generic display/cache helper deliberately
// discards envelope status and failed IDs, which is insufficient here.
pub(super) fn decode_people(
    ids: &[String],
    raw: Value,
) -> Result<Vec<super_platinum_core::slack::models::User>, Error> {
    let invalid = || Error::Api("invalid_profile_people_response".into());
    if raw.get("ok") != Some(&Value::Bool(true))
        || raw.get("error").is_some_and(|v| !v.is_null())
        || !raw.get("results").is_some_and(Value::is_array)
        || raw
            .get("failed_ids")
            .is_some_and(|v| !v.as_array().is_some_and(Vec::is_empty))
    {
        return Err(invalid());
    }
    let users: Vec<super_platinum_core::slack::models::User> =
        serde_json::from_value(raw["results"].clone()).map_err(|_| invalid())?;
    if users.len() != ids.len() || !directory::validates(ids, &users) {
        return Err(invalid());
    }
    Ok(users)
}
pub(super) fn preflight(
    shell: &mut ShellState,
    a: &SectionAttempt,
    fresh: &api::OwnProfileSections,
) -> bool {
    let allowed = current(shell, &a.token, a.visit, a.serial)
        && !competing(shell, &a.token.owner)
        && a.intent.revalidate(fresh).is_ok();
    let Some(e) = entry_mut(shell, a) else {
        return false;
    };
    if !e
        .pending
        .as_ref()
        .is_some_and(|a| a.phase == Phase::Preparing)
    {
        return false;
    }
    if !allowed {
        e.pending = None;
        e.message =
            Some("Your profile or permissions changed. Reopen these fields before saving.".into());
        return false;
    }
    e.pending.as_mut().unwrap().phase = Phase::Saving;
    true
}
fn preflight_error(shell: &mut ShellState, a: &SectionAttempt, error: Option<&Error>) {
    let Some(e) = entry_mut(shell, a) else {
        return;
    };
    if !e
        .pending
        .as_ref()
        .is_some_and(|a| a.phase == Phase::Preparing)
    {
        return;
    }
    e.pending = None;
    e.retry_at = e.retry_at.max(error.map_or(0, cooldown));
    e.message = Some("Could not verify this change. No section update was sent.".into());
}
pub(super) fn finish_write(
    shell: &mut ShellState,
    a: &SectionAttempt,
    result: Result<(), Error>,
) -> bool {
    let current = current(shell, &a.token, a.visit, a.serial);
    let Some(e) = entry_mut(shell, a) else {
        return false;
    };
    if !e.pending.as_ref().is_some_and(|a| a.phase == Phase::Saving) {
        return false;
    }
    match result {
        Ok(()) => {
            e.pending.as_mut().unwrap().phase = Phase::Uncertain;
            e.message = Some("Checking your saved profile fields…".into());
            current
        }
        Err(error) => {
            e.retry_at = e.retry_at.max(cooldown(&error));
            if uncertain(&error) {
                e.pending.as_mut().unwrap().phase = Phase::Uncertain;
                e.message = Some(
                    "Slack has not confirmed this change. Check your profile before saving again."
                        .into(),
                );
            } else {
                e.pending = None;
                e.message =
                    Some("Slack could not save these fields. Your draft is retained.".into());
            }
            false
        }
    }
}
pub(super) async fn check(mut state: Signal<ShellState>, a: SectionAttempt) {
    if !current(&state.peek(), &a.token, a.visit, a.serial) {
        finish_check(&mut state.write(), &a, Err(Error::TransportNotConfigured));
        return;
    }
    let ctx = context(&state.peek(), &a.token);
    let result = match ctx {
        Some(c) => api::fetch_own_profile_sections(&c.transport, &c.client, &c.session).await,
        None => Err(Error::TransportNotConfigured),
    };
    finish_check(&mut state.write(), &a, result);
}
pub(super) fn finish_check(
    shell: &mut ShellState,
    a: &SectionAttempt,
    result: Result<api::OwnProfileSections, Error>,
) {
    let current = current(shell, &a.token, a.visit, a.serial);
    let Some(e) = entry_mut(shell, a) else {
        return;
    };
    if !e
        .pending
        .as_ref()
        .is_some_and(|p| p.phase == Phase::Checking && p.read == a.read)
    {
        return;
    }
    let p = e.pending.as_mut().unwrap();
    p.phase = Phase::Uncertain;
    p.read = None;
    if !current {
        return;
    }
    match result {
        Ok(snapshot)
            if snapshot.team_id() == a.token.owner.team
                && snapshot.user_id() == a.token.owner.user =>
        {
            let matched = a.intent.confirms_readback(&snapshot);
            e.checked = Some(snapshot.clone());
            e.message=Some(if matched{"Profile fields saved."}else{"Slack currently shows different values. Check again, or keep the current section without sending another change."}.into());
            if matched {
                e.pending = None;
                let d = shell.profile_edit.sections.draft.as_mut().unwrap();
                d.form = a.intent.draft().reopen(&snapshot).ok();
                d.snapshot = Some(snapshot);
                d.serial = next_id();
            }
            // A differing read is evidence about the server, not permission to
            // discard the proposed edit. Keep both the visible draft and its
            // predecessor until confirmation or explicit Keep current section.
        }
        result => {
            e.retry_at = e.retry_at.max(result.as_ref().err().map_or(0, cooldown));
            e.message =
                Some("Could not check these fields. The earlier save remains unconfirmed.".into());
        }
    }
}
