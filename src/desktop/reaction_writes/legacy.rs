//! Root-compatible legacy bridge. Coherent explicit replies keep their old path;
//! shared blocking is checked first. This is not universal cross-version exclusion.
use super::*;
use crate::{
    bootstrap::{credentials, persist_workspace},
    state::ShellState,
};
use dioxus::prelude::*;
use serde_json::Value;
use super_platinum_core::{
    config::WorkspaceSession,
    slack::{
        Error, PreparedRequest, SlackClient, Transport, api,
        transport::{ObservedReactionFailure, ReactionRequestKind, ReactionWireEvent},
    },
};
mod owner;
use owner::Owner;

pub(crate) async fn toggle(
    mut state: Signal<ShellState>,
    channel: String,
    ts: String,
    name: String,
    removing: bool,
) {
    let Some((transport, client, sessions)) = credentials(&state) else {
        return;
    };
    let Some(team) = state.peek().core.active_team.clone() else {
        return;
    };
    let Some(session) = sessions.into_iter().find(|w| w.team_id == team) else {
        return;
    };
    let work = {
        let mut s = state.write();
        let Some(owner) = Owner::capture(&s, transport, client, session) else {
            return;
        };
        let registry = registry(&s);
        let actor = owner.actor();
        let key = crate::reaction_writes::Key {
            actor: actor.clone(),
            channel: channel.clone(),
            ts: ts.clone(),
        };
        if registry.blocked(&key) {
            let notice = if registry.pending(&key) {
                "A reaction is already being checked or saved. Please wait."
            } else if registry.cooling(&key.actor) {
                "Slack asked you to wait before reacting again."
            } else if registry
                .receipt(&key)
                .is_some_and(|r| r.outcome == Outcome::Unknown)
            {
                "The earlier reaction is unconfirmed. Refresh to check current reactions."
            } else {
                "Reactions are unavailable right now. This press did not send another reaction."
            };
            s.show_toast(notice);
            return;
        }
        let reservation = if shapes::explicit_reply(&s, &team, &channel, &ts) {
            None
        } else {
            let Some(r) = registry.claim(&key) else {
                return;
            };
            Some(r)
        };
        let request = if removing {
            api::reactions_remove(
                &owner.client,
                &owner.session,
                channel.clone(),
                ts.clone(),
                name.clone(),
            )
        } else {
            api::reactions_add(
                &owner.client,
                &owner.session,
                channel.clone(),
                ts.clone(),
                name.clone(),
            )
        };
        crate::realtime::apply_reaction(
            &mut s.core,
            &team,
            &channel,
            &ts,
            &owner.session.user_id,
            &name,
            !removing,
        );
        s.refresh_from_core();
        (owner, Trace::new(registry, actor), reservation, request)
    };
    #[cfg(test)]
    let io = dioxus::prelude::try_consume_context::<
        std::rc::Rc<crate::thread_feed::reaction_toggle::test_io::Io>,
    >();
    let (tx, rx) = tokio::sync::oneshot::channel();
    dioxus::core::spawn_forever(async move {
        let (owner, trace, reservation, request) = work;
        let result = execute(
            state,
            &owner,
            &trace,
            request,
            &channel,
            &ts,
            &name,
            !removing,
            #[cfg(test)]
            io,
        )
        .await;
        let outcome = trace.outcome(&result);
        if let Some(reservation) = reservation {
            let id = reservation.id();
            reservation.finish(Receipt {
                id,
                name: name.clone(),
                adding: !removing,
                outcome,
            });
        }
        // Physical settlement is independent of this original UI/publication fence.
        if state.try_peek().is_ok_and(|s| owner.current(&s)) {
            let old_result = legacy_result(result);
            if let Err(error) = old_result {
                if let Ok(mut s) = state.try_write() {
                    if owner.current(&s) {
                        crate::realtime::apply_reaction(
                            &mut s.core,
                            &owner.session.team_id,
                            &channel,
                            &ts,
                            &owner.session.user_id,
                            &name,
                            removing,
                        );
                        s.refresh_from_core();
                        s.report_failure(&error, "Reaction not saved — you are offline", || {
                            format!("Reaction failed: {error}")
                        });
                    }
                }
            } else {
                persist_workspace(&state, &owner.session.team_id);
            }
        }
        let _ = tx.send(());
    });
    // Existing callers still await completion; dropping this receiver cannot
    // cancel the original detached driver or manufacture an empty physical slot.
    let _ = rx.await;
}
async fn execute(
    state: Signal<ShellState>,
    owner: &Owner,
    trace: &Trace,
    request: PreparedRequest,
    c: &str,
    t: &str,
    n: &str,
    adding: bool,
    #[cfg(test)] io: Option<std::rc::Rc<crate::thread_feed::reaction_toggle::test_io::Io>>,
) -> Result<Value, ObservedReactionFailure> {
    let current = || state.try_peek().is_ok_and(|s| owner.current(&s));
    #[cfg(test)]
    {
        let Some(io) = io else {
            return Err(ObservedReactionFailure::NotRequested(Error::Api(
                "synthetic_reaction_io_missing".into(),
            )));
        };
        if !current() {
            return Err(ObservedReactionFailure::NotRequested(Error::Api(
                "reaction_owner_retired".into(),
            )));
        }
        trace.observe(ReactionWireEvent::Attempted(ReactionRequestKind::Write));
        let result = io.execute(request).await;
        trace.test_result(ReactionRequestKind::Write, &result);
        return result.map_err(ObservedReactionFailure::Unknown);
    }
    #[cfg(not(test))]
    owner
        .transport
        .execute_legacy_reaction_once_observed(request, c, t, n, adding, current, |e| {
            trace.observe(e)
        })
        .await
}
fn legacy_result(result: Result<Value, ObservedReactionFailure>) -> Result<(), Error> {
    match result {
        Err(ObservedReactionFailure::NotRequested(e) | ObservedReactionFailure::Unknown(e)) => {
            Err(e)
        }
        Ok(raw) if raw["ok"] == false => Err(Error::Api(
            raw["error"]
                .as_str()
                .filter(|s| {
                    !s.is_empty()
                        && s.len() <= 256
                        && s.bytes()
                            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
                })
                .unwrap_or("unknown_error")
                .to_owned(),
        )),
        Ok(_) => Ok(()),
    }
}
