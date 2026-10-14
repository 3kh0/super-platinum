use super::*;
pub(super) async fn read(mut state: Signal<ShellState>, r: Read) {
    if !current(&state.peek(), &r.token, r.visit) || cooling(&state.peek()) {
        finish_read(
            &mut state.write(),
            &r,
            Err(Error::Offline("Notification read admission changed".into())),
        );
        return;
    }
    let result = match r.context.as_ref() {
        Some(c) => api::fetch_notification_preferences(&c.transport, &c.client, &c.session).await,
        None => Err(Error::TransportNotConfigured),
    };
    finish_read(&mut state.write(), &r, result);
}
pub(super) async fn save(mut state: Signal<ShellState>, a: Attempt) {
    if !current(&state.peek(), &a.token, a.visit) || cooling(&state.peek()) {
        preflight_failed(&mut state.write(), &a, None);
        return;
    }
    let Some(c) = a.context.as_ref() else {
        preflight_failed(&mut state.write(), &a, None);
        return;
    };
    let fresh = match api::fetch_notification_preferences(&c.transport, &c.client, &c.session).await
    {
        Ok(s) => s,
        Err(e) => {
            preflight_failed(&mut state.write(), &a, Some(&e));
            return;
        }
    };
    if !preflight(&mut state.write(), &a, &fresh) {
        return;
    }
    let result =
        api::set_global_notification(&c.transport, &c.client, &c.session, &a.intent, &fresh).await;
    let followup = { finish_write(&mut state.write(), &a, result) };
    if let Some(r) = followup {
        read(state, r).await;
    }
}
