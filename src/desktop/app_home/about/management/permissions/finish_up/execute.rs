use super::*;
mod groups;
mod publication;
pub(super) async fn run(mut state: Signal<ShellState>, id: u64) {
    if !matches!(validation::acquire(state, id).await, Ok(true)) {
        if let Some(a) = attempt_mut(&mut state.write(), id) {
            if a.validation["status"] == "pending" {
                a.validation["status"] = json!("failed");
            }
            a.local.notice=Some("Publication checks could not permit this workflow. Your details and permission selections are retained; nothing was sent.".into());
        }
        finish(state, id).await;
        return;
    }
    let count = attempt(&state.peek(), id)
        .and_then(|a| a.metadata.as_ref())
        .map_or(0, |p| p.slots().len());
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    // Source metadata Promise.all fails fast; the other originally admitted
    // response still settles in its own slot and never becomes a replay.
    let tx0 = tx.clone();
    let tx1 = tx.clone();
    tokio::join!(
        async move {
            if count > 0 {
                let ok = groups::metadata(state, id, 0).await;
                let _ = tx0.send(ok);
            }
        },
        async move {
            if count > 1 {
                let ok = groups::metadata(state, id, 1).await;
                let _ = tx1.send(ok);
            }
        },
        async move {
            let mut metadata_ok = true;
            for _ in 0..count {
                if rx.recv().await != Some(true) {
                    metadata_ok = false;
                    break;
                }
            }
            if metadata_ok {
                settings(state, id).await;
            } else {
                if let Some(a) = attempt_mut(&mut state.write(), id) {
                    a.continuation = Some("metadata-rejected");
                }
                publication::run(state, id).await;
            }
        }
    );
    finish(state, id).await;
}
async fn settings(mut state: Signal<ShellState>, id: u64) {
    if let Some(a) = attempt_mut(&mut state.write(), id) {
        a.phase = "settings";
    }
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let tx0 = tx.clone();
    let tx1 = tx.clone();
    let tx2 = tx;
    tokio::join!(
        async move {
            let _ = tx0.send(groups::copy(state, id).await);
        },
        async move {
            let _ = tx1.send(groups::run_access(state, id).await);
        },
        async move {
            let _ = tx2.send(groups::managers(state, id).await);
        },
        async move {
            let mut all = true;
            for _ in 0..3 {
                if rx.recv().await != Some(true) {
                    all = false;
                    break;
                }
            }
            if let Some(a) = attempt_mut(&mut state.write(), id) {
                a.continuation = Some(if all {
                    "settings-complete"
                } else {
                    "settings-rejected"
                });
            }
            publication::run(state, id).await;
        }
    );
}
pub(super) async fn send(
    mut state: Signal<ShellState>,
    id: u64,
    plan: &protocol::RequestPlan,
    kind: &str,
) -> Result<Value, Error> {
    if !authorized(state, id).await {
        return Err(Error::Api("workflow_management_not_sent_retired".into()));
    }
    let lease = attempt(&state.peek(), id)
        .ok_or_else(unavailable)?
        .lease
        .clone();
    let ctx = &lease.owner.ctx;
    let request = plan.request(&ctx.client, &ctx.session);
    let result = if fixture::enabled(&state.peek()) {
        fixture::first_publish::mutation(state, &request)
            .await
            .unwrap_or_else(|| Err(Error::Api("workflow_management_not_sent_fixture".into())))
    } else if crate::fixture::is_fixture() {
        Err(Error::Api("workflow_management_not_sent_fixture".into()))
    } else {
        match kind {
            "copy" => {
                ctx.transport
                    .execute_workflow_copy_permission(request)
                    .await
            }
            "run" => ctx.transport.execute_workflow_run_permission(request).await,
            "restriction" => ctx.transport.execute(request).await,
            "managers" => ctx.transport.execute_workflow_collaborators(request).await,
            "publish" => ctx.transport.execute_workflow_publish(request).await,
            _ => ctx.transport.execute_workflow_management(request).await,
        }
    };
    if let Err(Error::RateLimited { retry_after_secs }) = &result {
        navigation::retain_limit(&mut state.write(), ctx, *retry_after_secs);
    }
    result
}
pub(super) fn classify(result: Result<Value, Error>) -> (Outcome, Reply) {
    match result {
        Ok(raw) => {
            let outcome = if raw.is_object()
                && raw["ok"] == true
                && raw.get("error").is_none_or(Value::is_null)
            {
                Outcome::Succeeded(raw.clone())
            } else if raw.is_object() && raw["ok"] == false {
                Outcome::Rejected(raw.clone())
            } else {
                Outcome::Unknown
            };
            (outcome, Reply::Raw(raw))
        }
        Err(Error::Offline(_)) => (Outcome::NotSent, Reply::Failure("offline before dispatch")),
        Err(Error::Api(code)) if code.starts_with("workflow_management_not_sent_") => {
            (Outcome::NotSent, Reply::Failure("refused before dispatch"))
        }
        Err(_) => (Outcome::Unknown, Reply::Failure("delivery unconfirmed")),
    }
}
async fn finish(mut state: Signal<ShellState>, id: u64) {
    let mut s = state.write();
    let Some(a) = attempt_mut(&mut s, id) else {
        return;
    };
    a.finished = true;
    a.complete = a.publication.primary_succeeded()
        && a.followup
            .as_ref()
            .is_some_and(protocol::FollowUpPlan::fully_succeeded);
    a.phase = if a.complete {
        "published"
    } else {
        "unconfirmed"
    };
    if a.local.notice.is_none() {
        a.local.notice=Some(if a.complete{"Your workflow is published. Check current publication to verify the saved version."}else{"Publication could not be fully confirmed. Each response is retained; no operation will be replayed."}.into());
    }
    let lease = a.lease.clone();
    lease.finish(&mut s, None);
    // Keep this modal's exact lease until explicit Done/Cancel. The parent
    // editor remains unchanged, including any independently dirty native draft.
}
