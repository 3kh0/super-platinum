use super::*;
use super_platinum_core::block_actions::BlockElementMetadata;
#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Pending,
    Uncertain,
}
pub(super) struct Attempt {
    id: u64,
    owner: Owner,
    button: Button,
    phase: Phase,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct Key {
    pub visit: u64,
    pub revision: u64,
}
fn capture(s: &ShellState, key: Key, m: &BlockElementMetadata) -> Option<(Owner, Button)> {
    if m.source != super_platinum_core::block_actions::BlockElementSource::Button {
        return None;
    }
    let h = s.app_home.surface.as_ref()?;
    if !h.owner.current(s)
        || h.id != key.visit
        || h.revision != key.revision
        || s.overlay.is_some()
        || h.loading
        || h.confirmation.is_some()
        || cooling(s, &h.owner.ctx)
    {
        return None;
    }
    let b = h
        .view
        .as_ref()?
        .button(
            m.block_id.as_deref()?,
            m.action_id.as_deref()?,
            &h.owner.ctx.session.user_id,
        )
        .ok()?;
    // Raw metadata remains the authority; DOM only names its owned key.
    if b.raw().get("value").and_then(serde_json::Value::as_str) != m.value.as_deref() {
        return None;
    }
    if s.app_home.attempts.iter().any(|a| {
        a.owner.ctx.epoch == h.owner.ctx.epoch
            && a.owner.ctx.session.team_id == h.owner.ctx.session.team_id
            && a.button == b
    }) {
        return None;
    }
    Some((h.owner.clone(), b))
}
pub(crate) fn enabled(s: &ShellState, key: Key, m: &BlockElementMetadata) -> bool {
    capture(s, key, m).is_some()
        && s.app_home
            .attempts
            .iter()
            .filter(|a| a.owner.ctx.epoch == s.core.account_epoch)
            .count()
            < 64
}
pub(crate) fn click(mut state: Signal<ShellState>, key: Key, m: BlockElementMetadata) {
    let Some((_, button)) = capture(&state.peek(), key, &m) else {
        return;
    };
    if button.raw().get("confirm").is_some() {
        state
            .write()
            .app_home
            .surface
            .as_mut()
            .unwrap()
            .confirmation = Some(button)
    } else {
        launch(state, key, button)
    }
}
pub(crate) fn cancel(mut state: Signal<ShellState>, key: Key) {
    if let Some(h) = state
        .write()
        .app_home
        .surface
        .as_mut()
        .filter(|h| h.id == key.visit && h.revision == key.revision)
    {
        h.confirmation = None
    }
}
pub(crate) fn confirm(mut state: Signal<ShellState>, key: Key) {
    let b = state
        .write()
        .app_home
        .surface
        .as_mut()
        .filter(|h| h.id == key.visit && h.revision == key.revision)
        .and_then(|h| h.confirmation.take());
    if let Some(b) = b {
        launch(state, key, b)
    }
}
// Called immediately before the once-only transport invocation. A queued action
// cannot outlive the exact raw view or an intervening surface/overlay change.
fn dispatch_current(s: &ShellState, id: u64, key: Key) -> bool {
    let Some(a) = s.app_home.attempts.iter().find(|a| a.id == id) else {
        return false;
    };
    let Some(h) = s.app_home.surface.as_ref() else {
        return false;
    };
    a.phase == Phase::Pending
        && a.owner.current(s)
        && h.id == key.visit
        && h.revision == key.revision
        && !h.loading
        && h.confirmation.is_none()
        && s.overlay.is_none()
        && !cooling(s, &a.owner.ctx)
        && h.view.as_ref().is_some_and(|v| a.button.same_view(v))
}
fn launch(mut state: Signal<ShellState>, key: Key, button: Button) {
    let url = button
        .raw()
        .get("url")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned);
    let prepared = {
        let mut s = state.write();
        let Some(h) = s.app_home.surface.as_ref() else {
            return;
        };
        if !h.owner.current(&s)
            || h.id != key.visit
            || h.revision != key.revision
            || s.overlay.is_some()
            || h.loading
            || cooling(&s, &h.owner.ctx)
            || !h.view.as_ref().is_some_and(|v| button.same_view(v))
        {
            return;
        }
        let owner = h.owner.clone();
        let fixture = h.fixture;
        s.app_home
            .attempts
            .retain(|a| a.owner.ctx.epoch == owner.ctx.epoch);
        if s.app_home.attempts.len() >= 64
            || s.app_home.attempts.iter().any(|a| {
                a.owner.ctx.session.team_id == owner.ctx.session.team_id && a.button == button
            })
        {
            return;
        }
        if fixture {
            if crate::fixture::is_fixture() {
                fixture::activate(&mut s, &button)
            }
            return;
        }
        let Some(token) = crate::app_modal::register_action_with_context(
            &mut s,
            &button_view_app(&button),
            None,
            None,
        ) else {
            return;
        };
        let v = s.app_home.surface.as_ref().unwrap().view.as_ref().unwrap();
        let request = match button.prepare(&owner.ctx.client, &owner.ctx.session, v, &token.token) {
            Ok(r) => r,
            Err(_) => {
                crate::app_modal::reject_action(&mut s, &token);
                return;
            }
        };
        s.app_home
            .correlations
            .push((token.token.clone(), owner.clone()));
        if s.app_home.correlations.len() > 10 {
            s.app_home.correlations.remove(0);
        }
        let id = next();
        s.app_home.attempts.push(Attempt {
            id,
            owner: owner.clone(),
            button,
            phase: Phase::Pending,
        });
        (owner, id, token, request)
    };
    dioxus::core::spawn_forever(async move {
        let (owner, id, token, request) = prepared;
        if !dispatch_current(&state.peek(), id, key) {
            let mut s = state.write();
            s.app_home.attempts.retain(|a| a.id != id);
            crate::app_modal::reject_action(&mut s, &token);
            return;
        }
        let result = owner
            .ctx
            .transport
            .execute_block_interaction(request)
            .await
            .and_then(|r| api::decode_block_action_response(&r));
        let mut s = state.write();
        if let Err(e) = &result {
            failure(&mut s, &owner.ctx, e)
        }
        if !crate::message_buttons::inputs::user::directory::owner_current(&s, &owner.ctx) {
            if let Some(a) = s.app_home.attempts.iter_mut().find(|a| a.id == id) {
                a.phase = Phase::Uncertain;
            }
            return;
        }
        let unsent = matches!(&result, Err(Error::RateLimited { .. }))
            || matches!(&result,Err(Error::Api(e)) if e=="block_interaction_not_sent_offline");
        match &result {
            Ok(api::BlockActionResponse::Accepted) => s.app_home.attempts.retain(|a| a.id != id),
            Ok(api::BlockActionResponse::Rejected(_)) => {
                s.app_home.attempts.retain(|a| a.id != id);
                crate::app_modal::reject_action(&mut s, &token)
            }
            Err(_) if unsent => {
                s.app_home.attempts.retain(|a| a.id != id);
                crate::app_modal::reject_action(&mut s, &token)
            }
            Err(_) => {
                if let Some(a) = s.app_home.attempts.iter_mut().find(|a| a.id == id) {
                    a.phase = Phase::Uncertain
                }
            }
        }
        if owner.current(&s) {
            s.app_home.surface.as_mut().unwrap().notice=Some(match result{Ok(api::BlockActionResponse::Accepted)=>"Action received by the app. A form or updated Home content may follow.",Ok(api::BlockActionResponse::Rejected(_))=>"The app declined this action.",Err(_) if unsent=>"The action was not sent. Wait for connectivity or the rate limit before trying again.",Err(_)=>"The app has not confirmed this action. It will not be sent again."}.into())
        }
    });
    // Same user-requested URL order as the existing ordinary app button path.
    if let Some(url) = url {
        let allowed = crate::slack_links::button_url_allowed(&state.peek(), &url);
        match allowed {
            Ok(()) => crate::slack_links::open(state, &url),
            Err(reason) => state.write().show_toast(reason),
        }
    }
}
fn button_view_app(b: &Button) -> String {
    b.app_id().into()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn queued_action_rechecks_exact_view_owner_overlay_and_delivery_state() {
        let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
        fixture::install(&mut s);
        let h = s.app_home.surface.as_ref().unwrap();
        let key = Key {
            visit: h.id,
            revision: h.revision,
        };
        let owner = h.owner.clone();
        let original_view = h.view.clone().unwrap();
        let button = h
            .view
            .as_ref()
            .unwrap()
            .button("entry", "review", &owner.ctx.session.user_id)
            .unwrap();
        s.app_home.attempts.push(Attempt {
            id: 99,
            owner,
            button,
            phase: Phase::Pending,
        });
        assert!(dispatch_current(&s, 99, key));
        s.overlay = Some(Overlay::AppModal);
        assert!(!dispatch_current(&s, 99, key));
        s.overlay = None;
        s.app_home.attempts[0].phase = Phase::Uncertain;
        assert!(!dispatch_current(&s, 99, key));
        s.app_home.attempts[0].phase = Phase::Pending;
        let h = s.app_home.surface.as_mut().unwrap();
        let mut raw = h.view.as_ref().unwrap().raw().clone();
        raw["hash"] = serde_json::json!("changed");
        h.view = Some(View::decode(h.home.as_ref().unwrap(), raw).unwrap());
        assert!(!dispatch_current(&s, 99, key));
        s.app_home.surface.as_mut().unwrap().view = Some(original_view);
        assert!(dispatch_current(&s, 99, key));
        s.channel_generation += 1;
        assert!(!dispatch_current(&s, 99, key));
    }
}
