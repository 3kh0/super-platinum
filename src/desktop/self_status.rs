//! Account-owned status drafts and one-attempt profile writes. No automatic resend.
use crate::{
    bootstrap::SelfMenuContext,
    state::{Overlay, ShellState},
};
use dioxus::prelude::*;
use serde_json::{Value, json};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use super_platinum_core::slack::{Error, PreparedRequest, SlackClient, api, models::UserProfile};
mod model;
pub(crate) use model::Expiration;
use model::{Draft, Phase, Scope, Status};
pub(crate) mod check;
pub(crate) mod fixture;
#[cfg(test)]
mod tests;

#[derive(Clone, Default)]
pub(crate) struct State {
    draft: Option<Draft>,
    pending: Option<Attempt>,
    pub(crate) fixture: Option<fixture::Fixture>,
}
#[derive(Clone)]
pub(super) struct Attempt {
    id: u64,
    revision: u64,
    scope: Scope,
    baseline: Status,
    desired: Status,
    context: Option<Arc<SelfMenuContext>>,
    pub(super) profile: Value,
}
impl Attempt {
    fn current(&self, shell: &ShellState) -> bool {
        self.scope.current(shell)
            && self
                .context
                .as_ref()
                .is_none_or(|context| context.current(shell))
            && shell
                .self_status
                .pending
                .as_ref()
                .is_some_and(|pending| pending.id == self.id)
            && shell
                .self_status
                .draft
                .as_ref()
                .is_some_and(|draft| draft.revision == self.revision)
    }
}
fn next_id() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}
fn now() -> i64 {
    super_platinum_core::state::now_secs()
}

pub(crate) struct Presentation {
    pub text: String,
    pub emoji: String,
    pub expiration: Expiration,
    pub custom_datetime: String,
    pub error: Option<String>,
    pub busy: bool,
    pub uncertain: bool,
    pub can_save: bool,
    pub revision: u64,
}
pub(crate) fn presentation(shell: &ShellState) -> Option<Presentation> {
    let draft = shell
        .self_status
        .draft
        .as_ref()
        .filter(|draft| draft.scope.current(shell))?;
    let now = now();
    let validation = draft.profile(now);
    Some(Presentation {
        text: draft.text.clone(),
        emoji: draft.emoji.clone(),
        expiration: draft.expiration,
        custom_datetime: draft.custom_datetime.clone(),
        error: draft
            .error
            .clone()
            .or_else(|| validation.as_ref().err().map(|error| (*error).to_owned())),
        busy: matches!(draft.phase, Phase::Saving | Phase::Checking),
        uncertain: matches!(draft.phase, Phase::Uncertain | Phase::Checking),
        can_save: draft.phase == Phase::Editing && draft.retry_at <= now && validation.is_ok(),
        revision: draft.revision,
    })
}
pub(crate) fn open(mut state: Signal<ShellState>) {
    open_shell(&mut state.write(), now());
}
pub(super) fn open_shell(shell: &mut ShellState, now: i64) {
    poll(shell);
    if shell.self_status.draft.is_none() {
        shell.self_status.draft = Draft::open(shell, now);
    }
    if shell.self_status.draft.is_some() {
        shell.shortcut_generation = shell.shortcut_generation.wrapping_add(1);
        shell.overlay = Some(Overlay::SelfStatus);
    }
}
pub(crate) fn close(mut state: Signal<ShellState>) {
    close_shell(&mut state.write());
}
pub(crate) fn close_shell(shell: &mut ShellState) {
    if shell
        .self_status
        .draft
        .as_ref()
        .is_some_and(|draft| matches!(draft.phase, Phase::Saving | Phase::Checking))
    {
        return;
    }
    if shell.overlay == Some(Overlay::SelfStatus) {
        shell.overlay = None;
    }
    if shell.self_status.pending.is_none() {
        shell.self_status.draft = None;
    }
}
/// Retire disconnected account/session drafts and superseded local intentions.
pub(crate) fn poll(shell: &mut ShellState) -> bool {
    let stale = shell
        .self_status
        .draft
        .as_ref()
        .is_some_and(|draft| !draft.scope.current(shell))
        || shell
            .self_status
            .pending
            .as_ref()
            .is_some_and(|attempt| !attempt.current(shell));
    if stale {
        shell.self_status.draft = None;
        shell.self_status.pending = None;
        if shell.overlay == Some(Overlay::SelfStatus) {
            shell.overlay = None;
        }
    }
    if !stale
        && let Some(draft) = shell.self_status.draft.as_mut()
        && draft.retry_at > 0
        && draft.retry_at <= now()
    {
        draft.retry_at = 0;
        draft.error = Some("You can try saving again. Your draft is still here.".into());
        return true;
    }
    stale
}
fn change(mut state: Signal<ShellState>, edit: impl FnOnce(&mut Draft)) {
    let mut shell = state.write();
    poll(&mut shell);
    if let Some(draft) = shell
        .self_status
        .draft
        .as_mut()
        .filter(|draft| draft.phase == Phase::Editing)
    {
        edit(draft);
        draft.error = None;
    }
}
pub(crate) fn change_text(state: Signal<ShellState>, value: String) {
    change(state, |draft| draft.text = value);
}
pub(crate) fn change_emoji(state: Signal<ShellState>, value: String) {
    change(state, |draft| {
        let value = value.trim().trim_matches(':');
        draft.emoji = if value.is_empty() {
            String::new()
        } else {
            format!(":{value}:")
        };
    });
}
pub(crate) fn change_expiration(state: Signal<ShellState>, value: Expiration) {
    change(state, |draft| draft.expiration = value);
}
pub(crate) fn change_custom_datetime(state: Signal<ShellState>, value: String) {
    change(state, |draft| {
        draft.custom_datetime = value;
        draft.original_expiration = None;
    });
}

