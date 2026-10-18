use super::*;
#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Queued,
    Sent,
    Unknown,
}
pub(super) struct Attempt {
    id: u64,
    owner: Owner,
    app: String,
    key: Key,
    connection: Connection,
    phase: Phase,
    pub(super) token: Arc<crate::app_modal::ActionToken>,
    pub(super) cancelled: bool,
}
impl Attempt {
    pub(super) fn unknown_fixture(
        owner: Owner,
        app: String,
        key: Key,
        connection: Connection,
        token: Arc<crate::app_modal::ActionToken>,
    ) -> Self {
        Self {
            id: next(),
            owner,
            app,
            key,
            connection,
            phase: Phase::Unknown,
            token,
            cancelled: false,
        }
    }
}
pub(super) fn enabled(s: &ShellState, key: Key) -> bool {
    if !read_current(s, key) {
        return false;
    }
    let h = s.app_home.about.surface.as_ref().unwrap();
    let Some(connection) = h.connection.as_ref() else {
        return false;
    };
    let Some(home) = h.home.as_ref() else {
        return false;
    };
    !h.loading
        && h.connection_enabled == Some(true)
        && (connection.can_connect(true) || connection.can_manage(true))
        && s.app_home
            .about
            .attempts
            .iter()
            .filter(|a| a.owner.ctx.epoch == h.owner.ctx.epoch)
            .count()
            < 64
        && !s.app_home.about.attempts.iter().any(|a| {
            a.owner.ctx.epoch == h.owner.ctx.epoch
                && a.owner.ctx.session.team_id == h.owner.ctx.session.team_id
                && a.app == home.app
        })
}
fn queued_current(s: &ShellState, id: u64) -> bool {
    let Some(a) = s.app_home.about.attempts.iter().find(|a| a.id == id) else {
        return false;
    };
    a.phase == Phase::Queued
        && !a.cancelled
        && read_current(s, a.key)
        && crate::app_modal::action_scope_current(s, &a.token)
        && s.app_home.about.surface.as_ref().is_some_and(|h| {
            h.connection_enabled == Some(true)
                && h.connection.as_ref() == Some(&a.connection)
                && h.home.as_ref().is_some_and(|v| v.app == a.app)
        })
}
pub(super) fn launch(mut state: Signal<ShellState>, key: Key) {
    let prepared = {
        let mut s = state.write();
        if !enabled(&s, key) {
            return;
        }
        let h = s.app_home.about.surface.as_ref().unwrap();
        if h.fixture {
            fixture::connect(&mut s, key);
            return;
        }
        let owner = h.owner.clone();
        let app = h.home.as_ref().unwrap().app.clone();
        let connection = h.connection.clone().unwrap();
        let Some(token) = crate::app_modal::register_action_with_context(&mut s, &app, None, None)
        else {
            return;
        };
        let request =
            match connection.submit(&owner.ctx.client, &owner.ctx.session, &token.token, true) {
                Ok(r) => r,
                Err(_) => {
                    crate::app_modal::reject_action(&mut s, &token);
                    return;
                }
            };
        let token = Arc::new(token);
        let id = next();
        s.app_home
            .about
            .attempts
            .retain(|a| a.owner.ctx.epoch == owner.ctx.epoch);
        s.app_home
            .about
            .correlations
            .retain(|(_, o)| o.ctx.epoch == owner.ctx.epoch);
        if s.app_home.about.correlations.len() >= 64 {
            crate::app_modal::reject_action(&mut s, &token);
            return;
        }
        s.app_home
            .about
            .correlations
            .push((token.token.clone(), owner.clone()));
        s.app_home.about.attempts.push(Attempt {
            id,
            owner: owner.clone(),
            app,
            key,
            connection,
            phase: Phase::Queued,
            token: token.clone(),
            cancelled: false,
        });
        (owner, id, token, request)
    };
    dioxus::core::spawn_forever(async move {
        let (owner, id, token, request) = prepared;
        {
            let mut s = state.write();
            if !queued_current(&s, id) {
                s.app_home.about.attempts.retain(|a| a.id != id);
                crate::app_modal::reject_action(&mut s, &token);
                return;
            }
            s.app_home
                .about
                .attempts
                .iter_mut()
                .find(|a| a.id == id)
                .unwrap()
                .phase = Phase::Sent;
        }
        let result = owner
            .ctx
            .transport
            .execute_block_interaction(request)
            .await
            .and_then(|raw| api::decode_block_action_response(&raw));
        settle(&mut state.write(), &owner, id, &token, result);
    });
}
fn settle(
    s: &mut ShellState,
    owner: &Owner,
    id: u64,
    token: &crate::app_modal::ActionToken,
    result: Result<api::BlockActionResponse, Error>,
) {
    if let Err(Error::RateLimited { retry_after_secs }) = &result {
        navigation::retain_limit(s, &owner.ctx, *retry_after_secs);
    }
    // An accepted modal can precede the HTTP acknowledgement. It already owns
    // its controls; this response must not replace or reject the new modal.
    let Some(key) = s
        .app_home
        .about
        .attempts
        .iter()
        .find(|a| a.id == id)
        .map(|a| a.key)
    else {
        return;
    };
    let unsent = matches!(&result, Err(Error::RateLimited { .. }))
        || matches!(&result,Err(Error::Api(e)) if e=="block_interaction_not_sent_offline");
    let confirmed = result.is_ok() || unsent;
    if confirmed {
        s.app_home.about.attempts.retain(|a| a.id != id);
        if !matches!(result, Ok(api::BlockActionResponse::Accepted)) {
            crate::app_modal::reject_action(s, token);
        }
    } else if let Some(a) = s.app_home.about.attempts.iter_mut().find(|a| a.id == id) {
        a.phase = Phase::Unknown;
    }
    if !owner.current(s) || !current(s, key) {
        return;
    }
    if let Err(e) = &result {
        failure(s, &owner.ctx, e)
    }
    if let Some(h) = s.app_home.about.surface.as_mut() {
        h.connection = None;
        h.notice=Some(match result{
            Ok(api::BlockActionResponse::Accepted)=>"The app received the request. Its connection form may follow. Refresh to read the latest status.",
            Ok(api::BlockActionResponse::Rejected(_))=>"The app declined the connection request. Refresh to check its current status.",
            Err(_) if unsent=>"The request was not sent. Wait for the connection or rate limit, then refresh.",
            Err(_)=>"The connection request is unconfirmed and will not be sent again. You can refresh its status or leave this page.",
        }.into());
    }
}
pub(super) fn status(s: &ShellState) -> Option<(u64, bool)> {
    let h = s
        .app_home
        .about
        .surface
        .as_ref()
        .filter(|h| h.owner.current(s))?;
    let app = &h.home.as_ref()?.app;
    s.app_home
        .about
        .attempts
        .iter()
        .find(|a| {
            a.owner.ctx.epoch == h.owner.ctx.epoch
                && a.owner.ctx.session.team_id == h.owner.ctx.session.team_id
                && &a.app == app
        })
        .map(|a| {
            (
                a.id,
                a.phase == Phase::Unknown || a.cancelled || !a.owner.current(s),
            )
        })
}
pub(super) fn cancel(s: &mut ShellState, id: u64) {
    let Some(a) = s
        .app_home
        .about
        .attempts
        .iter()
        .find(|a| a.id == id && a.owner.current(s))
    else {
        return;
    };
    let queued = a.phase == Phase::Queued;
    let token = a.token.clone();
    crate::app_modal::reject_action(s, &token);
    if queued {
        s.app_home.about.attempts.retain(|a| a.id != id);
    } else if let Some(a) = s.app_home.about.attempts.iter_mut().find(|a| a.id == id) {
        a.cancelled = true;
    }
    if let Some(h) = s.app_home.about.surface.as_mut() {
        h.notice=Some(if queued{"Connection request cancelled before sending."}else{"Stopped waiting for the app. The request may have reached it and will not be replayed."}.into());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn setup(phase: Phase) -> (ShellState, u64, Owner, Arc<crate::app_modal::ActionToken>) {
        let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
        fixture::install(&mut s, true);
        let a = s.app_home.about.attempts.first_mut().unwrap();
        a.phase = phase;
        let (id, owner, token) = (a.id, a.owner.clone(), a.token.clone());
        (s, id, owner, token)
    }
    fn deliver(s: &mut ShellState, owner: &Owner, token: &crate::app_modal::ActionToken) {
        let event=RtEvent::Unknown(serde_json::from_value(serde_json::json!({"type":"view_opened","view_type":"modal","view_id":"V_HOME_FORM","app_id":"A_HOME","client_token":token.token,"view":{"id":"V_HOME_FORM","type":"modal","app_id":"A_HOME","title":{"type":"plain_text","text":"Connect"},"close":{"type":"plain_text","text":"Cancel"},"blocks":[]}})).unwrap());
        crate::app_modal::home_fixture_open(s, &owner.ctx.session.team_id, owner.rt, &event);
    }
    #[test]
    fn queued_cancel_revokes_real_token_but_sent_cancel_retains_unknown_attempt() {
        for phase in [Phase::Queued, Phase::Sent] {
            let (mut s, id, owner, token) = setup(phase);
            cancel(&mut s, id);
            deliver(&mut s, &owner, &token);
            assert_eq!(s.overlay, None);
            if phase == Phase::Queued {
                assert!(s.app_home.about.attempts.is_empty());
            } else {
                assert!(s.app_home.about.attempts[0].cancelled);
                assert!(status(&s).unwrap().1);
            }
        }
    }
    #[test]
    fn changed_owner_cannot_cancel_new_attempt_or_adopt_late_failure() {
        let (mut s, id, owner, token) = setup(Phase::Sent);
        s.channel_generation += 1;
        cancel(&mut s, id);
        assert!(!s.app_home.about.attempts[0].cancelled);
        let notice = s.app_home.about.surface.as_ref().unwrap().notice.clone();
        settle(
            &mut s,
            &owner,
            id,
            &token,
            Err(Error::Api("unconfirmed".into())),
        );
        assert!(s.app_home.about.attempts[0].phase == Phase::Unknown);
        assert_eq!(s.app_home.about.surface.as_ref().unwrap().notice, notice);
    }
    #[test]
    fn known_unsent_rate_limit_retains_cooldown_without_unconfirmed_replay_block() {
        let (mut s, id, owner, token) = setup(Phase::Sent);
        settle(
            &mut s,
            &owner,
            id,
            &token,
            Err(Error::RateLimited {
                retry_after_secs: Some(5),
            }),
        );
        assert!(s.app_home.about.attempts.is_empty());
        assert!(cooling(&s, &owner.ctx));
        deliver(&mut s, &owner, &token);
        assert_eq!(s.overlay, None);
    }
    #[test]
    fn actual_modal_before_http_reply_is_not_rejected_by_late_error() {
        let (mut s, id, owner, token) = setup(Phase::Sent);
        deliver(&mut s, &owner, &token);
        assert_eq!(s.overlay, Some(Overlay::AppModal));
        assert!(s.app_home.about.attempts.is_empty());
        let before = s.app_home.about.surface.as_ref().unwrap().notice.clone();
        settle(
            &mut s,
            &owner,
            id,
            &token,
            Err(Error::Api("late_reply".into())),
        );
        assert_eq!(s.app_home.about.surface.as_ref().unwrap().notice, before);
        assert!(crate::app_modal::action_scope_current(&s, &token));
    }

    #[test]
    fn same_owner_refresh_keeps_new_presentation_while_original_attempt_settles() {
        for accepted in [true, false] {
            let (mut s, id, owner, token) = setup(Phase::Sent);
            let prior_key = s.app_home.about.attempts[0].key;
            let connection = s
                .app_home
                .about
                .surface
                .as_ref()
                .unwrap()
                .connection
                .clone();
            s.app_home.about.surface.as_mut().unwrap().fixture = false;
            let (_, fresh_key) = begin_read(&mut s).unwrap();
            assert!(owner.current(&s));
            assert_ne!(prior_key.id, fresh_key.id);
            let h = s.app_home.about.surface.as_mut().unwrap();
            h.loading = false;
            h.connection = connection.clone();
            h.notice = Some("New refresh owns this presentation".into());
            let result = if accepted {
                Ok(api::BlockActionResponse::Accepted)
            } else {
                Err(Error::Api("old unconfirmed result".into()))
            };
            settle(&mut s, &owner, id, &token, result);
            let h = s.app_home.about.surface.as_ref().unwrap();
            assert_eq!(h.connection, connection);
            assert_eq!(
                h.notice.as_deref(),
                Some("New refresh owns this presentation")
            );
            assert!(current(&s, fresh_key));
            if accepted {
                assert!(s.app_home.about.attempts.is_empty());
            } else {
                assert!(s.app_home.about.attempts[0].phase == Phase::Unknown);
            }
        }
    }
}
