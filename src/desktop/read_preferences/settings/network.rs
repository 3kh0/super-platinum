use super::*;
async fn fetch(c: &crate::scheduled::Context) -> Result<ReadPreferences, Error> {
    let response = c
        .transport
        .execute_read_preferences(api::read_preferences_request(&c.client, &c.session))
        .await?;
    api::parse_read_preferences(&response)
}
pub(super) async fn read(mut state: Signal<ShellState>, r: Read) {
    if !current(&state.peek(), &r.token, r.visit) || cooling(&state.peek()) {
        finish_read(
            &mut state.write(),
            &r,
            Err(Error::Offline(
                "Reading preference admission changed".into(),
            )),
        );
        return;
    }
    let result = match r.context.as_ref() {
        Some(c) => fetch(c).await,
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
    let fresh = match fetch(c).await {
        Ok(fresh) => fresh,
        Err(e) => {
            preflight_failed(&mut state.write(), &a, Some(&e));
            return;
        }
    };
    let request =
        match api::read_preferences_update_request(&c.client, &c.session, &a.intent, &fresh) {
            Ok(request) => request,
            Err(_) => {
                rejected_fresh(&mut state.write(), &a, &fresh);
                return;
            }
        };
    if !preflight(&mut state.write(), &a, &fresh) {
        return;
    }
    // No await between the final owner/baseline check and single-attempt dispatch.
    let result = c
        .transport
        .execute_read_preferences(request)
        .await
        .map(|_| ());
    let followup = { finish_write(&mut state.write(), &a, result) };
    if let Some(r) = followup {
        read(state, r).await;
    }
}
