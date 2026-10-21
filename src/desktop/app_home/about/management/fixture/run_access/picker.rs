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
fn page(kind: SearchKind) -> Value {
    match kind {
        SearchKind::Members => json!({"ok":true,"results":[seed::person()],"next_marker":""}),
        SearchKind::Channels => {
            json!({"ok":true,"results":[seed::channel()],"member_channels":["CRUNTEAM1"],"next_marker":""})
        }
        SearchKind::InternalTeams => {
            json!({"ok":true,"items":[seed::teams()[1]],"next_cursor_mark":""})
        }
        SearchKind::ExternalTeams | SearchKind::AvailableExternal => {
            json!({"ok":true,"results":[seed::teams()[2]],"next_marker":""})
        }
    }
}
pub(crate) async fn picker(
    mut state: Signal<ShellState>,
    request: &PreparedRequest,
) -> Option<Result<(Value, Option<Vec<String>>), Error>> {
    if !requested() {
        return None;
    }
    if prerequisites::requested() {
        return Some(prerequisites::picker(state, request).await);
    }
    let prepared = (|| {
        let mut s = state.write();
        let model = model(&s)?;
        if case() != "run-named-combined" {
            return Err(closed());
        }
        let h = s
            .app_home
            .about
            .management
            .surface
            .as_ref()
            .ok_or_else(closed)?;
        let ctx = &h.owner.ctx;
        let now = Instant::now();
        let endpoint = DirectoryEndpoint::decode(&ctx.session, &discovery_raw(), now)?;
        let mut matched = None;
        for kind in [
            SearchKind::Members,
            SearchKind::Channels,
            SearchKind::InternalTeams,
            SearchKind::ExternalTeams,
            SearchKind::AvailableExternal,
        ] {
            let query = if kind == SearchKind::AvailableExternal {
                ""
            } else {
                "run"
            };
            let read = Read::new(&ctx.session, kind, query, None)?;
            let expected = read.request(&ctx.client, &ctx.session, Some(&endpoint), now)?;
            if wire::same_request(request, &expected).is_ok() {
                matched = Some((read, kind));
                break;
            }
        }
        let (read, kind) = matched.ok_or_else(closed)?;
        let raw = page(kind);
        read.decode(raw.clone())?;
        let witness = owner(&s);
        let serial = {
            let mut m = model.0.lock().unwrap();
            let serial = m.requests.len() + 1;
            let body = match &request.body {
                RequestBody::Form(fields) => json!(
                    fields
                        .iter()
                        .filter(|(k, _)| k != "token")
                        .collect::<Vec<_>>()
                ),
                RequestBody::Json(v) => {
                    let mut v = v.clone();
                    let o = v.as_object_mut().ok_or_else(closed)?;
                    o.remove("token");
                    o.remove("enterprise_token");
                    v
                }
            };
            m.requests.push(json!({"serial":serial,"phase":"picker","method":read.method(),"body":body,"kind":format!("{kind:?}"),"query":read.body(),"retrySafe":request.retry_safe(),"owner":witness,"applied":false}));
            serial
        };
        s.app_home
            .about
            .management
            .fixture
            .as_mut()
            .ok_or_else(closed)?
            .requests
            .push(request.clone());
        Ok((model, serial, raw))
    })();
    Some(match prepared {
        Ok((model, serial, raw)) => deliver(model, serial, false, Ok(raw))
            .await
            .map(|v| (v, None)),
        Err(e) => Err(e),
    })
}
