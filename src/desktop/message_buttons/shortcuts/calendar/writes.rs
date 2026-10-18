use super::*;
use super_platinum_core::slack::PreparedRequest;
fn admit(s: &mut ShellState, id: u64, write: Write) -> Option<(Arc<Context>, u64)> {
    if !editable(s, id) {
        return None;
    }
    let ctx = context(s, id)?;
    if cooling(s, &ctx) {
        return None;
    }
    let provider = active(s, id)?.provider;
    let account = active(s, id)?.account.clone();
    let book = &mut s.message_buttons.shortcuts.calendar_attempts;
    if book.iter().any(|a| {
        a.epoch == ctx.epoch
            && a.team == ctx.session.team_id
            && a.provider == provider
            && a.write == write
            && matches!(a.result, Settlement::Sending | Settlement::Unknown)
    }) {
        return None;
    }
    // Completed attempts can retire; unknown writes never disappear to make room
    // for an accidental replay. The finite book is scoped to this account epoch.
    book.retain(|a| {
        a.epoch == ctx.epoch
            && !matches!(
                a.result,
                Settlement::Created(_) | Settlement::Rejected | Settlement::Unsent
            )
    });
    if book.len() >= 64 {
        return None;
    }
    let attempt = super::super::super::next_id();
    book.push(Attempt {
        id: attempt,
        epoch: ctx.epoch,
        team: ctx.session.team_id.clone(),
        provider,
        account,
        write,
        result: Settlement::Sending,
    });
    Some((ctx, attempt))
}
fn settle(s: &mut ShellState, ctx: &Context, attempt: u64, result: Settlement) {
    if !super::super::super::inputs::user::directory::owner_current(s, ctx) {
        return;
    }
    if let Some(a) = s
        .message_buttons
        .shortcuts
        .calendar_attempts
        .iter_mut()
        .find(|a| {
            a.id == attempt
                && a.epoch == ctx.epoch
                && a.team == ctx.session.team_id
                && a.result == Settlement::Sending
        })
    {
        a.result = result;
    }
}
fn response(result: &Result<serde_json::Value, Error>, room: bool) -> Settlement {
    match result {
        Err(Error::RateLimited { .. }) => Settlement::Unsent,
        Err(Error::Api(code)) if code == "block_interaction_not_sent_offline" => Settlement::Unsent,
        Err(_) => Settlement::Unknown,
        Ok(raw) if room => match cal::room_url(raw) {
            Ok(url) => Settlement::Created(Some(url)),
            Err(_) => match cal::event_result(raw) {
                cal::EventResult::Rejected => Settlement::Rejected,
                _ => Settlement::Unknown,
            },
        },
        Ok(raw) => match cal::event_result(raw) {
            cal::EventResult::Created { provider_link, .. } => Settlement::Created(provider_link),
            cal::EventResult::Rejected => Settlement::Rejected,
            cal::EventResult::Unknown => Settlement::Unknown,
        },
    }
}
pub(super) fn create(mut state: Signal<ShellState>, id: u64) {
    let job = {
        let mut s = state.write();
        let Some(event) = actions::event(&s, id) else {
            return;
        };
        let Some(c) = active(&s, id) else {
            return;
        };
        let Some(account) = c.account.as_ref() else {
            return;
        };
        let ctx = context(&s, id).unwrap();
        let Ok(request) = cal::create(&ctx.client, &ctx.session, c.provider, account, &event)
        else {
            return;
        };
        let Some((ctx, attempt)) = admit(&mut s, id, Write::Event(event)) else {
            if let Some(c) = active_mut(&mut s, id) {
                c.notice = Some(
                    "This request is already pending or unconfirmed. It cannot be repeated.".into(),
                );
            }
            return;
        };
        let c = active_mut(&mut s, id).unwrap();
        c.stage = Stage::Creating;
        c.attempt = Some(attempt);
        c.notice = None;
        (ctx, attempt, request)
    };
    dispatch(state, id, job, None);
}
pub(super) fn room(mut state: Signal<ShellState>, id: u64, app: String) {
    let job = {
        let mut s = state.write();
        if !editable(&s, id) {
            return;
        }
        let c = active(&s, id).unwrap();
        if c.room_pending.is_some() {
            return;
        }
        if app.is_empty() {
            let c = active_mut(&mut s, id).unwrap();
            c.selected_room.clear();
            c.room_unknown = false;
            return;
        }
        if !c.conferences.iter().any(|p| p.id == app) {
            return;
        }
        if c.rooms.contains_key(&app) {
            let c = active_mut(&mut s, id).unwrap();
            c.selected_room = app;
            c.room_unknown = false;
            return;
        }
        let d = s.message_buttons.shortcuts.dialog.as_ref().unwrap();
        let ctx = d.owner.ctx.clone();
        let Ok(request) = cal::room(&ctx.client, &ctx.session, &app, &d.owner.channel) else {
            return;
        };
        let channel = d.owner.channel.clone();
        let Some((ctx, attempt)) = admit(
            &mut s,
            id,
            Write::Room {
                app: app.clone(),
                channel,
            },
        ) else {
            if let Some(c) = active_mut(&mut s, id) {
                c.room_unknown = true;
                c.notice=Some("The earlier conference request is unconfirmed. Select None or another provider; this request will not be repeated.".into());
            }
            return;
        };
        let c = active_mut(&mut s, id).unwrap();
        c.room_pending = Some(attempt);
        c.room_unknown = false;
        c.selected_room = app.clone();
        c.notice = None;
        (ctx, attempt, request)
    };
    dispatch(state, id, job, Some(app));
}
fn dispatch(
    mut state: Signal<ShellState>,
    id: u64,
    (ctx, attempt, request): (Arc<Context>, u64, PreparedRequest),
    room: Option<String>,
) {
    dioxus::core::spawn_forever(async move {
        let admitted = {
            let s = state.peek();
            !cooling(&s, &ctx)
                && active(&s, id).is_some_and(|c| {
                    if room.is_some() {
                        c.room_pending == Some(attempt)
                    } else {
                        c.stage == Stage::Creating && c.attempt == Some(attempt)
                    }
                })
        };
        if !admitted {
            let mut s = state.write();
            settle(&mut s, &ctx, attempt, Settlement::Unsent);
            if let Some(c) = active_mut(&mut s, id) {
                if room.is_some() && c.room_pending == Some(attempt) {
                    c.room_pending = None;
                    c.selected_room.clear();
                } else if c.attempt == Some(attempt) {
                    c.stage = Stage::Failed;
                }
                c.notice = Some(
                    "The request was not sent because its owner changed or Slack asked us to wait."
                        .into(),
                );
            }
            return;
        }
        let result = ctx.transport.execute_block_interaction(request).await;
        let outcome = response(&result, room.is_some());
        let mut s = state.write();
        retain_limit(&mut s, &ctx, result.as_ref().err());
        settle(&mut s, &ctx, attempt, outcome.clone());
        let Some(c) = active_mut(&mut s, id) else {
            return;
        };
        if let Some(app) = room {
            if c.room_pending != Some(attempt) {
                return;
            }
            c.room_pending = None;
            match outcome {
                Settlement::Created(Some(url)) => {
                    c.rooms.insert(app, url);
                }
                Settlement::Unknown => {
                    c.room_unknown = true;
                    c.notice=Some("The conference may have been created, but its reply was not confirmed. This request will not be repeated. You can choose None to continue without it.".into());
                }
                _ => {
                    c.selected_room.clear();
                    c.notice = Some(
                        "The conference request was not confirmed. No event has been created."
                            .into(),
                    );
                }
            }
        } else {
            if c.attempt != Some(attempt) {
                return;
            }
            match outcome {
                Settlement::Created(link) => {
                    c.stage = Stage::Created;
                    c.provider_link = link;
                    c.notice = Some("Event created.".into());
                }
                Settlement::Unknown => {
                    c.stage = Stage::Uncertain;
                    c.notice=Some("The event may have been created. Check your calendar before creating another event. This request will not be repeated.".into());
                }
                Settlement::Unsent => {
                    c.stage = Stage::Failed;
                    c.notice=Some("The event was not sent. Wait for connectivity or the rate limit before a new attempt.".into());
                }
                _ => {
                    c.stage = Stage::Failed;
                    c.notice =
                        Some("The calendar declined this event. Your draft is retained.".into());
                }
            }
        }
    });
}

