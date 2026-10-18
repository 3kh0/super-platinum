//! Concrete Init reducers and the SXpy temporary-mute timer.
//! Source: tmp/cfm-auxiliary-source-20261018/{FxVs,Cqrp,A_zd,3spc,SXpy}.js.
//! Onboarding setters store source state; they do not perform onboarding navigation.
use super::super::Owner;
use super::{ShellState, source};
use dioxus::prelude::*;
use serde_json::{Map, Value, json};
use std::{
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

pub(super) struct State {
    onboarding: Map<String, Value>,
    acquisition_source: Value,
    user_date_created: Option<Value>,
    redirect_slackbot: Value,
    eligible_big_mode: Value,
    redirect_big_mode: Value,
    slackbot_channel: Option<Value>,
    timer: Option<Timer>,
    timer_error: Option<source::Error>,
}
struct Timer {
    token: Arc<()>,
    due: Instant,
    // Replacing or dropping a timer cancels its sleeping task immediately.
    cancel: Option<tokio::sync::oneshot::Sender<()>>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            onboarding: json!({
                "onboarding_complete":true,"needs_initial_password_set":false,
                "tractor_enabled":false,"welcome_place_state":"",
                "send_your_first_message_banner_enabled":false,"created_with_google":false,
                "initial_channel_id":"","has_sent_invite_during_team_creation":false,
                "onboardingChannelId":"","welcomeModel":{},
                "tractorMessageCoachmarkVisible":false,"tractorFilesCoachmarkVisible":false
            })
            .as_object()
            .unwrap()
            .clone(),
            acquisition_source: json!(""),
            user_date_created: None,
            redirect_slackbot: json!(false),
            eligible_big_mode: json!(false),
            redirect_big_mode: json!(false),
            slackbot_channel: None,
            timer: None,
            timer_error: None,
        }
    }
}
impl State {
    fn reduce(&mut self, effect: &source::Effect) -> Result<(), source::Error> {
        use source::Kind;
        let value = &effect.payload;
        match effect.kind {
            Kind::OnboardingData => self.onboarding.extend(
                value
                    .as_object()
                    .ok_or(source::Error::Malformed("onboarding data"))?
                    .clone(),
            ),
            Kind::OnboardingChannel => {
                self.onboarding
                    .insert("onboardingChannelId".into(), value.clone());
            }
            Kind::AcquisitionSource => self.acquisition_source = value.clone(),
            Kind::UserDateCreated => self.user_date_created = Some(value.clone()),
            Kind::RedirectSlackbot => self.redirect_slackbot = value.clone(),
            Kind::EligibleBigMode => self.eligible_big_mode = value.clone(),
            Kind::RedirectBigMode => self.redirect_big_mode = value.clone(),
            Kind::SlackbotChannel => self.slackbot_channel = Some(value.clone()),
            // NlSN advances durable minLastFetched auth state, consumed on the
            // next startup. A memory-only flag would not execute that action.
            Kind::ForceAuthAtStartup => return Err(source::Error::Unsupported(effect.kind)),
            // RkCx.N7 is an actual permission request, not a boolean setter.
            Kind::SlackbotPermission => return Err(source::Error::Unsupported(effect.kind)),
            _ => return Err(source::Error::Unsupported(effect.kind)),
        }
        Ok(())
    }
    fn schedule(&mut self, channels: &Value, enabled: bool, now: f64) -> Result<(), source::Error> {
        // Source clearTimeout occurs before checking the gate or reading prefs.
        self.timer = None;
        self.timer_error = None;
        if !enabled || channels.is_null() {
            return Ok(());
        }
        let delay = next_delay(channels, now)?;
        self.timer = delay.map(|delay| Timer {
            token: Arc::new(()),
            due: Instant::now() + delay,
            cancel: None,
        });
        Ok(())
    }
    fn current_timer(&self, token: &Arc<()>) -> bool {
        self.timer
            .as_ref()
            .is_some_and(|timer| Arc::ptr_eq(&timer.token, token))
    }
}

pub(super) fn apply(
    shell: &mut ShellState,
    owner: &Owner,
    effect: &source::Effect,
) -> Result<(), source::Error> {
    if !owner.current(shell) {
        return Err(source::Error::Superseded);
    }
    let slot = shell
        .app_home
        .channel_fetches
        .slots
        .get_mut(&owner.context.key())
        .ok_or(source::Error::Superseded)?;
    if effect.kind == source::Kind::TempMuteTimer {
        slot.payload.auxiliary.schedule(
            &effect.payload,
            slot.transformations.temp_mute,
            unix_now()?,
        )
    } else {
        slot.payload.auxiliary.reduce(effect)
    }
}

