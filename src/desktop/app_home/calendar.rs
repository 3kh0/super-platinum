//! Calendar Home entries keep their original view/input owner through confirmation.
use super::*;
use actions::Key;
use super_platinum_core::block_actions::{
    BlockElementMetadata, BlockElementSource,
    home::calendar::{Entry, Operation},
    shortcuts::calendar as cal,
};

#[derive(Clone)]
pub(crate) struct Origin {
    owner: Owner,
    key: Key,
    entry: Entry,
    input_revision: Option<u64>,
}
impl Origin {
    pub(crate) fn current(&self, s: &ShellState) -> bool {
        self.owner.current(s)
            && inputs::ready(s)
            && inputs::revision(s) == self.input_revision
            && s.app_home.surface.as_ref().is_some_and(|h| {
                h.id == self.key.visit
                    && h.revision == self.key.revision
                    && !h.loading
                    && h.view
                        .as_ref()
                        .is_some_and(|v| self.entry.same_view(v, &self.owner.ctx.session.user_id))
            })
    }
    pub(crate) fn context(&self) -> Arc<Context> {
        self.owner.ctx.clone()
    }
    pub(crate) fn creation(&self) -> Option<(cal::Provider, Option<String>)> {
        match self.entry.operation() {
            Operation::Create { provider, preset } => Some((*provider, preset.clone())),
            _ => None,
        }
    }
}
#[derive(Clone)]
enum Phase {
    Queued,
    Sent,
    Ready(String),
    Uncertain,
    Rejected,
    Unsent,
}
pub(super) struct Attempt {
    id: u64,
    origin: Origin,
    phase: Phase,
    cancelled: bool,
    native_lease: Option<inputs::native::Release>,
}
fn capture(s: &ShellState, key: Key, m: &BlockElementMetadata) -> Option<Origin> {
    if m.source != BlockElementSource::Button || s.overlay.is_some() || !inputs::ready(s) {
        return None;
    }
    let h = s.app_home.surface.as_ref()?;
    if !h.owner.current(s)
        || h.id != key.visit
        || h.revision != key.revision
        || h.loading
        || h.fixture
        || h.confirmation.is_some()
        || h.calendar_confirmation.is_some()
        || cooling(s, &h.owner.ctx)
    {
        return None;
    }
    let entry = Entry::capture(
        h.view.as_ref()?,
        m.block_id.as_deref()?,
        m.action_id.as_deref()?,
        &h.owner.ctx.session.user_id,
        &cal::time::today(),
    )
    .ok()?;
    if entry.raw().get("value").and_then(serde_json::Value::as_str) != m.value.as_deref() {
        return None;
    }
    Some(Origin {
        owner: h.owner.clone(),
        key,
        entry,
        input_revision: inputs::revision(s),
    })
}
fn same_operation(a: &Attempt, origin: &Origin) -> bool {
    a.origin.owner.ctx.epoch == origin.owner.ctx.epoch
        && a.origin.owner.ctx.session.team_id == origin.owner.ctx.session.team_id
        && a.origin.entry.operation() == origin.entry.operation()
}
fn available(s: &ShellState, origin: &Origin) -> bool {
    if matches!(origin.entry.operation(), Operation::Create { .. }) {
        return true;
    }
    if s.app_home.calendar_attempts.iter().any(|a| {
        same_operation(a, origin)
            && matches!(a.phase, Phase::Queued | Phase::Sent | Phase::Uncertain)
    }) {
        return false;
    }
    s.app_home
        .calendar_attempts
        .iter()
        .filter(|a| {
            a.origin.owner.ctx.epoch == s.core.account_epoch
                && matches!(a.phase, Phase::Queued | Phase::Sent | Phase::Uncertain)
        })
        .count()
        < 64
}
pub(super) fn enabled(s: &ShellState, key: Key, m: &BlockElementMetadata) -> bool {
    capture(s, key, m).is_some_and(|o| available(s, &o))
}
/// True means this is a captured Calendar action, even if now refused. It must
/// never fall through to ordinary blocks.actions as a different operation.
pub(super) fn click(mut state: Signal<ShellState>, key: Key, m: &BlockElementMetadata) -> bool {
    let origin = capture(&state.peek(), key, m);
    let Some(origin) = origin else {
        return false;
    };
    if !available(&state.peek(), &origin) {
        return true;
    }
    if origin.entry.raw().get("confirm").is_some() {
        state
            .write()
            .app_home
            .surface
            .as_mut()
            .unwrap()
            .calendar_confirmation = Some(origin);
    } else {
        launch(state, origin);
    }
    true
}
pub(super) fn cancel(mut state: Signal<ShellState>, key: Key) {
    if let Some(h) = state
        .write()
        .app_home
        .surface
        .as_mut()
        .filter(|h| h.id == key.visit && h.revision == key.revision)
    {
        h.calendar_confirmation = None;
    }
}
pub(super) fn confirm(mut state: Signal<ShellState>, key: Key) {
    let origin = state
        .write()
        .app_home
        .surface
        .as_mut()
        .filter(|h| h.id == key.visit && h.revision == key.revision)
        .and_then(|h| h.calendar_confirmation.take());
    if let Some(origin) = origin {
        launch(state, origin);
    }
}
pub(super) fn confirmation(h: &Surface) -> Option<serde_json::Value> {
    h.calendar_confirmation
        .as_ref()?
        .entry
        .raw()
        .get("confirm")
        .cloned()
}
fn launch(mut state: Signal<ShellState>, origin: Origin) {
    if crate::fixture::is_fixture() {
        return;
    }
    {
        let s = state.peek();
        if !origin.current(&s)
            || s.overlay.is_some()
            || cooling(&s, &origin.owner.ctx)
            || !available(&s, &origin)
        {
            return;
        }
    }
    if origin.creation().is_some() {
        dioxus::core::spawn_forever(async move {
            let Some(lease) = inputs::native::lock_owned(state, origin.key).await else {
                return;
            };
            let current = {
                let s = state.peek();
                origin.current(&s) && s.overlay.is_none() && !cooling(&s, &origin.owner.ctx)
            };
            if current {
                crate::message_buttons::shortcuts::open_calendar_home(state, origin);
            }
            lease.release();
        });
        return;
    }
    let request = match origin
        .entry
        .connect_request(&origin.owner.ctx.client, &origin.owner.ctx.session)
    {
        Ok(r) => r,
        Err(_) => return,
    };
    let id = next();
    {
        let mut s = state.write();
        s.app_home.calendar_attempts.retain(|a| {
            a.origin.owner.ctx.epoch == origin.owner.ctx.epoch
                && matches!(a.phase, Phase::Queued | Phase::Sent | Phase::Uncertain)
        });
        s.app_home.calendar_attempts.push(Attempt {
            id,
            origin: origin.clone(),
            phase: Phase::Queued,
            cancelled: false,
            native_lease: None,
        });
    }
    dioxus::core::spawn_forever(async move {
        let Some(lease) = inputs::native::lock_owned(state, origin.key).await else {
            if let Some(a) = state
                .write()
                .app_home
                .calendar_attempts
                .iter_mut()
                .find(|a| a.id == id && matches!(a.phase, Phase::Queued))
            {
                a.phase = Phase::Unsent;
            }
            return;
        };
        if !claim(&mut state.write(), id) {
            lease.release();
            return;
        }
        if let Some(a) = state
            .write()
            .app_home
            .calendar_attempts
            .iter_mut()
            .find(|a| a.id == id)
        {
            a.native_lease = Some(lease.clone());
        }
        let result = origin
            .owner
            .ctx
            .transport
            .execute_block_interaction(request)
            .await;
        lease.release();
        let mut s = state.write();
        let Some(phase) = settle(&mut s, id, &result) else {
            return;
        };
        let notice = match phase {
            Phase::Ready(url) => {
                // No state borrow is held while asking the OS to open the URL.
                drop(s);
                let opened = crate::media::open_external(&url).is_ok();
                let mut s = state.write();
                if origin.current(&s) {
                    s.app_home.surface.as_mut().unwrap().notice=Some(if opened{"Finish connecting in the browser, then refresh Home."}else{"The connection page could not be opened. Select Connect to request a new connection page."}.into());
                }
                return;
            }
            Phase::Uncertain => {
                "The connection request could not be confirmed. It will not be repeated automatically."
            }
            Phase::Unsent => {
                "The connection request was not sent. Wait for the connection or rate limit before trying again."
            }
            _ => "The app declined the connection request.",
        };
        s.app_home.surface.as_mut().unwrap().notice = Some(notice.into());
    });
}
// The once worker and tests use the same admission/settlement boundary.
fn claim(s: &mut ShellState, id: u64) -> bool {
    let Some(origin) = s
        .app_home
        .calendar_attempts
        .iter()
        .find(|a| a.id == id && !a.cancelled && matches!(a.phase, Phase::Queued))
        .map(|a| a.origin.clone())
    else {
        return false;
    };
    let admitted = origin.current(s) && s.overlay.is_none() && !cooling(s, &origin.owner.ctx);
    let a = s
        .app_home
        .calendar_attempts
        .iter_mut()
        .find(|a| a.id == id)
        .unwrap();
    a.phase = if admitted { Phase::Sent } else { Phase::Unsent };
    admitted
}
fn settle(s: &mut ShellState, id: u64, result: &Result<serde_json::Value, Error>) -> Option<Phase> {
    let origin = s
        .app_home
        .calendar_attempts
        .iter()
        .find(|a| a.id == id && matches!(a.phase, Phase::Sent))?
        .origin
        .clone();
    if !crate::message_buttons::inputs::user::directory::owner_current(s, &origin.owner.ctx) {
        return None;
    }
    let phase = match result {
        Ok(raw) => match home::calendar::authorization_url(raw) {
            Ok(u) => Phase::Ready(u),
            Err(_) => {
                if raw.get("ok") == Some(&serde_json::Value::Bool(false))
                    && raw
                        .get("error")
                        .and_then(serde_json::Value::as_str)
                        .is_some_and(|e| !e.is_empty() && e != "dispatch_failed")
                {
                    Phase::Rejected
                } else {
                    Phase::Uncertain
                }
            }
        },
        Err(Error::RateLimited { .. }) => Phase::Unsent,
        Err(Error::Api(code)) if code == "block_interaction_not_sent_offline" => Phase::Unsent,
        Err(_) => Phase::Uncertain,
    };
    let current = origin.current(s) && s.overlay.is_none();
    let a = s
        .app_home
        .calendar_attempts
        .iter_mut()
        .find(|a| a.id == id)
        .unwrap();
    let adopt = current && !a.cancelled;
    a.phase = phase.clone();
    a.native_lease = None;
    if let Err(Error::RateLimited { retry_after_secs }) = result {
        navigation::retain_limit(s, &origin.owner.ctx, *retry_after_secs);
    }
    adopt.then_some(phase)
}
fn cancel_pending(s: &mut ShellState, id: u64, key: Key) {
    let current = s
        .app_home
        .calendar_attempts
        .iter()
        .find(|a| a.id == id)
        .is_some_and(|a| {
            // Dismissal releases only this attempt. A newer local draft may
            // revoke dispatch/adoption without revoking its visible Cancel.
            a.origin.key == key
                && a.origin.owner.current(s)
                && s.app_home
                    .surface
                    .as_ref()
                    .is_some_and(|h| h.id == key.visit && h.revision == key.revision)
        });
    if !current {
        return;
    }
    if let Some(a) = s.app_home.calendar_attempts.iter_mut().find(|a| a.id == id) {
        if let Some(lease) = a.native_lease.take() {
            lease.release();
        }
        a.cancelled = true;
        if matches!(a.phase, Phase::Queued) {
            a.phase = Phase::Unsent;
        }
    }
}
pub(super) fn retire_ui(s: &mut ShellState) {
    for a in &mut s.app_home.calendar_attempts {
        if let Some(lease) = a.native_lease.take() {
            lease.release();
        }
        a.cancelled = true;
        if matches!(a.phase, Phase::Queued) {
            a.phase = Phase::Unsent;
        }
    }
}
pub(super) fn status(mut state: Signal<ShellState>, key: Key) -> Element {
    let id = state
        .read()
        .app_home
        .calendar_attempts
        .iter()
        .find(|a| {
            a.origin.key == key && !a.cancelled && matches!(a.phase, Phase::Queued | Phase::Sent)
        })
        .map(|a| a.id);
    rsx! {if let Some(id)=id {div{role:"status",class:"app-home-notice","Opening the connection page…" button{onclick:move |_|cancel_pending(&mut state.write(),id,key),"Cancel"}}}}
}

#[cfg(test)]
mod tests;
