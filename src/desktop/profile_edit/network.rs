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
            api::fetch_own_profile_capabilities(
                &context.transport,
                &context.client,
                &context.session,
            )
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
        api::fetch_own_profile_capabilities(&context.transport, &context.client, &context.session)
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
    let result = api::set_own_profile_text(
        &context.transport,
        &context.client,
        &context.session,
        attempt.patch.clone(),
    )
    .await;
    let accepted = finish_write(&mut state.write(), &attempt, result);
    if accepted {
        let next = {
            let mut shell = state.write();
            let visit = shell.profile_edit.draft.as_ref().map(|d| d.visit);
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
    let patch = state
        .peek()
        .profile_edit
        .entries
        .get(&token.owner)
        .and_then(|e| e.pending.as_ref())
        .filter(|a| a.id == id && a.read_id == Some(read) && a.phase == Phase::Checking)
        .map(|a| a.patch.clone());
    let Some(patch) = patch else {
        return;
    };
    let result = match context {
        Some(context) => context
            .transport
            .execute(api::users_profile_get(
                &context.client,
                &context.session,
                token.owner.user.clone(),
            ))
            .await
            .and_then(|v| decode(v, &patch)),
        None => Err(Error::TransportNotConfigured),
    };
    finish_check(&mut state.write(), &token, id, read, result);
    if token.current(&state.peek()) {
        crate::bootstrap::persist_workspace(&state, &token.owner.team);
    }
}
/// A missing patched field is not proof of a canonical clear. Every changed
/// field must be explicitly present as a string, alongside the real name.
/// Unchanged optional fields may be absent and never enter the adoption patch.
fn decode(value: Value, patch: &Value) -> Result<UserProfile, Error> {
    let malformed = || Error::Api("decode_own_profile_readback".into());
    if value.get("ok").and_then(Value::as_bool) != Some(true) {
        return Err(malformed());
    }
    let profile = value
        .get("profile")
        .filter(|p| p.is_object())
        .ok_or_else(malformed)?;
    if patch.as_object().is_none_or(|keys| {
        keys.keys()
            .any(|key| profile.get(key).and_then(Value::as_str).is_none())
    }) || profile.get("real_name").and_then(Value::as_str).is_none()
        || ["display_name", "pronouns"]
            .into_iter()
            .any(|key| profile.get(key).is_some_and(|v| !v.is_string()))
    {
        return Err(malformed());
    }
    serde_json::from_value(profile.clone()).map_err(|_| malformed())
}
