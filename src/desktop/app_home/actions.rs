use super::*;
use inputs::text::button as text_button;
use std::future::Future;
use super_platinum_core::block_actions::BlockElementMetadata;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Pending,
    Uncertain,
    Completed,
}
pub(super) struct Attempt {
    id: u64,
    owner: Owner,
    button: Button,
    route: context::Witness,
    phase: Phase,
    repeatable: bool,
    processing_until: Option<Instant>,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct Key {
    pub visit: u64,
    pub revision: u64,
}
fn capture(
    s: &ShellState,
    key: Key,
    m: &BlockElementMetadata,
) -> Option<(Owner, Button, context::Witness)> {
    if workflows::locked(s)
        || m.source != super_platinum_core::block_actions::BlockElementSource::Button
    {
        return None;
    }
    let h = s.app_home.surface.as_ref()?;
    if !h.owner.current(s)
        || h.id != key.visit
        || h.revision != key.revision
        || s.overlay.is_some()
        || h.loading
        || h.confirmation.is_some()
        || !inputs::ready(s)
        || h.calendar_confirmation.is_some()
        || cooling(s, &h.owner.ctx)
    {
        return None;
    }
    let view = h.view.as_ref()?;
    let b = if let Some(form) = inputs::form(s).filter(|f| !f.fields.is_empty()) {
        view.button_with_form(
            m.block_id.as_deref()?,
            m.action_id.as_deref()?,
            &h.owner.ctx.session.user_id,
            form,
        )
        .ok()?
    } else {
        view.button(
            m.block_id.as_deref()?,
            m.action_id.as_deref()?,
            &h.owner.ctx.session.user_id,
        )
        .ok()?
    };
    // Raw metadata remains the authority; DOM only names its owned key.
    if b.raw().get("value").and_then(serde_json::Value::as_str) != m.value.as_deref() {
        return None;
    }
    if s.app_home.attempts.iter().any(|a| {
        a.owner.ctx.epoch == h.owner.ctx.epoch
            && a.owner.ctx.session.team_id == h.owner.ctx.session.team_id
            && a.button.same_action(&b)
            && a.blocks_click(Instant::now())
    }) {
        return None;
    }
    Some((h.owner.clone(), b, context::Witness::capture(s).ok()?))
}
pub(crate) fn enabled(s: &ShellState, key: Key, m: &BlockElementMetadata) -> bool {
    if workflows::enabled(s, key, m) {
        return true;
    }
    if workflows::locked(s) {
        return false;
    }
    if calendar::enabled(s, key, m) {
        return true;
    }
    capture(s, key, m).is_some()
        && s.app_home
            .attempts
            .iter()
            .filter(|a| a.owner.ctx.epoch == s.core.account_epoch)
            .count()
            < 64
}
pub(crate) fn click(mut state: Signal<ShellState>, key: Key, m: BlockElementMetadata) {
    if workflows::click(state, key, &m) {
        return;
    }
    if workflows::locked(&state.peek()) {
        return;
    }
    if calendar::click(state, key, &m) {
        return;
    }
    let Some((_, button, route)) = capture(&state.peek(), key, &m) else {
        return;
    };
    if button.raw().get("confirm").is_some() {
        state
            .write()
            .app_home
            .surface
            .as_mut()
            .unwrap()
            .confirmation = Some((button, route))
    } else {
        launch(state, key, button, route)
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
    if let Some((b, route)) = b {
        launch(state, key, b, route)
    }
}
// Called immediately before the once-only transport invocation. A queued action
// cannot outlive the exact raw view or an intervening surface/overlay change.
fn dispatch_current(s: &ShellState, id: u64, key: Key) -> bool {
    s.app_home
        .attempts
        .iter()
        .find(|a| a.id == id)
        .is_some_and(|a| {
            a.phase == Phase::Pending && final_current(s, &a.owner, key, &a.button, &a.route)
        })
}
/// Shared final admission after the actual native-control lease. Fixture
/// activation replaces transport only; it uses this same current-state gate.
fn final_current(
    s: &ShellState,
    owner: &Owner,
    key: Key,
    button: &Button,
    route: &context::Witness,
) -> bool {
    let Some(h) = s.app_home.surface.as_ref() else {
        return false;
    };
    owner.current(s)
        && route.current(s)
        && h.id == key.visit
        && h.revision == key.revision
        && !h.loading
        && h.confirmation.is_none()
        && h.calendar_confirmation.is_none()
        && s.overlay.is_none()
        && !cooling(s, &owner.ctx)
        && h.view.as_ref().is_some_and(|v| button.same_view(v))
        && inputs::form(s).is_none_or(|form| button.same_form(form))
}

fn launch(mut state: Signal<ShellState>, key: Key, button: Button, route: context::Witness) {
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
            || !route.current(&s)
            || h.id != key.visit
            || h.revision != key.revision
            || s.overlay.is_some()
            || h.loading
            || cooling(&s, &h.owner.ctx)
            || !h.view.as_ref().is_some_and(|v| button.same_view(v))
            || inputs::form(&s).is_some_and(|form| !button.same_form(form))
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
                a.owner.ctx.session.team_id == owner.ctx.session.team_id
                    && a.button.same_action(&button)
                    && a.blocks_click(Instant::now())
            })
        {
            return;
        }
        if fixture && !text_button::fixture::intercepts() {
            drop(s);
            dioxus::core::spawn_forever(async move {
                if let Some(lease) = text_button::lock(state, key).await {
                    context::fixture::after_native_lock(state).await;
                    let mut s = state.write();
                    let admitted = crate::fixture::is_fixture()
                        && final_current(&s, &owner, key, &button, &route)
                        && lease.current(&s, key);
                    if admitted {
                        fixture::activate_with_context(&mut s, &button, route.team_id());
                    }
                    context::fixture::action_finished(&mut s, admitted);
                    lease.release();
                }
            });
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
        let request = match button.prepare_with_context(
            &owner.ctx.client,
            &owner.ctx.session,
            v,
            &token.token,
            route.team_id(),
        ) {
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
        let repeatable = ordinary(&button);
        s.app_home.attempts.push(Attempt {
            repeatable,
            processing_until: repeatable.then(|| Instant::now() + Duration::from_secs(1)),
            id,
            owner: owner.clone(),
            button,
            route,
            phase: Phase::Pending,
        });
        (owner, id, token, request)
    };
    let processing_id = prepared.1;
    // One finite wake per accepted click. HTTP completion neither shortens nor
    // extends the retained ordinary Button's local processing window.
    if state
        .peek()
        .app_home
        .attempts
        .iter()
        .any(|a| a.id == processing_id && a.repeatable)
    {
        dioxus::core::spawn_forever(async move {
            tokio::time::sleep(Duration::from_secs(1)).await;
            expire_processing(&mut state.write(), processing_id, Instant::now());
        });
    }
    dioxus::core::spawn_forever(async move {
        let (owner, id, token, request) = prepared;
        let Some(native_lease) = text_button::lock(state, key).await else {
            let mut s = state.write();
            complete(&mut s, id);
            crate::app_modal::reject_action(&mut s, &token);
            s.app_home.correlations.retain(|(t, _)| t != &token.token);
            text_button::fixture::refused(&mut s, id, "native-custody");
            return;
        };
        context::fixture::after_native_lock(state).await;
        text_button::fixture::after_native_lock(state).await;
        if !native_lease.current(&state.peek(), key) || !dispatch_current(&state.peek(), id, key) {
            native_lease.release();
            let mut s = state.write();
            complete(&mut s, id);
            crate::app_modal::reject_action(&mut s, &token);
            s.app_home.correlations.retain(|(t, _)| t != &token.token);
            text_button::fixture::refused(&mut s, id, "final-current");
            return;
        }
        // The URL side effect shares the exact final native admission.
        if let Some(url) = url {
            let allowed = crate::slack_links::button_url_allowed(&state.peek(), &url);
            match allowed {
                Ok(()) => crate::slack_links::open(state, &url),
                Err(reason) => state.write().show_toast(reason),
            }
        }
        let block_activity = {
            let mut s = state.write();
            let action = s
                .app_home
                .attempts
                .iter()
                .find(|a| a.id == id)
                .map(|a| (a.button.clone(), a.route.clone()));
            action.and_then(|(button, route)| {
                inputs::static_select::wrappers::button_started(&mut s, &button, &route)
                    .map(|binding| (binding, route))
            })
        };
        let intercepted = text_button::fixture::request(&mut state.write(), id, key, &request);
        let result = if let Some(reply) = intercepted {
            native_lease.release();
            reply
                .await
                .unwrap_or_else(|_| Err(Error::Api("block_interaction_not_sent_offline".into())))
        } else {
            let future = owner.ctx.transport.execute_block_interaction(request);
            tokio::pin!(future);
            let mut released = false;
            std::future::poll_fn(|cx| {
                let result = future.as_mut().poll(cx);
                if native_lease.short() && !released {
                    native_lease.release();
                    released = true;
                }
                result
            })
            .await
        }
        .and_then(|r| api::decode_block_action_response(&r));
        native_lease.release();
        let mut s = state.write();
        text_button::fixture::response(&mut s, id, &result);
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
            Ok(api::BlockActionResponse::Accepted) => complete(&mut s, id),
            Ok(api::BlockActionResponse::Rejected(_)) => {
                complete(&mut s, id);
                crate::app_modal::reject_action(&mut s, &token)
            }
            Err(_) if unsent => {
                complete(&mut s, id);
                crate::app_modal::reject_action(&mut s, &token)
            }
            Err(_) => {
                if let Some(a) = s.app_home.attempts.iter_mut().find(|a| a.id == id) {
                    a.phase = Phase::Uncertain
                }
            }
        }
        if let Some((binding, route)) = block_activity {
            let error = match &result {
                Ok(api::BlockActionResponse::Accepted) => None,
                Ok(api::BlockActionResponse::Rejected(_)) => {
                    Some("The app declined this action.".to_owned())
                }
                Err(_) => Some(
                    "The app has not confirmed this action. It will not be replayed automatically."
                        .to_owned(),
                ),
            };
            inputs::static_select::wrappers::button_finished(&mut s, binding, &route, error);
        }
        if owner.current(&s) {
            s.app_home.surface.as_mut().unwrap().notice=Some(match result{Ok(api::BlockActionResponse::Accepted)=>"Action received by the app. A form or updated Home content may follow.",Ok(api::BlockActionResponse::Rejected(_))=>"The app declined this action.",Err(_) if unsent=>"The action was not sent. Wait for connectivity or the rate limit before trying again.",Err(_)=>"The app has not confirmed this action. It will not be sent again."}.into())
        }
    });
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
        let route = context::Witness::capture(&s).unwrap();
        s.app_home.attempts.push(Attempt {
            id: 99,
            repeatable: true,
            processing_until: Some(Instant::now() + Duration::from_secs(1)),
            owner,
            button,
            route,
            phase: Phase::Pending,
        });
        assert!(dispatch_current(&s, 99, key));
        let flags =
            std::mem::take(&mut s.core.session.as_mut().unwrap().unified_user_client_enabled);
        assert!(!dispatch_current(&s, 99, key));
        s.core.session.as_mut().unwrap().unified_user_client_enabled = flags;
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

pub(crate) fn text_button_fixture_attempts(s: &ShellState) -> serde_json::Value {
    if !text_button::fixture::intercepts() {
        return serde_json::Value::Null;
    }
    serde_json::json!(s.app_home.attempts.iter().filter(|a|a.phase!=Phase::Completed).map(|a|serde_json::json!({"id":a.id,"phase":format!("{:?}",a.phase),"ownerCurrent":a.owner.current(s),"routeCurrent":a.route.current(s),"raw":a.button.raw()})).collect::<Vec<_>>())
}

fn ordinary(button: &Button) -> bool {
    button.raw().get("confirm").is_none() && button.raw().get("url").is_none()
}
impl Attempt {
    fn blocks_click(&self, now: Instant) -> bool {
        !self.repeatable || self.processing_until.is_some_and(|until| now < until)
    }
}
fn complete(s: &mut ShellState, id: u64) {
    if let Some(a) = s.app_home.attempts.iter_mut().find(|a| a.id == id) {
        if a.repeatable && a.blocks_click(Instant::now()) {
            a.phase = Phase::Completed;
            return;
        }
    }
    s.app_home.attempts.retain(|a| a.id != id);
}
fn expire_processing(s: &mut ShellState, id: u64, now: Instant) {
    if let Some(a) = s.app_home.attempts.iter_mut().find(|a| a.id == id) {
        if a.repeatable && a.processing_until.is_some_and(|until| now >= until) {
            a.processing_until = None;
        }
    }
    s.app_home
        .attempts
        .retain(|a| a.id != id || a.phase != Phase::Completed || a.processing_until.is_some());
}
pub(crate) fn wrappers_fixture_processing(s: &ShellState) -> serde_json::Value {
    serde_json::json!(s.app_home.attempts.iter().map(|a|serde_json::json!({"id":a.id,"phase":format!("{:?}",a.phase),"repeatable":a.repeatable,"processing":a.processing_until.is_some(),"ownerCurrent":a.owner.current(s),"routeCurrent":a.route.current(s),"raw":a.button.raw()})).collect::<Vec<_>>())
}
#[cfg(test)]
#[path = "actions/processing_tests.rs"]
mod processing_tests;