/// Called after each serial dispatch, including a later failed action: an
/// earlier successfully scheduled timer is not rolled back by that failure.
pub(super) fn kick(mut state: Signal<ShellState>, owner: Owner) {
    let (token, due, mut cancelled) = {
        let Ok(mut shell) = state.try_write() else {
            return;
        };
        if !owner.current(&shell) {
            return;
        }
        let auxiliary = &mut shell
            .app_home
            .channel_fetches
            .slots
            .get_mut(&owner.context.key())
            .unwrap()
            .payload
            .auxiliary;
        let Some(timer) = auxiliary.timer.as_mut() else {
            return;
        };
        if timer.cancel.is_some() {
            return;
        }
        let (send, receive) = tokio::sync::oneshot::channel();
        timer.cancel = Some(send);
        (timer.token.clone(), timer.due, receive)
    };
    dioxus::core::spawn_forever(async move {
        tokio::select! {
            _ = &mut cancelled => return,
            _ = tokio::time::sleep_until(due.into()) => {}
        }
        let rearm = {
            let Ok(mut shell) = state.try_write() else {
                return;
            };
            if !owner.current(&shell)
                || !shell.app_home.channel_fetches.slots[&owner.context.key()]
                    .payload
                    .auxiliary
                    .current_timer(&token)
            {
                return;
            }
            match fire(&mut shell, &owner) {
                Ok(rearm) => rearm,
                Err(error) => {
                    let auxiliary = &mut shell
                        .app_home
                        .channel_fetches
                        .slots
                        .get_mut(&owner.context.key())
                        .unwrap()
                        .payload
                        .auxiliary;
                    auxiliary.timer = None;
                    auxiliary.timer_error = Some(error);
                    super::super::failed(&mut shell, &owner, super::super::unavailable());
                    false
                }
            }
        };
        if rearm {
            kick(state, owner);
        }
    });
}

