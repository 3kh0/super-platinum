//! Final HTTP-only substitution reached by the actual issued-claim executor.
use super::*;
use super_platinum_core::slack::{
    client::{PreparedRequest, RequestBody},
    transport::ProtectedDraftFailure,
};
fn refused() -> ProtectedDraftFailure {
    ProtectedDraftFailure::NotRequested(super_platinum_core::slack::Error::Transport(
        "Owned fixture HTTP admission refused".into(),
    ))
}
pub(crate) fn response(request: PreparedRequest) -> Result<Value, ProtectedDraftFailure> {
    let b = RUN.get().ok_or_else(refused)?;
    if !enabled() || !b.released.load(Ordering::Acquire) {
        return Err(refused());
    }
    let parent = b
        .parent
        .lock()
        .map_err(|_| refused())?
        .clone()
        .ok_or_else(refused)?;
    if !parent.current() {
        return Err(refused());
    }
    let receipt = parent
        .record()
        .evidence
        .lock()
        .map_err(|_| refused())?
        .receipt
        .clone()
        .ok_or_else(refused)?;
    let op = receipt.operation();
    if receipt.scope().account_id != b.account.id()
        || receipt.scope().key != key(b.target())
        || !matches!(
            op.kind,
            super_platinum_core::drafts::sync::OperationKind::Send
        )
        || request.method != "POST"
        || request.retry_safe()
    {
        return Err(refused());
    }
    let parsed = url::Url::parse(&request.url).map_err(|_| refused())?;
    if parsed.scheme() != "https"
        || parsed.host_str() != Some("owned-send.slack.com")
        || parsed.path() != "/api/chat.postMessage"
    {
        return Err(refused());
    }
    let RequestBody::Form(form) = &request.body else {
        return Err(refused());
    };
    let get = |key: &str| {
        let values = form
            .iter()
            .filter(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
            .collect::<Vec<_>>();
        if values.len() == 1 {
            values.first().copied()
        } else {
            None
        }
    };
    if get("token") != Some("owned-send-fixture-token")
        || get("channel") != Some("C2")
        || get("client_msg_id") != Some(op.client_msg_id.as_str())
        || get("thread_ts") != op.scope.key.thread.as_deref()
    {
        return Err(refused());
    }
    let mut attempts = b.attempts.lock().map_err(|_| refused())?;
    if !attempts.is_empty() {
        return Err(refused());
    }
    let user = seed::self_user().ok_or_else(refused)?;
    let mut message =
        json!({"channel":"C2","ts":ACK_TS,"user":user,"client_msg_id":op.client_msg_id});
    if let Some(root) = &op.scope.key.thread {
        message["thread_ts"] = json!(root);
    }
    let raw = json!({"ok":true,"channel":"C2","ts":ACK_TS,"message":message,"fixtureEvidence":{"retained":true}});
    let fields = form
        .iter()
        .filter(|(k, _)| k != "token")
        .cloned()
        .collect::<Vec<_>>();
    attempts.push(json!({"method":request.method,"url":request.url,"headers":request.headers,"fields":fields,"retrySafe":false,"operation":op,"rawResponse":raw,"delivered":true}));
    Ok(raw)
}