/// Persist only an explicit user toggle, never the initial mount or Cancel.
/// This is separate from event creation and has its own owned pending identity.
pub(super) fn preference(mut state: Signal<ShellState>, id: u64, value: bool) {
    let job = {
        let mut s = state.write();
        if !editable(&s, id) {
            return;
        }
        let c = active(&s, id).unwrap();
        if c.share == value
            || c.preference_pending.is_some()
            || !matches!(c.account, Some(cal::Account::Legacy { .. }))
        {
            return;
        }
        let Some((ctx, attempt)) = admit(&mut s, id, Write::Preference(value)) else {
            return;
        };
        let c = active_mut(&mut s, id).unwrap();
        c.share = value;
        c.preference_pending = Some(attempt);
        (ctx, attempt)
    };
    dioxus::core::spawn_forever(async move {
        let (ctx, attempt) = job;
        let admitted = {
            let s = state.peek();
            !cooling(&s, &ctx)
                && active(&s, id).is_some_and(|c| c.preference_pending == Some(attempt))
        };
        if !admitted {
            let mut s = state.write();
            settle(&mut s, &ctx, attempt, Settlement::Unsent);
            if let Some(c) =
                active_mut(&mut s, id).filter(|c| c.preference_pending == Some(attempt))
            {
                c.preference_pending = None;
                c.notice = Some(
                    "The sharing preference was not sent. Your choice still applies to this draft."
                        .into(),
                );
            }
            return;
        }
        let result = ctx
            .transport
            .execute_block_interaction(cal::set_share_preference(&ctx.client, &ctx.session, value))
            .await;
        let outcome = response(&result, false);
        let mut s = state.write();
        retain_limit(&mut s, &ctx, result.as_ref().err());
        settle(&mut s, &ctx, attempt, outcome.clone());
        if let Some(c) = active_mut(&mut s, id).filter(|c| c.preference_pending == Some(attempt)) {
            c.preference_pending = None;
            if !matches!(outcome, Settlement::Created(_)) {
                c.notice=Some("Your sharing choice applies to this draft, but saving the preference was not confirmed. It will not be retried automatically.".into());
            }
        }
    });
}

#[cfg(test)]
#[path = "writes_tests.rs"]
mod tests;
