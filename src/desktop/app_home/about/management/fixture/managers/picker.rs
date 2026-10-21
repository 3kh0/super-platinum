//! Original factory requests and immutable raw picker pages; never preselected IDs.
use super::*;
use protocol::settings_acquisition::{Read, SearchKind};
use std::time::Instant;
use super_platinum_core::block_actions::directory_endpoint::DirectoryEndpoint;

pub(super) fn discovery_raw() -> Value {
    json!({"ok":true,"url":"https://edgeapi.slack.com/cache/TWORKFLOW","ttl_seconds":60})
}
pub(super) fn discovery(s: &mut ShellState, request: &PreparedRequest) -> Result<Value, Error> {
    let h = s
        .app_home
        .about
        .management
        .surface
        .as_ref()
        .ok_or_else(closed)?;
    let expected = DirectoryEndpoint::discovery(&h.owner.ctx.client, &h.owner.ctx.session)?;
    wire::same_request(request, &expected)?;
    s.app_home
        .about
        .management
        .fixture
        .as_mut()
        .ok_or_else(closed)?
        .requests
        .push(request.clone());
    Ok(discovery_raw())
}

pub(crate) async fn picker(
    mut state: Signal<ShellState>,
    request: &PreparedRequest,
) -> Option<Result<(Value, Option<Vec<String>>), Error>> {
    if !requested() {
        return None;
    }
    let prepared = (|| {
        let mut s = state.write();
        let model = model(&s)?;
        let ctx = &s
            .app_home
            .about
            .management
            .surface
            .as_ref()
            .ok_or_else(closed)?
            .owner
            .ctx;
        let now = Instant::now();
        let endpoint = DirectoryEndpoint::decode(&ctx.session, &discovery_raw(), now)?;
        let (method, kind, query, raw) = if case() == "managers-cold" {
            use super_platinum_core::block_actions::directory_endpoint::member_info;
            let read = member_info::Read::capture(vec![raw::COLD.into()])?;
            let expected = endpoint.member_info(&ctx.client, &ctx.session, now, &read)?;
            wire::same_request(request, &expected)?;
            let raw = json!({"ok":true,"results":[raw::cold()],"can_interact":{"UMANCOLD1":true},"next_marker":""});
            member_info::Page::decode(&read, raw.clone(), None)?;
            ("users/info", "MemberInfo", Value::Null, raw)
        } else {
            let query = if agents::active() {
                let ordinary = Read::new(&ctx.session, SearchKind::Members, "ordinary", None)?;
                if wire::same_request(
                    request,
                    &ordinary.request(&ctx.client, &ctx.session, Some(&endpoint), now)?,
                )
                .is_ok()
                {
                    "ordinary"
                } else {
                    "manager"
                }
            } else {
                "manager"
            };
            let read = Read::new(&ctx.session, SearchKind::Members, query, None)?;
            let expected = read.request(&ctx.client, &ctx.session, Some(&endpoint), now)?;
            wire::same_request(request, &expected)?;
            let rows = if agents::active() {
                agents::rows(query)
            } else {
                raw::query_rows()
            };
            let raw = json!({"ok":true,"results":rows,"next_marker":""});
            read.decode(raw.clone())?;
            ("users/search", "Members", read.body().clone(), raw)
        };
        let mut body = match &request.body {
            RequestBody::Json(v) => v.clone(),
            RequestBody::Form(_) => return Err(closed()),
        };
        body.as_object_mut().ok_or_else(closed)?.remove("token");
        body.as_object_mut().unwrap().remove("enterprise_token");
        let witness = owner(&s);
        let (serial, hold, result) = {
            let mut m = model.0.lock().unwrap();
            let count = m.requests.iter().filter(|r| r["method"] == method).count();
            let offline = case() == "managers-cold" && count == 0;
            let hold = case() == "managers-cold" && count == 1 && !m.held_once;
            if case() == "managers-cold" && count >= 2 {
                return Err(closed());
            }
            let serial = m.requests.len() + 1;
            m.requests.push(json!({"serial":serial,"phase":"picker","method":method,"kind":kind,"body":body,"query":query,"owner":witness,"retrySafe":request.retry_safe(),"applied":false}));
            (
                serial,
                hold,
                if offline {
                    Err(Error::Offline(
                        "synthetic original manager unavailable".into(),
                    ))
                } else {
                    Ok(raw)
                },
            )
        };
        s.app_home
            .about
            .management
            .fixture
            .as_mut()
            .ok_or_else(closed)?
            .requests
            .push(request.clone());
        Ok((model, serial, hold, result))
    })();
    Some(match prepared {
        Ok((m, n, h, r)) => deliver(m, n, h, r).await.map(|v| (v, None)),
        Err(e) => Err(e),
    })
}
