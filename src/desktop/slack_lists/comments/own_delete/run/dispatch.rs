use super::*;
pub(crate) fn confirm(mut state: Signal<ShellState>, id: u64) {
    let (source, recovery, original) = {
        let shell = state.peek();
        if !presentation(&shell).is_some_and(|p| p.id == id && p.can_confirm) {
            return;
        }
        let s = session(&shell, id).unwrap();
        (
            s.source.clone(),
            s.recovery.clone().unwrap(),
            s.record.clone().unwrap(),
        )
    };
    {
        let mut shell = state.write();
        let active = session_mut(&mut shell, id).unwrap();
        active.busy = true;
        active.notice = None;
    }
    dioxus::core::spawn_forever(async move {
        let ready = async {
            let (_, serial, _, _) =
                controller::begin_preflight(&mut state.write()).ok_or_else(unavailable)?;
            let result = prepare(state, id, &source, serial).await;
            finish_read(
                &mut state.write(),
                &source.owner,
                serial,
                result.as_ref().err(),
            );
            let (intent, policy) = result?;
            // A reaction-only refresh can change raw bytes, but never the
            // original selected identity/body or source document.
            if intent.record().target() != original.target()
                || intent.record().ts() != original.ts()
                || !original.current(&source.original)
            {
                return Err(unavailable());
            }
            let context = source
                .owner
                .scope
                .request(&state.peek())
                .ok_or_else(unavailable)?;
            let request =
                api::list_record_comment_delete(&context.client, &context.session, &intent)?;
            Ok((intent, policy, request, context.transport))
        }
        .await;
        let (intent, policy, request, transport) = match ready {
            Ok(v) => v,
            Err(_) => {
                notice(
                    state,
                    id,
                    "The message or deletion permissions changed. Nothing was sent; Cancel returns to your saved reply.",
                );
                return;
            }
        };
        let attempt = match recovery.begin(intent.record()).await {
            Ok(id) => id,
            Err(_) => {
                adopt(state, id, &recovery, None).await;
                notice(
                    state,
                    id,
                    "The deletion was not sent because its recovery record could not be saved. Your drafts are retained.",
                );
                return;
            }
        };
        let current = || {
            let shell = state.peek();
            session(&shell, id).is_some()
                && source.current(&shell)
                && policy.current(&shell)
                && suspended(&shell, id)
        };
        let result = if !current() {
            Err(ListCommentDeleteFailure::NotRequested(unavailable()))
        } else if crate::fixture::is_fixture() {
            fixture::own_delete::delete(state, &request, intent).await
        } else {
            transport
                .execute_list_comment_delete(request, intent, current)
                .await
        };
        let (outcome, deadline) = settle(result);
        let published = recovery
            .resolve(attempt, outcome, deadline)
            .await
            .ok()
            .flatten();
        adopt(state, id, &recovery, published).await;
    });
}
pub(super) fn settle(
    result: Result<
        super_platinum_core::slack_lists::comments::OwnDeleteAccepted,
        ListCommentDeleteFailure,
    >,
) -> (Outcome, u64) {
    match result {
        Ok(value) => (Outcome::Confirmed(Arc::new(value)), 0),
        Err(ListCommentDeleteFailure::NotRequested(error)) => {
            (Outcome::NotRequested, retry_deadline(&error).max(0) as u64)
        }
        Err(ListCommentDeleteFailure::Rejected(raw)) => {
            let deadline = if raw["error"] == "ratelimited" {
                now().saturating_add(30).max(0) as u64
            } else {
                0
            };
            (Outcome::Rejected(raw), deadline)
        }
        Err(ListCommentDeleteFailure::Unconfirmed { error, receipt }) => {
            let deadline = retry_deadline(&error).max(0) as u64;
            if matches!(
                error,
                Error::RateLimited { .. } | Error::HttpStatus { status: 429, .. }
            ) && receipt.is_none()
            {
                (Outcome::RateLimited, deadline)
            } else {
                (Outcome::Uncertain(receipt), deadline)
            }
        }
    }
}
