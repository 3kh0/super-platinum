//! Canvas-owned, read-only attribution. Never rewrites the original root or
//! borrows a Profile/Home query permit. Missing raw members are unavailable;
//! this slice does not invent an unknown-member network request.
use super::{Owner, Query, State as Comments, Ticket};
use crate::state::ShellState;
use dioxus::prelude::*;
use serde_json::Value;
use std::{sync::Arc, time::Instant};
use super_platinum_core::{
    MediaAssetId,
    canvas::comments::attribution as source,
    slack::{Error, models::Message},
};
mod acquire;
mod member;
mod recovery;
#[cfg(test)]
mod tests;
pub(in crate::canvas) use source::Name;

const UNAVAILABLE: &str = "Editor details could not be loaded. Retry editor details.";
const CHANGED: &str = "Editor details changed. Retry editor details.";

#[derive(Default)]
pub(super) struct State {
    token: Option<Arc<()>>,
    loading: bool,
    error: Option<&'static str>,
    retry_at: Option<Instant>,
    blocked: bool,
    observed: Option<Arc<acquire::Observation>>,
}

pub(in crate::canvas) enum Presentation {
    Hidden,
    Pending,
    Unavailable {
        notice: &'static str,
        retryable: bool,
    },
    Ready {
        id: String,
        name: Name,
        avatar: Option<MediaAssetId>,
        initials: String,
        interaction: Option<Interaction>,
    },
}

#[derive(Clone)]
pub(in crate::canvas) struct Interaction(Arc<acquire::Observation>);
/// Once an exact current attribution admits opening a profile, that pane's
/// existing ProfileVisit owns same-user hydration. This is only visibility
/// custody; it cannot authorize another hover/open or a comment action.
#[derive(Clone)]
pub(in crate::canvas) struct Navigation(Arc<acquire::Observation>);
impl Navigation {
    pub(in crate::canvas) fn current(&self, shell: &ShellState, model: &Comments) -> bool {
        shell.profile_user.as_deref() == Some(self.0.member.id.as_str())
            && self.0.navigation_current(shell, model)
    }
}
impl Interaction {
    pub(in crate::canvas) fn current(&self, shell: &ShellState, model: &Comments) -> bool {
        self.0.current(shell, model)
    }
    pub(in crate::canvas) fn id(&self) -> &str {
        &self.0.member.id
    }
    pub(in crate::canvas) fn message_ts(&self) -> &str {
        self.0.root.ts.as_deref().unwrap_or_default()
    }
    pub(in crate::canvas) fn navigation(
        &self,
        shell: &ShellState,
        model: &Comments,
    ) -> Option<Navigation> {
        self.current(shell, model)
            .then(|| Navigation(self.0.clone()))
    }
}

/// The shared accepted preference-event revision is merely an invalidation
/// witness. It supplies no workflow policy or workflow owner authority.
pub(crate) fn observes_preferences(shell: &ShellState, team: &str, generation: u64) -> bool {
    Owner::capture(shell).is_some_and(|owner| {
        owner.scope.team == team
            && shell
                .core
                .workspaces
                .get(team)
                .is_some_and(|w| w.rt_generation == generation)
    })
}

fn context(model: &Comments) -> source::Context {
    if matches!(model.query, Some(Query::Replies(_))) {
        source::Context::Thread
    } else {
        source::Context::Stream
    }
}

pub(in crate::canvas) fn presentation(
    shell: &ShellState,
    model: &Comments,
    message: &Message,
) -> Presentation {
    let selected = match source::editor(message, context(model), false) {
        Ok(None) => return Presentation::Hidden,
        Err(_) => return unavailable_presentation(false),
        Ok(Some(value)) => value,
    };
    let value = &model.attribution;
    if value.loading {
        return Presentation::Pending;
    }
    let Some(observed) = value.observed.as_ref() else {
        return Presentation::Unavailable {
            notice: value.error.unwrap_or(UNAVAILABLE),
            retryable: !model.loading
                && !value.blocked
                && value
                    .retry_at
                    .is_none_or(|deadline| Instant::now() >= deadline),
        };
    };
    if !observed.current(shell, model)
        || selected.id() != observed.member.id
        || !same_message(message, &observed.root)
    {
        return Presentation::Unavailable {
            notice: CHANGED,
            retryable: !model.loading,
        };
    }
    let Ok(name) = selected.name(&observed.member.raw, &observed.policy, "Name hidden") else {
        return unavailable_presentation(true);
    };
    let known = matches!(name, Name::Known(_));
    let avatar = known
        .then(|| {
            super_platinum_core::state::user_profile_image_url(&observed.member.native)
                .map(|url| shell.media.register_avatar(selected.id(), &url))
        })
        .flatten();
    let initials = match &name {
        Name::Known(name) => name
            .split_whitespace()
            .take(2)
            .filter_map(|word| word.chars().next())
            .collect::<String>(),
        _ => String::new(),
    };
    Presentation::Ready {
        id: selected.id().into(),
        name,
        avatar,
        initials,
        interaction: known.then(|| Interaction(observed.clone())),
    }
}
fn unavailable_presentation(retryable: bool) -> Presentation {
    Presentation::Unavailable {
        notice: UNAVAILABLE,
        retryable,
    }
}
fn same_message(a: &Message, b: &Message) -> bool {
    serde_json::to_value(a)
        .ok()
        .zip(serde_json::to_value(b).ok())
        .is_some_and(|(a, b)| a == b)
}
fn unavailable() -> Error {
    Error::Api("canvas_editor_attribution_unavailable".into())
}

