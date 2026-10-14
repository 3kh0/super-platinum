use super::*;

pub(super) async fn load(mut state: Signal<ShellState>, token: Token, visit: u64) {
    let context = {
        let shell = state.peek();
        if !draft_current(&shell, &token, visit) {
            return;
        }
        context(&shell, &token)
    };
    let result = match context {
        Some(context) => {
            api::fetch_own_profile_sections(&context.transport, &context.client, &context.session)
                .await
        }
        None => Err(Error::TransportNotConfigured),
    };
    finish_load(&mut state.write(), &token, visit, result);
}
pub(super) async fn save(mut state: Signal<ShellState>, attempt: Attempt) {
    if !draft_current(&state.peek(), &attempt.token, attempt.visit) {
        preflight_failed(&mut state.write(), &attempt, None);
        return;
    }
    let Some(context) = attempt.context.as_ref() else {
        preflight_failed(&mut state.write(), &attempt, None);
        return;
    };
    let fresh =
        api::fetch_own_profile_sections(&context.transport, &context.client, &context.session)
            .await;
    let fresh = match fresh {
        Ok(v) => v,
        Err(error) => {
            preflight_failed(&mut state.write(), &attempt, Some(&error));
            return;
        }
    };
    if !preflight(&mut state.write(), &attempt, &fresh) {
        return;
    }
    let result = api::set_own_profile_title(
        &context.transport,
        &context.client,
        &context.session,
        &attempt.intent,
        &fresh,
    )
    .await;
    let accepted = finish_write(&mut state.write(), &attempt, result);
    if accepted {
        let next = {
            let mut shell = state.write();
            let visit = shell.profile_title.draft.as_ref().map(|d| d.visit);
            visit.and_then(|v| begin_check(&mut shell, v))
        };
        if let Some((token, id, read)) = next {
            check(state, token, id, read).await;
        }
    }
}
pub(super) async fn check(mut state: Signal<ShellState>, token: Token, id: u64, read: u64) {
    let context = {
        let shell = state.peek();
        if !token.current(&shell) {
            None
        } else {
            context(&shell, &token)
        }
    };
    let current = state
        .peek()
        .profile_title
        .entries
        .get(&token.owner)
        .and_then(|e| e.pending.as_ref())
        .is_some_and(|a| a.id == id && a.read_id == Some(read) && a.phase == Phase::Checking);
    if !current {
        return;
    }
    let result = match context {
        Some(c) => api::fetch_own_profile_sections(&c.transport, &c.client, &c.session).await,
        None => Err(Error::TransportNotConfigured),
    };
    finish_check(&mut state.write(), &token, id, read, result);
    if token.current(&state.peek()) {
        crate::bootstrap::persist_workspace(&state, &token.owner.team);
    }
}