fn fire(shell: &mut ShellState, owner: &Owner) -> Result<bool, source::Error> {
    let now = unix_now()?;
    let team = &owner.context.session.team_id;
    let workspace = shell
        .core
        .workspaces
        .get_mut(team)
        .ok_or(source::Error::Superseded)?;
    if !workspace.notifications.loaded {
        return Err(source::Error::Unavailable(
            "current notification channel preferences",
        ));
    }
    // This is the existing native sink for boot + pref_change. Reading the
    // originally scheduled payload here would miss intervening preference edits.
    let channels = serde_json::to_value(&workspace.notifications.channels)
        .map_err(|_| source::Error::Adoption)?;
    let muted = current_mutes(&channels, now)?;
    let changed = workspace.cfm_payload.muted_channels != muted;
    if changed {
        workspace.cfm_payload.muted_channels = muted;
        workspace.sidebar.muted = workspace
            .cfm_payload
            .muted_channels
            .keys()
            .cloned()
            .collect();
    }
    let slot = shell
        .app_home
        .channel_fetches
        .slots
        .get_mut(&owner.context.key())
        .ok_or(source::Error::Superseded)?;
    slot.payload
        .auxiliary
        .schedule(&channels, slot.transformations.temp_mute, now)?;
    let rearm = slot.payload.auxiliary.timer.is_some();
    if changed {
        shell.refresh_from_core();
    }
    Ok(rearm)
}
fn unix_now() -> Result<f64, source::Error> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs_f64().floor())
        .map_err(|_| source::Error::Unavailable("current unix seconds"))
}
fn expiry(value: &Value) -> Result<f64, source::Error> {
    if value.is_null() {
        return Err(source::Error::Malformed("notification channel"));
    }
    source::temp_mute_number(value.get("temp_mute_expiration_ts").unwrap_or(&Value::Null))
}
fn next_delay(channels: &Value, now: f64) -> Result<Option<Duration>, source::Error> {
    let channels = channels
        .as_object()
        .ok_or(source::Error::Malformed("notification channels"))?;
    let mut next: Option<f64> = None;
    for value in channels.values() {
        let expiration = expiry(value)?;
        if expiration > now && next.is_none_or(|next| expiration < next) {
            next = Some(expiration);
        }
    }
    Ok(next.map(|expiration| Duration::from_secs_f64((expiration - now).clamp(15.0, 300.0))))
}
fn truth(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::Number(value) => value.as_f64() != Some(0.0),
        Value::String(value) => !value.is_empty(),
        _ => true,
    }
}
fn current_mutes(
    channels: &Value,
    now: f64,
) -> Result<std::collections::BTreeMap<String, Value>, source::Error> {
    let channels = channels
        .as_object()
        .ok_or(source::Error::Malformed("notification channels"))?;
    let mut muted = std::collections::BTreeMap::new();
    for (id, prefs) in channels {
        if prefs.get("muted").is_some_and(truth) || expiry(prefs)? > now {
            muted.insert(id.clone(), json!(true));
        }
    }
    Ok(muted)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn effect(kind: source::Kind, payload: Value) -> source::Effect {
        source::Effect { kind, payload }
    }

    #[test]
    fn onboarding_merges_fields_and_dedicated_channel_setter_overwrites_same_store() {
        let mut state = State::default();
        assert_eq!(state.onboarding["onboarding_complete"], true);
        assert_eq!(state.onboarding["welcomeModel"], json!({}));
        state
            .reduce(&effect(
                source::Kind::OnboardingData,
                json!({"onboarding_complete":false,"welcomeModel":{"first":1},"extra":null}),
            ))
            .unwrap();
        state
            .reduce(&effect(
                source::Kind::OnboardingData,
                json!({"welcomeModel":{"second":2}}),
            ))
            .unwrap();
        state
            .reduce(&effect(source::Kind::OnboardingChannel, json!("C_ONBOARD")))
            .unwrap();
        assert_eq!(state.onboarding["welcomeModel"], json!({"second":2}));
        assert_eq!(state.onboarding["onboarding_complete"], false);
        assert_eq!(state.onboarding["extra"], Value::Null);
        assert_eq!(state.onboarding["onboardingChannelId"], "C_ONBOARD");
    }
    #[test]
    fn setters_preserve_source_presence_null_and_non_boolean_values() {
        let mut state = State::default();
        assert_eq!(state.user_date_created, None);
        for (kind, value) in [
            (source::Kind::UserDateCreated, Value::Null),
            (source::Kind::RedirectSlackbot, json!(false)),
            (source::Kind::EligibleBigMode, Value::Null),
            (source::Kind::RedirectBigMode, json!("source-value")),
            (source::Kind::SlackbotChannel, json!("D_SLACKBOT")),
            (source::Kind::AcquisitionSource, json!("invite")),
        ] {
            state.reduce(&effect(kind, value)).unwrap();
        }
        assert_eq!(state.user_date_created, Some(Value::Null));
        assert_eq!(state.redirect_slackbot, false);
        assert_eq!(state.eligible_big_mode, Value::Null);
        assert_eq!(state.redirect_big_mode, "source-value");
        assert_eq!(state.slackbot_channel, Some(json!("D_SLACKBOT")));
        assert_eq!(state.acquisition_source, "invite");
    }
    #[test]
    fn real_side_effects_do_not_succeed_as_stored_flags() {
        for kind in [
            source::Kind::ForceAuthAtStartup,
            source::Kind::SlackbotPermission,
        ] {
            assert_eq!(
                State::default().reduce(&effect(kind, Value::Null)),
                Err(source::Error::Unsupported(kind))
            );
        }
    }
    #[test]
    fn source_timer_clamps_nearest_future_expiration_and_includes_permanent_rows() {
        for (expiration, expected) in [(101.0, 15.0), (180.5, 80.5), (1000.0, 300.0)] {
            assert_eq!(
                next_delay(
                    &json!({"C":{"muted":true,"temp_mute_expiration_ts":expiration}}),
                    100.0
                )
                .unwrap(),
                Some(Duration::from_secs_f64(expected))
            );
        }
        assert_eq!(next_delay(&json!({"C":{"temp_mute_expiration_ts":"0x78"},"D":{"temp_mute_expiration_ts":150}}),100.0).unwrap(),Some(Duration::from_secs(20)));
        assert_eq!(
            next_delay(&json!({"C":{"temp_mute_expiration_ts":100}}), 100.0).unwrap(),
            None
        );
    }
    #[test]
    fn expired_latest_preferences_clear_mutes_without_altering_canonical_data() {
        let current = json!({"C":{"temp_mute_expiration_ts":100,"unknown":"retained"}});
        assert!(current_mutes(&current, 100.0).unwrap().is_empty());
        assert_eq!(current["C"]["unknown"], "retained");
        assert_eq!(
            current_mutes(
                &json!({"C":{"muted":"false"},"D":{"temp_mute_expiration_ts":110}}),
                100.0
            )
            .unwrap(),
            std::collections::BTreeMap::from([
                ("C".into(), json!(true)),
                ("D".into(), json!(true))
            ])
        );
    }
    #[test]
    fn replacement_cancels_sleep_and_rejects_old_timer_token_even_when_disabled() {
        let mut state = State::default();
        state
            .schedule(&json!({"C":{"temp_mute_expiration_ts":120}}), true, 100.0)
            .unwrap();
        let token = state.timer.as_ref().unwrap().token.clone();
        let (sender, mut receiver) = tokio::sync::oneshot::channel();
        state.timer.as_mut().unwrap().cancel = Some(sender);
        state
            .schedule(&json!({"D":{"temp_mute_expiration_ts":160}}), true, 100.0)
            .unwrap();
        assert!(!state.current_timer(&token));
        assert_eq!(
            receiver.try_recv(),
            Err(tokio::sync::oneshot::error::TryRecvError::Closed)
        );
        state
            .schedule(&json!({"D":{"temp_mute_expiration_ts":160}}), false, 100.0)
            .unwrap();
        assert!(state.timer.is_none());
    }
}
