use super::*;
pub(in crate::channel_mute) async fn run(mut state: Signal<ShellState>, attempt: Attempt) {
    if !attempt.token.current(&state.peek())
        || shared_retry_at(&state.peek(), &attempt.token) > now()
    {
        preflight_failed(&mut state.write(), &attempt, None, now());
        return;
    }
    let Some(c) = attempt.context.clone() else {
        preflight_failed(&mut state.write(), &attempt, None, now());
        return;
    };
    let fresh = match api::fetch_notification_preferences(&c.transport, &c.client, &c.session).await
    {
        Ok(v) => v,
        Err(e) => {
            preflight_failed(&mut state.write(), &attempt, Some(&e), now());
            return;
        }
    };
    if !preflight(&mut state.write(), &attempt, &fresh, now()) {
        return;
    }
    let result = api::set_channel_notification(
        &c.transport,
        &c.client,
        &c.session,
        attempt.settings.as_ref().unwrap(),
        &fresh,
        now(),
    )
    .await;
    let should_check = { finish_write(&mut state.write(), &attempt, result, now()) };
    if should_check {
        let read = {
            let mut s = state.write();
            Token::capture(&s, &attempt.token.owner.channel)
                .and_then(|t| begin_check(&mut s, t, now()))
        };
        if let Some(read) = read {
            super::super::network::read(state, read).await;
        }
    }
}