pub(super) fn after_read(state: Signal<ShellState>, model: Signal<Comments>, ticket: &Ticket) {
    if model.peek().accepts(&state.peek(), ticket)
        && !model.peek().loading
        && model.peek().error.is_none()
    {
        start(state, model, false);
    }
}

/// Explicit retry touches only attribution state, never the reply draft, page,
/// read serial, original root, selected control or mutation state.
pub(in crate::canvas) fn retry(state: Signal<ShellState>, model: Signal<Comments>) {
    start(state, model, true);
}

fn start(state: Signal<ShellState>, mut model: Signal<Comments>, explicit: bool) {
    let (ticket, root, token) = {
        let shell = state.peek();
        let mut value = model.write();
        if value.loading
            || value.attribution.loading
            || value.attribution.blocked
            || value
                .attribution
                .retry_at
                .is_some_and(|t| Instant::now() < t)
        {
            return;
        }
        let Some(owner) = value.owner.clone().filter(|owner| owner.current(&shell)) else {
            return;
        };
        let Some(Query::Replies(ts)) = value.query.clone() else {
            return;
        };
        let Some(root) = value
            .root
            .clone()
            .filter(|root| root.ts.as_deref() == Some(ts.as_str()))
        else {
            return;
        };
        match source::editor(&root, source::Context::Thread, false) {
            Ok(Some(_)) => {}
            Ok(None) => return,
            Err(_) => {
                value.attribution.error = Some(UNAVAILABLE);
                return;
            }
        }
        let token = Arc::new(());
        value.attribution = State {
            token: Some(token.clone()),
            loading: true,
            ..Default::default()
        };
        (
            Ticket {
                owner,
                serial: value.serial,
                query: Query::Replies(ts),
                cursor: None,
                direction: super::Direction::First,
            },
            root,
            token,
        )
    };
    spawn(async move {
        let result = if explicit {
            recovery::capture(state, model, ticket.clone(), root, token.clone()).await
        } else {
            acquire::capture(state, model, ticket.clone(), root, token.clone())
                .await
                .map_err(recovery::Failure::policy)
        };
        if !model
            .try_read()
            .is_ok_and(|model| slot_current(&state.peek(), &model, &ticket, &token))
        {
            return;
        }
        let wait = result.as_ref().err().and_then(|failure| failure.retry_at);
        {
            let mut value = model.write();
            value.attribution.loading = false;
            match result {
                Ok(observed) => value.attribution.observed = Some(Arc::new(observed)),
                Err(failure) => {
                    let limited = matches!(failure.error, Error::RateLimited { .. });
                    value.attribution.error = Some(if limited {
                        "Slack asked us to wait before loading editor details."
                    } else {
                        UNAVAILABLE
                    });
                    value.attribution.retry_at = wait;
                    value.attribution.blocked = failure.blocked;
                }
            }
        }
        if let Some(deadline) = wait {
            tokio::time::sleep(deadline.saturating_duration_since(Instant::now())).await;
            if model
                .try_read()
                .is_ok_and(|model| slot_current(&state.peek(), &model, &ticket, &token))
            {
                model.write().attribution.retry_at = None;
            }
        }
    });
}
fn slot_current(shell: &ShellState, model: &Comments, ticket: &Ticket, token: &Arc<()>) -> bool {
    model.accepts(shell, ticket)
        && model
            .attribution
            .token
            .as_ref()
            .is_some_and(|value| Arc::ptr_eq(value, token))
}

/// Passive fixture/checker evidence contains no credentials or alternate policy.
pub(in crate::canvas) fn snapshot(shell: &ShellState, model: &Comments) -> Value {
    let value = &model.attribution;
    serde_json::json!({"loading":value.loading,"error":value.error,
        "current":value.observed.as_ref().is_some_and(|o|o.current(shell,model)),
        "observed":value.observed.as_ref().map(|o|o.proof())})
}