pub(super) fn begin(shell: &mut ShellState, now: i64) -> Result<Attempt, &'static str> {
    poll(shell);
    let draft = shell
        .self_status
        .draft
        .as_ref()
        .ok_or("Open the status editor again.")?;
    if draft.phase != Phase::Editing {
        return Err("Check the previous status change before saving again.");
    }
    if draft.retry_at > now {
        return Err("Slack asked us to wait before trying again.");
    }
    let (profile, desired) = draft.profile(now)?;
    let scope = draft.scope.clone();
    let revision = draft.revision;
    let baseline = scope.status(shell);
    let context = if fixture::enabled(shell) {
        None
    } else {
        Some(Arc::new(
            SelfMenuContext::begin_status(shell).ok_or("Sign in to save your status.")?,
        ))
    };
    let attempt = Attempt {
        id: next_id(),
        revision,
        scope,
        baseline,
        desired,
        context,
        profile,
    };
    shell.self_status.pending = Some(attempt.clone());
    let draft = shell.self_status.draft.as_mut().expect("draft checked");
    draft.phase = Phase::Saving;
    draft.error = None;
    Ok(attempt)
}
pub(super) fn prepare(
    attempt: &Attempt,
    client: &SlackClient,
    session: &super_platinum_core::config::WorkspaceSession,
) -> PreparedRequest {
    api::users_profile_set(client, session, attempt.profile.to_string())
}
pub(super) fn finish(
    shell: &mut ShellState,
    attempt: &Attempt,
    result: Result<UserProfile, Error>,
) {
    if !attempt.current(shell) {
        return;
    }
    match result {
        Ok(profile) => {
            let current = attempt.scope.status(shell);
            if current != attempt.baseline && current != attempt.desired {
                unlock(
                    shell,
                    "Your status changed elsewhere while saving. Review it before saving again.",
                );
                return;
            }
            attempt
                .scope
                .merge(shell, &Status::from_profile(Some(&profile)));
            shell.self_status.pending = None;
            shell.self_status.draft = None;
            if shell.overlay == Some(Overlay::SelfStatus) {
                shell.overlay = None;
            }
        }
        Err(error) => {
            let uncertain = outcome_uncertain(&error);
            if !uncertain {
                shell.self_status.pending = None;
            }
            let draft = shell.self_status.draft.as_mut().expect("attempt current");
            draft.phase = if uncertain {
                Phase::Uncertain
            } else {
                Phase::Editing
            };
            draft.error = Some(
                if uncertain {
                    "Slack has not confirmed this change. Check your status before saving again."
                } else if matches!(error, Error::RateLimited { .. }) {
                    "Slack asked us to wait before trying again. Your draft is still here."
                } else {
                    "Slack could not save your status. Your draft is still here."
                }
                .into(),
            );
            if let Error::RateLimited {
                retry_after_secs: Some(seconds),
            }
            | Error::HttpStatus {
                status: 429,
                retry_after_secs: Some(seconds),
            } = error
            {
                draft.retry_at = now().saturating_add(i64::try_from(seconds).unwrap_or(i64::MAX));
            }
        }
    }
}
fn unlock(shell: &mut ShellState, error: &str) {
    shell.self_status.pending = None;
    if let Some(draft) = shell.self_status.draft.as_mut() {
        draft.phase = Phase::Editing;
        draft.error = Some(error.into());
    }
}
fn outcome_uncertain(error: &Error) -> bool {
    match error {
        Error::Api(code) => {
            matches!(
                code.as_str(),
                "internal_error" | "fatal_error" | "service_unavailable"
            ) || code.starts_with("decode_")
        }
        Error::RateLimited { .. } | Error::TransportNotConfigured | Error::MissingWorkspace(_) => {
            false
        }
        Error::HttpStatus { status, .. } => *status >= 500 || *status == 408,
        _ => true,
    }
}
fn set_error(shell: &mut ShellState, error: &str) {
    if let Some(draft) = shell.self_status.draft.as_mut() {
        draft.error = Some(error.into());
    }
}

