use super::*;

pub(super) async fn run(mut state: Signal<ShellState>, attempt: Attempt) {
    if !attempt.token.current(&state.peek())
        || shared_retry_at(&state.peek(), &attempt.token) > now()
    {
        preflight_failed(&mut state.write(), &attempt, None, now());
        return;
    }
    let Some(context) = attempt.context.clone() else {
        preflight_failed(&mut state.write(), &attempt, None, now());
        return;
    };
    // The first read is read-only. Its result may never redirect this captured
    // user action to the workspace/channel currently displayed after the await.
    let prefs =
        api::fetch_notification_preferences(&context.transport, &context.client, &context.session)
            .await;
    let prefs = match prefs {
        Ok(prefs) => prefs,
        Err(error) => {
            preflight_failed(&mut state.write(), &attempt, Some(&error), now());
            return;
        }
    };
    {
        let mut shell = state.write();
        if !attempt.token.current(&shell)
            || target(&shell, &attempt.token.owner) != attempt.baseline
            || attempt.schedule.as_ref().is_some_and(|schedule| {
                shell.core.workspaces[&attempt.token.owner.team]
                    .notifications
                    .user
                    != *schedule
            })
            || matches!(attempt.change, api::ChannelMuteChange::TemporaryUntil(expiry) if expiry <= now())
        {
            preflight_failed(&mut shell, &attempt, None, now());
            return;
        }
        let Ok(canonical) = prefs.channel_state(&attempt.token.owner.channel) else {
            preflight_failed(&mut shell, &attempt, None, now());
            return;
        };
        if matches!(attempt.change, api::ChannelMuteChange::TemporaryUntil(_)) && canonical.muted {
            adopt(&mut shell, &attempt.token.owner, &prefs, now());
            preflight_failed(&mut shell, &attempt, None, now());
            return;
        }
        if attempt.change.confirms(canonical, now()) {
            let Some(entry) = operation_mut(&mut shell, &attempt) else {
                return;
            };
            if entry
                .operation
                .as_ref()
                .is_none_or(|o| o.phase != Phase::Preparing)
            {
                return;
            }
            entry.operation = None;
            entry.settled = Some((canonical, canonical.effective_muted(now())));
            entry.message = Some(
                if attempt.desired {
                    "Channel already muted."
                } else {
                    "Channel already unmuted."
                }
                .into(),
            );
            adopt(&mut shell, &attempt.token.owner, &prefs, now());
            return;
        }
        if !mark_sending(&mut shell, &attempt) {
            return;
        }
    }
    let result = api::set_channel_mute_change(
        &context.transport,
        &context.client,
        &context.session,
        &attempt.token.owner.channel,
        attempt.change,
    )
    .await;
    let check = {
        let mut shell = state.write();
        finish_write(&mut shell, &attempt, result, now())
    };
    if check {
        let read = {
            let mut shell = state.write();
            Token::capture(&shell, &attempt.token.owner.channel)
                .and_then(|token| begin_check(&mut shell, token, now()))
        };
        if let Some(read) = read {
            self::read(state, read).await;
        }
    }
}
pub(super) async fn read(mut state: Signal<ShellState>, read: Read) {
    if !read.token.current(&state.peek()) || shared_retry_at(&state.peek(), &read.token) > now() {
        finish_read(
            &mut state.write(),
            &read,
            Err(Error::Transport("stale read scope".into())),
            now(),
        );
        return;
    }
    let Some(context) = read.context.clone() else {
        return;
    };
    let result =
        api::fetch_notification_preferences(&context.transport, &context.client, &context.session)
            .await;
    let current = read.token.current(&state.peek());
    finish_read(&mut state.write(), &read, result, now());
    if current {
        crate::bootstrap::persist_workspace(&state, &read.token.owner.team);
    }
}
