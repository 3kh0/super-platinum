//! Closed factory-checked original discovery and held known-member callback.
use super::*;
use std::time::Instant;
use super_platinum_core::block_actions::directory_endpoint::{
    DirectoryEndpoint, member_info::Read,
};
fn record(
    ctx: &Context,
    request: &PreparedRequest,
    expected: &PreparedRequest,
    kind: &str,
) -> Result<usize, Error> {
    if !completion::protocol::same_request(request, expected) {
        return Err(refused());
    }
    let mut args = match &request.body {
        RequestBody::Json(v) => v.clone(),
        RequestBody::Form(v) => {
            Value::Object(v.iter().map(|(k, v)| (k.clone(), json!(v))).collect())
        }
    };
    args.as_object_mut().ok_or_else(refused)?.remove("token");
    args.as_object_mut().unwrap().remove("enterprise_token");
    let mut l = ledger().lock().unwrap();
    let serial = l.requests.len() + 1;
    l.requests.push(json!({"serial":serial,"kind":kind,"lane":"known","method":if kind=="discovery"{"api.getFlannelHttpUrl"}else{"users/info"},
        "args":args,"account":ctx.account,"epoch":ctx.epoch,"team":ctx.session.team_id,"user":ctx.session.user_id,"response":null,"disposition":null}));
    Ok(serial)
}
pub(crate) async fn discovery(
    state: Signal<ShellState>,
    ctx: &Context,
    request: &PreparedRequest,
) -> Result<Value, Error> {
    admit(&state.peek(), ctx)?;
    let expected = DirectoryEndpoint::discovery(&ctx.client, &ctx.session)?;
    let serial = record(ctx, request, &expected, "discovery")?;
    let raw = completion::protocol::discovery(&ctx.session);
    let mut l = ledger().lock().unwrap();
    let r = l
        .requests
        .iter_mut()
        .find(|r| r["serial"] == serial)
        .unwrap();
    r["response"] = raw.clone();
    r["disposition"] = json!({"kind":"immediate-original"});
    Ok(raw)
}
pub(crate) async fn member(
    state: Signal<ShellState>,
    ctx: &Context,
    request: &PreparedRequest,
    read: &Read,
) -> Result<(Value, Option<Vec<String>>), Error> {
    admit(&state.peek(), ctx)?;
    if read.ids() != [ID.to_owned()] {
        return Err(refused());
    }
    let now = Instant::now();
    let endpoint = DirectoryEndpoint::decode(
        &ctx.session,
        &completion::protocol::discovery(&ctx.session),
        now,
    )?;
    let expected = endpoint.member_info(&ctx.client, &ctx.session, now, read)?;
    let serial = record(ctx, request, &expected, "member")?;
    let (tx, rx) = oneshot::channel();
    {
        let mut l = ledger().lock().unwrap();
        if l.held.is_some() {
            return Err(refused());
        }
        l.held = Some((serial, tx));
    }
    rx.await.map_err(|_| refused())?
}
