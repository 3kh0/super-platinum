use super::*;
pub(super) async fn fetch(c: &Context) -> Result<ClearUnreadsPreference, Error> {
    let transport = c.transport.as_ref().ok_or(Error::TransportNotConfigured)?;
    let raw = transport
        .execute_read_shortcuts(api::clear_unreads_preference_request(&c.client, &c.session))
        .await?;
    api::parse_clear_unreads_preference(&raw)
}
pub(super) async fn preference_read(mut state: Signal<ShellState>, r: PrefRead) {
    if !r.context.visit_current(&state.peek()) || preference::cooling(&state.peek()) {
        preference::finish_read(
            &mut state.write(),
            &r,
            Err(Error::Offline("Preference read was not dispatched".into())),
        );
        return;
    }
    let result = fetch(&r.context).await;
    let next = { preference::finish_read(&mut state.write(), &r, result) };
    if let Some(next) = next {
        dispatch(state, next);
    }
}
pub(super) async fn bulk(mut state: Signal<ShellState>, w: Work) {
    if !sending(&state.peek(), &w) {
        cancel_unsent(&mut state.write(), &w);
        return;
    }
    let Some(transport) = w.context.transport.as_ref() else {
        cancel_unsent(&mut state.write(), &w);
        return;
    };
    // Only this stack-local wire request is dispatched. The retained operation
    // contains no request or intent that a later action could replay.
    let result = transport.execute_read_shortcuts(w.request.clone()).await;
    let id = { finish_write(&mut state.write(), &w, result) };
    if let Some(id) = id {
        reconcile::run(state, w.context, w.baseline, id).await;
    }
}
pub(super) async fn disable_confirmation(mut state: Signal<ShellState>, w: PrefWork) {
    if !w.context.current(&state.peek()) || preference::cooling(&state.peek()) {
        preference::preflight_failed(&mut state.write(), &w, None);
        return;
    }
    let fresh = match fetch(&w.context).await {
        Ok(p) => p,
        Err(e) => {
            preference::preflight_failed(&mut state.write(), &w, Some(&e));
            return;
        }
    };
    let Some(request) = preference::preflight(&mut state.write(), &w, &fresh) else {
        return;
    };
    let Some(transport) = w.context.transport.as_ref() else {
        return;
    };
    let result = transport.execute_read_shortcuts(request).await.map(|_| ());
    let check = { preference::finish_write(&mut state.write(), &w, result) };
    if check && !preference::cooling(&state.peek()) && w.context.current(&state.peek()) {
        let result = fetch(&w.context).await;
        preference::finish_check(&mut state.write(), &w, result);
    }
}
