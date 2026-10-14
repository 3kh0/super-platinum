use super::*;

pub(super) fn finish_load(
    shell: &mut ShellState,
    token: &Token,
    visit: u64,
    result: Result<api::FreshOwnProfileCapabilities, Error>,
) {
    if !draft_current(shell, token, visit) {
        return;
    }
    match result {
        Ok(cap) if cap.team_id() == token.owner.team && cap.user_id() == token.owner.user => {
            let draft = shell.profile_edit.draft.as_mut().unwrap();
            draft.loading = false;
            if draft.capabilities.is_none() {
                draft.baseline = Values::from_profile(Some(cap.profile()));
                draft.values = draft.baseline.clone();
            }
            draft.capabilities = Some(cap);
            draft.error = None;
        }
        result => {
            let retry = result.as_ref().err().map_or(0, cooldown);
            let entry = shell
                .profile_edit
                .entries
                .entry(token.owner.clone())
                .or_default();
            entry.retry_at = entry.retry_at.max(retry);
            let draft = shell.profile_edit.draft.as_mut().unwrap();
            draft.loading = false;
            draft.error =
                Some("Could not verify profile editing permissions. Try loading again.".into());
        }
    }
}
pub(super) fn draft_current(shell: &ShellState, token: &Token, visit: u64) -> bool {
    token.current(shell)
        && shell.overlay == Some(Overlay::ProfileEdit)
        && shell
            .profile_edit
            .draft
            .as_ref()
            .is_some_and(|d| d.visit == visit && d.token == *token)
}
pub(super) fn begin(shell: &mut ShellState, visit: u64) -> Option<Attempt> {
    let view = presentation(shell)?;
    if view.visit != visit || !view.can_save || shell.overlay != Some(Overlay::ProfileEdit) {
        return None;
    }
    let draft = shell.profile_edit.draft.as_ref()?;
    if draft.token.owner.values(shell) != draft.observed {
        shell.profile_edit.draft.as_mut()?.error=Some("Your profile changed elsewhere. Close and reopen this editor to review the latest values.".into());
        return None;
    }
    let context = if fixture::enabled(shell) {
        None
    } else {
        Some(context(shell, &draft.token)?)
    };
    let attempt = Attempt {
        id: next_id(),
        visit,
        token: draft.token.clone(),
        baseline: draft.baseline.clone(),
        observed: draft.observed.clone(),
        desired: draft.values.clone(),
        patch: draft.values.patch(&draft.baseline),
        phase: Phase::Preparing,
        write_finished: false,
        read_id: None,
        context,
    };
    let entry = shell
        .profile_edit
        .entries
        .entry(attempt.token.owner.clone())
        .or_default();
    if entry.pending.is_some() || entry.retry_at > now() {
        return None;
    }
    entry.message = None;
    entry.checked = false;
    entry.pending = Some(attempt.clone());
    Some(attempt)
}
pub(super) fn pending_mut<'a>(
    shell: &'a mut ShellState,
    owner: &Owner,
    id: u64,
) -> Option<&'a mut Entry> {
    shell
        .profile_edit
        .entries
        .get_mut(owner)
        .filter(|e| e.pending.as_ref().is_some_and(|a| a.id == id))
}
pub(super) fn preflight(
    shell: &mut ShellState,
    attempt: &Attempt,
    fresh: &api::FreshOwnProfileCapabilities,
) -> bool {
    let allowed = draft_current(shell, &attempt.token, attempt.visit)
        && !crate::self_status::blocks_profile_edit(shell)
        && !crate::profile_title::blocks_profile_writes(shell)
        && attempt.token.owner.values(shell) == attempt.observed
        && fresh.team_id() == attempt.token.owner.team
        && fresh.user_id() == attempt.token.owner.user
        && Values::from_profile(Some(fresh.profile())) == attempt.baseline
        && FIELDS
            .into_iter()
            .filter(|f| attempt.patch.get(f.key()).is_some())
            .all(|f| fresh.capability(f).allowed());
    let Some(entry) = pending_mut(shell, &attempt.token.owner, attempt.id) else {
        return false;
    };
    if entry.pending.as_ref().unwrap().phase != Phase::Preparing {
        return false;
    }
    if !allowed {
        entry.pending = None;
        entry.message=Some("Your profile or editing permissions changed. Close and reopen this editor before saving.".into());
        return false;
    }
    entry.pending.as_mut().unwrap().phase = Phase::Saving;
    true
}
pub(super) fn preflight_failed(shell: &mut ShellState, attempt: &Attempt, error: Option<&Error>) {
    let Some(entry) = pending_mut(shell, &attempt.token.owner, attempt.id) else {
        return;
    };
    if entry.pending.as_ref().unwrap().phase != Phase::Preparing {
        return;
    }
    entry.pending = None;
    entry.retry_at = entry.retry_at.max(error.map_or(0, cooldown));
    entry.message = Some(
        "Could not verify this change. Your draft is retained; no profile update was sent.".into(),
    );
}
pub(super) fn finish_write(
    shell: &mut ShellState,
    attempt: &Attempt,
    result: Result<(), Error>,
) -> bool {
    let current = attempt.token.current(shell);
    let Some(entry) = pending_mut(shell, &attempt.token.owner, attempt.id) else {
        return false;
    };
    let pending = entry.pending.as_mut().unwrap();
    if pending.write_finished || pending.phase != Phase::Saving {
        return false;
    }
    pending.write_finished = true;
    match result {
        Ok(()) => {
            pending.phase = Phase::Uncertain;
            entry.message = Some("Checking your saved profile…".into());
            current
        }
        Err(error) => {
            entry.retry_at = entry.retry_at.max(cooldown(&error));
            if uncertain(&error) {
                pending.phase = Phase::Uncertain;
                entry.message = Some(
                    "Slack has not confirmed this change. Check your profile before saving again."
                        .into(),
                );
            } else {
                entry.pending = None;
                entry.message =
                    Some("Slack could not save your profile. Your draft is retained.".into());
            }
            false
        }
    }
}
pub(super) fn begin_check(shell: &mut ShellState, visit: u64) -> Option<(Token, u64, u64)> {
    let draft = shell.profile_edit.draft.as_ref()?;
    if draft.visit != visit || !draft_current(shell, &draft.token, visit) {
        return None;
    }
    let token = draft.token.clone();
    let entry = shell.profile_edit.entries.get_mut(&token.owner)?;
    let pending = entry.pending.as_mut()?;
    if pending.phase != Phase::Uncertain || entry.retry_at > now() {
        return None;
    }
    let read = next_id();
    pending.phase = Phase::Checking;
    pending.read_id = Some(read);
    entry.checked = false;
    Some((token, pending.id, read))
}
pub(super) fn finish_check(
    shell: &mut ShellState,
    token: &Token,
    id: u64,
    read: u64,
    result: Result<UserProfile, Error>,
) {
    let current = token.current(shell);
    let Some(entry) = pending_mut(shell, &token.owner, id) else {
        return;
    };
    let attempt = entry.pending.as_ref().unwrap().clone();
    if attempt.read_id != Some(read) || attempt.phase != Phase::Checking {
        return;
    }
    entry.pending.as_mut().unwrap().read_id = None;
    entry.pending.as_mut().unwrap().phase = Phase::Uncertain;
    if !current {
        return;
    }
    match result {
        Ok(profile) => {
            let canonical = Values::from_profile(Some(&profile));
            let matches = FIELDS
                .into_iter()
                .filter(|f| attempt.patch.get(f.key()).is_some())
                .all(|f| canonical.get(f) == attempt.desired.get(f));
            entry.checked = true;
            entry.message=Some(if matches{"Profile saved."}else{"Slack currently shows different values. Check again, or keep the current profile without sending another change."}.into());
            if matches {
                entry.pending = None;
            }
            // Only this operation's changed text fields may be adopted, and a
            // newer realtime value must not be overwritten by an older read.
            let observed = token.owner.values(shell);
            if let Some(user) = shell
                .core
                .workspaces
                .get_mut(&token.owner.team)
                .and_then(|w| w.users.get_mut(&token.owner.user))
            {
                let target = user.profile.get_or_insert_with(Default::default);
                for field in FIELDS {
                    if attempt.patch.get(field.key()).is_none() {
                        continue;
                    }
                    if observed.get(field) != attempt.observed.get(field)
                        && observed.get(field) != attempt.desired.get(field)
                    {
                        continue;
                    }
                    let value = Some(canonical.get(field).to_owned());
                    match field {
                        Field::FullName => {
                            target.real_name = value.clone();
                            user.real_name = value;
                        }
                        Field::DisplayName => target.display_name = value,
                        Field::Pronouns => target.pronouns = value,
                    }
                }
            }
            shell.refresh_from_core();
            if matches
                && shell
                    .profile_edit
                    .draft
                    .as_ref()
                    .is_some_and(|d| d.token == *token)
            {
                shell.profile_edit.draft = None;
                if shell.overlay == Some(Overlay::ProfileEdit) {
                    shell.overlay = None;
                }
            }
        }
        Err(error) => {
            entry.retry_at = entry.retry_at.max(cooldown(&error));
            entry.message=Some("Could not check your profile. The previous save remains unconfirmed; try checking again.".into());
        }
    }
}
