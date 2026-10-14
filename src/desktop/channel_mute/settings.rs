//! General channel notification changes share mute's operation/owner/cooldown.
use super::*;
use serde_json::json;
use super_platinum_core::config::WorkspaceSession;
pub(crate) mod check;
pub(crate) mod fixture;
pub(super) mod network;
#[cfg(test)]
mod tests;
pub(crate) use api::{ChannelNotificationAction as Action, ChannelNotificationMode as Mode};
pub(crate) struct View {
    pub token: Token,
    pub settings: Option<api::ChannelNotificationSettings>,
    pub muted: bool,
    pub enabled: bool,
    pub message: Option<String>,
}
fn cached(shell: &ShellState, token: &Token) -> Result<api::NotificationPreferences, Error> {
    let config = &shell.core.workspaces[&token.owner.team].notifications;
    if !config.loaded {
        return Err(Error::Api("notification_prefs_not_loaded".into()));
    }
    // Only the target channel and globals feed this intent; unrelated channels
    // are never cloned on a render or serialized into a write.
    let channels = target(shell, &token.owner)
        .map(|v| json!({token.owner.channel.clone():v}))
        .unwrap_or_else(|| json!({}));
    api::parse_notification_preferences(
        &json!({"ok":true,"prefs":{"all_notifications_prefs":{"global":config.global,"channels":channels}}}),
    )
}
pub(crate) fn presentation(shell: &ShellState, channel: &str) -> Option<View> {
    let mute = super::presentation(shell, channel)?;
    let settings = cached(shell, &mute.token)
        .and_then(|p| p.channel_settings(channel, false))
        .ok();
    Some(View {
        enabled: mute.can_change && settings.is_some(),
        message: if settings.is_none() {
            Some("Refresh the workspace to load supported notification settings.".into())
        } else {
            mute.message
        },
        token: mute.token,
        settings,
        muted: mute.muted,
    })
}
pub(crate) fn change(mut state: Signal<ShellState>, token: Token, action: Action) {
    let attempt = { begin(&mut state.write(), token, action, now()) };
    let Some(attempt) = attempt else {
        return;
    };
    if fixture::dispatch(state, attempt.clone()) {
        return;
    }
    dioxus::core::spawn_forever(network::run(state, attempt));
}
fn begin(shell: &mut ShellState, token: Token, action: Action, now: i64) -> Option<Attempt> {
    if !token.current(shell) || presentation(shell, &token.owner.channel).is_none_or(|v| !v.enabled)
    {
        return None;
    }
    let baseline = cached(shell, &token).ok()?;
    let session = if let Some(context) = context(shell, &token) {
        context.session.clone()
    } else if fixture::enabled() || cfg!(test) {
        synthetic_session(&token)
    } else {
        return None;
    };
    let intent = Arc::new(
        api::ChannelNotificationIntent::capture(
            &session,
            &token.owner.channel,
            false,
            &baseline,
            action,
            now,
        )
        .ok()?,
    );
    if intent.confirms(&baseline, now) {
        return None;
    }
    let mut attempt = begin_change(
        shell,
        token,
        Some(api::ChannelMuteChange::Permanent(false)),
        None,
        now,
    )?;
    attempt.settings = Some(intent.clone());
    operation_mut(shell, &attempt)?.operation.as_mut()?.settings = Some(intent);
    Some(attempt)
}
fn synthetic_session(token: &Token) -> WorkspaceSession {
    WorkspaceSession {
        team_id: token.owner.team.clone(),
        enterprise_id: None,
        user_id: token.owner.user.clone(),
        name: "Notification fixture".into(),
        url: "https://fixture.slack.com".into(),
        token: "fixture-token".into(),
    }
}
fn preflight(
    shell: &mut ShellState,
    attempt: &Attempt,
    fresh: &api::NotificationPreferences,
    now: i64,
) -> bool {
    let valid = attempt.token.current(shell)
        && target(shell, &attempt.token.owner) == attempt.baseline
        && attempt.settings.as_ref().is_some_and(|i| {
            i.revalidate(fresh, now).is_ok()
                && cached(shell, &attempt.token)
                    .is_ok_and(|local| i.revalidate(&local, now).is_ok())
        });
    if !valid {
        preflight_failed(shell, attempt, None, now);
        return false;
    }
    mark_sending(shell, attempt)
}
pub(super) fn finish_read(
    shell: &mut ShellState,
    read: &Read,
    result: Result<api::NotificationPreferences, Error>,
    now: i64,
) -> bool {
    if let Err(e) = &result {
        remember_cooldown(shell, &read.token, e, now);
    }
    let current = read.token.current(shell);
    let same = target(shell, &read.token.owner) == read.baseline;
    let Some(entry) = shell.channel_mute.entries.get_mut(&read.token.owner) else {
        return false;
    };
    let Some(op) = entry.operation.as_mut().filter(|o| {
        o.id == read.operation && o.read_id == Some(read.id) && o.phase == Phase::Checking
    }) else {
        return false;
    };
    let Some(intent) = op.settings.clone() else {
        return false;
    };
    op.read_id = None;
    op.phase = Phase::Uncertain;
    if !current {
        entry.message = Some("Return to this channel and check the change's status.".into());
        return false;
    }
    let prefs = match result {
        Ok(v) => v,
        Err(e) => {
            entry.retry_at = entry.retry_at.max(cooldown(&e, now));
            entry.message =
                Some("Notification settings could not be checked. No change was retried.".into());
            return false;
        }
    };
    if !same {
        entry.message =
            Some("Notification settings changed during the check. Check status again.".into());
        return false;
    }
    let settled = intent.confirms(&prefs, now);
    if settled {
        entry.operation = None;
        entry.settled = None;
        entry.message = Some("Notification settings saved.".into());
    } else {
        entry.message = Some(
            "Slack has not confirmed these settings. Check status again; nothing will be resent."
                .into(),
        );
    }
    // Partial readbacks never erase known values. Only validated canonical
    // settings are adopted, and only this intent's explicitly affected keys.
    if settled {
        adopt(shell, &read.token.owner, &prefs, intent.changed_keys(), now);
    }
    settled
}
fn adopt(
    shell: &mut ShellState,
    owner: &Owner,
    prefs: &api::NotificationPreferences,
    keys: &[String],
    now: i64,
) {
    let Some(w) = shell
        .core
        .workspaces
        .get_mut(&owner.team)
        .filter(|w| w.self_user_id == owner.user)
    else {
        return;
    };
    let canonical = prefs.raw()["channels"].get(&owner.channel);
    let target = w
        .notifications
        .channels
        .entry(owner.channel.clone())
        .or_default();
    for key in keys {
        if let Some(v) = canonical.and_then(|c| c.get(key)) {
            target.insert(key.clone(), v.clone());
        } else {
            target.remove(key);
        }
    }
    w.sidebar.sync_notification_mutes(&w.notifications, now);
    shell.refresh_from_core();
}
