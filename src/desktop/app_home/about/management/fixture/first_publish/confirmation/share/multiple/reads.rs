use super::*;
use std::time::Instant;
use super_platinum_core::block_actions::directory_endpoint::{DirectoryEndpoint, member_info};

fn record(
    s: &ShellState,
    request: &PreparedRequest,
    method: &str,
    raw: &Value,
) -> Result<(), Error> {
    let model = capture(s).map_err(|_| closed())?;
    let (_, session) = context(s)?;
    let mut m = model.0.lock().unwrap();
    let d = m.multiple.as_mut().ok_or_else(closed)?;
    if d.reads.len() >= 64 {
        return Err(closed());
    }
    let serial = d.reads.len() + 1;
    d.reads.push(json!({"serial":serial,"method":method,"body":safe_body(&request.body),"endpoint":request.url.split('?').next(),"retrySafe":request.retry_safe(),"owner":owner(s,&session)}));
    d.replies
        .push(json!({"serial":serial,"raw":raw,"keys":null}));
    Ok(())
}
pub(crate) async fn read(
    state: Signal<ShellState>,
    request: &PreparedRequest,
) -> Option<Result<Value, Error>> {
    if !requested() {
        return None;
    }
    let (method, fields) = crate::app_home::about::management::fixture::decode(request).ok()?;
    let target = fields
        .iter()
        .find(|(k, _)| k == "channel")
        .map(|(_, v)| v.as_str());
    if method != "api.getFlannelHttpUrl"
        && !(method == "conversations.info" && matches!(target, Some(CHANNEL | dm::CHANNEL)))
    {
        return None;
    }
    Some((|| {
        let s = state.peek();
        if !active(&s) {
            return Err(closed());
        }
        let (client, session) = context(&s)?;
        let (expected, raw) = if method == "api.getFlannelHttpUrl" {
            (
                DirectoryEndpoint::discovery(&client, &session)?,
                dm::discovery(),
            )
        } else {
            let id = target.ok_or_else(closed)?;
            (
                api::conversations_info(&client, &session, id.into()),
                json!({"ok":true,"channel":if id==CHANNEL{channel()}else{dm::channel()},"opaque_receipt":"original-multiple-posting"}),
            )
        };
        super::super::super::super::picker::same_request(request, &expected)?;
        record(&s, request, &method, &raw)?;
        Ok(raw)
    })())
}
pub(crate) async fn member(
    state: Signal<ShellState>,
    request: &PreparedRequest,
) -> Option<Result<(Value, Option<Vec<String>>), Error>> {
    if !requested() {
        return None;
    }
    Some((|| {
        let s = state.peek();
        if !active(&s) {
            return Err(closed());
        }
        let (client, session) = context(&s)?;
        let RequestBody::Json(body) = &request.body else {
            return Err(closed());
        };
        let ids = body["ids"]
            .as_array()
            .and_then(|a| {
                a.iter()
                    .map(|id| {
                        id.as_str()
                            .filter(|id| PEERS.contains(id))
                            .map(str::to_owned)
                    })
                    .collect::<Option<Vec<_>>>()
            })
            .ok_or_else(closed)?;
        let read = member_info::Read::capture(ids)?;
        let now = Instant::now();
        let endpoint = DirectoryEndpoint::decode(&session, &dm::discovery(), now)?;
        super::super::super::super::picker::same_request(
            request,
            &endpoint.member_info(&client, &session, now, &read)?,
        )?;
        let rows = read
            .ids()
            .iter()
            .map(|id| peer(id).unwrap())
            .collect::<Vec<_>>();
        let interactions = read
            .ids()
            .iter()
            .map(|id| (id.clone(), json!(true)))
            .collect::<serde_json::Map<_, _>>();
        let raw = json!({"ok":true,"results":rows,"can_interact":interactions,"next_marker":"","opaque_receipt":"original-multiple-peers"});
        record(&s, request, "users/info", &raw)?;
        Ok((raw, None))
    })())
}