pub(crate) fn save(mut state: Signal<ShellState>) {
    if fixture::enabled(&state.read()) {
        fixture::save(&mut state.write());
        return;
    }
    let started = {
        let mut shell = state.write();
        begin(&mut shell, now())
    };
    let attempt = match started {
        Ok(attempt) => attempt,
        Err(error) => {
            set_error(&mut state.write(), error);
            return;
        }
    };
    dioxus::core::spawn_forever(async move {
        let request = attempt
            .context
            .as_ref()
            .expect("real save context")
            .request();
        let result = request
            .transport
            .execute(prepare(&attempt, &request.client, &request.session))
            .await
            .and_then(decode_profile);
        finish(&mut state.write(), &attempt, result);
    });
}

pub(super) fn begin_check(shell: &mut ShellState) -> Option<Attempt> {
    poll(shell);
    let attempt = shell.self_status.pending.clone()?;
    let draft = shell.self_status.draft.as_mut()?;
    if draft.phase != Phase::Uncertain {
        return None;
    }
    draft.phase = Phase::Checking;
    draft.error = None;
    Some(attempt)
}
pub(super) fn finish_check(
    shell: &mut ShellState,
    attempt: &Attempt,
    result: Result<UserProfile, Error>,
) {
    if !attempt.current(shell) {
        return;
    }
    match result {
        Ok(profile) => {
            let read = Status::from_profile(Some(&profile));
            let current = attempt.scope.status(shell);
            if current != attempt.baseline && current != attempt.desired && current != read {
                unlock(
                    shell,
                    "Your status changed while checking. Review it before saving again.",
                );
                return;
            }
            attempt.scope.merge(shell, &read);
            if read.clone().effective(now()) == attempt.desired.clone().effective(now()) {
                shell.self_status.pending = None;
                shell.self_status.draft = None;
                if shell.overlay == Some(Overlay::SelfStatus) {
                    shell.overlay = None;
                }
            } else {
                unlock(
                    shell,
                    "Slack currently shows a different status. Review your draft before saving again.",
                );
            }
        }
        Err(_) => {
            if let Some(draft) = shell.self_status.draft.as_mut() {
                draft.phase = Phase::Uncertain;
                draft.error = Some(
                    "Could not check your status. Your draft is retained; try checking again."
                        .into(),
                );
            }
        }
    }
}
pub(crate) fn check_status(mut state: Signal<ShellState>) {
    if fixture::enabled(&state.read()) {
        fixture::check_status(&mut state.write());
        return;
    }
    let Some(attempt) = begin_check(&mut state.write()) else {
        return;
    };
    dioxus::core::spawn_forever(async move {
        let request = attempt
            .context
            .as_ref()
            .expect("real status check context")
            .request();
        let result = request
            .transport
            .execute(api::users_profile_get(
                &request.client,
                &request.session,
                request.session.user_id.clone(),
            ))
            .await
            .and_then(decode_profile);
        finish_check(&mut state.write(), &attempt, result);
    });
}

/// Missing/defaulted profile data is not evidence that Slack cleared a status.
fn decode_profile(value: Value) -> Result<UserProfile, Error> {
    let malformed = || Error::Transport("Malformed status profile response".into());
    if value.get("ok").and_then(Value::as_bool) != Some(true) {
        return Err(malformed());
    }
    let profile = value
        .get("profile")
        .filter(|profile| profile.is_object())
        .ok_or_else(malformed)?;
    if profile.get("status_text").and_then(Value::as_str).is_none()
        || profile
            .get("status_emoji")
            .and_then(Value::as_str)
            .is_none()
        || profile
            .get("status_expiration")
            .and_then(Value::as_i64)
            .is_none_or(|value| value < 0)
    {
        return Err(malformed());
    }
    serde_json::from_value(profile.clone()).map_err(|_| malformed())
}
