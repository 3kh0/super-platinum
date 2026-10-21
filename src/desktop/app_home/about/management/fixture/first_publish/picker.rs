//! Closed original picker requests and immutable raw pages for Finish Up.
use super::*;
use protocol::settings_acquisition::{Read, SearchKind};
use std::time::Instant;
use super_platinum_core::block_actions::directory_endpoint::{DirectoryEndpoint, member_info};

pub(super) fn discovery_raw() -> Value {
    json!({"ok":true,"url":"https://edgeapi.slack.com/cache/TWORKFLOW","ttl_seconds":60})
}

pub(super) fn validate_discovery(s: &ShellState, request: &PreparedRequest) -> Result<(), Error> {
    let ctx = &s
        .app_home
        .about
        .management
        .surface
        .as_ref()
        .ok_or_else(closed)?
        .owner
        .ctx;
    if !ctx.current(s) {
        return Err(closed());
    }
    same_request(
        request,
        &DirectoryEndpoint::discovery(&ctx.client, &ctx.session)?,
    )
}

pub(super) fn person(id: &str, label: &str) -> Value {
    json!({"id":id,"name":label.to_lowercase().replace(' ',"_"),"real_name":label,
        "team_id":"TWORKFLOW","updated":101,"deleted":false,"is_bot":false,
        "is_restricted":false,"is_ultra_restricted":false,"is_stranger":false,
        "profile":{"real_name":label,"real_name_normalized":label,
            "display_name":label,"display_name_normalized":label}})
}

fn hydration_rows(model: &Model, ids: &[String]) -> Result<Vec<Value>, Error> {
    let m = model.0.lock().unwrap();
    let seeded = m
        .seed
        .as_ref()
        .and_then(|s| s["boot"]["users"].as_array())
        .ok_or_else(closed)?;
    ids.iter()
        .map(|id| match id.as_str() {
            "UMANNEW01" => Ok(person(id, "New Manager")),
            "URUNADA01" => Ok(person(id, "Ada Reviewer")),
            "UWORKFLOW" | "UMANOLD01" => seeded
                .iter()
                .find(|r| r["id"] == id.as_str())
                .cloned()
                .ok_or_else(closed),
            _ => Err(closed()),
        })
        .collect()
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
        let h = s
            .app_home
            .about
            .management
            .surface
            .as_ref()
            .ok_or_else(closed)?;
        let ctx = &h.owner.ctx;
        if !ctx.current(&s) {
            return Err(closed());
        }
        let now = Instant::now();
        let endpoint = DirectoryEndpoint::decode(&ctx.session, &discovery_raw(), now)?;
        let body = match &request.body {
            RequestBody::Json(v) => v,
            _ => return Err(closed()),
        };
        let (method, kind, query, raw) = if let Some(ids) =
            body.get("ids").and_then(Value::as_array)
        {
            let ids = ids
                .iter()
                .map(|v| v.as_str().map(str::to_owned).ok_or_else(closed))
                .collect::<Result<Vec<_>, _>>()?;
            let read = member_info::Read::capture(ids)?;
            same_request(
                request,
                &endpoint.member_info(&ctx.client, &ctx.session, now, &read)?,
            )?;
            let rows = hydration_rows(&model, read.ids())?;
            let interactions = read
                .ids()
                .iter()
                .map(|id| (id.clone(), json!(true)))
                .collect::<serde_json::Map<_, _>>();
            let raw =
                json!({"ok":true,"results":rows,"can_interact":interactions,"next_marker":""});
            member_info::Page::decode(&read, raw.clone(), None)?;
            ("users/info", "MemberInfo".to_owned(), Value::Null, raw)
        } else {
            let mut matched = None;
            for (kind, query) in [
                (SearchKind::Members, "manager"),
                (SearchKind::Members, "Ada"),
                (SearchKind::Channels, "Ada"),
            ] {
                let read = Read::new(&ctx.session, kind, query, None)?;
                if same_request(
                    request,
                    &read.request(&ctx.client, &ctx.session, Some(&endpoint), now)?,
                )
                .is_ok()
                {
                    let raw = match kind {
                        SearchKind::Members => {
                            json!({"ok":true,"results":[if query == "manager" {person("UMANNEW01","New Manager")} else {person("URUNADA01","Ada Reviewer")}],"next_marker":""})
                        }
                        SearchKind::Channels => {
                            json!({"ok":true,"results":[],"member_channels":[],"next_marker":""})
                        }
                        _ => return Err(closed()),
                    };
                    read.decode(raw.clone())?;
                    matched = Some((read.method(), format!("{kind:?}"), read.body().clone(), raw));
                    break;
                }
            }
            matched.ok_or_else(closed)?
        };
        let mut safe = body.clone();
        let safe_object = safe.as_object_mut().ok_or_else(closed)?;
        safe_object.remove("token");
        safe_object.remove("enterprise_token");
        let d = document(&s, h.id);
        let owner = json!({"account":s.core.active_account,"epoch":s.core.account_epoch,"team":ctx.session.team_id,
            "user":ctx.session.user_id,"visit":h.id,"document":d.map(|d|d.id),"revision":d.map(|d|d.revision)});
        let serial = {
            let mut m = model.0.lock().unwrap();
            let serial = m.requests.len() + 1;
            m.requests.push(
                json!({"serial":serial,"phase":"picker","method":method,"kind":kind,"query":query,
                "body":safe,"retrySafe":request.retry_safe(),"owner":owner,"applied":false}),
            );
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
            .map(|raw| (raw, None)),
        Err(e) => Err(e),
    })
}

pub(super) fn same_request(
    request: &PreparedRequest,
    expected: &PreparedRequest,
) -> Result<(), Error> {
    if request.retry_safe() != expected.retry_safe()
        || request.method != expected.method
        || request.body != expected.body
        || request.headers != expected.headers
        || normalized(&request.url)
            .zip(normalized(&expected.url))
            .is_none_or(|(a, b)| a != b)
    {
        return Err(closed());
    }
    Ok(())
}

fn normalized(value: &str) -> Option<(String, std::collections::BTreeMap<String, String>)> {
    let parsed = url::Url::parse(value).ok()?;
    if parsed.fragment().is_some() {
        return None;
    }
    let mut fields = std::collections::BTreeMap::new();
    for (key, value) in parsed.query_pairs() {
        let value = if matches!(key.as_ref(), "_x_id" | "_x_csid" | "fp") {
            if value.len() != 32
                || !value
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                || uuid::Uuid::parse_str(&value).ok()?.get_version_num() != 4
            {
                return None;
            }
            "trace".to_owned()
        } else if matches!(key.as_ref(), "_x_b3_spanid" | "_x_b3_traceid") {
            let len = if key == "_x_b3_spanid" { 16 } else { 32 };
            if value.len() != len
                || !value
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            {
                return None;
            }
            "trace".to_owned()
        } else {
            value.into_owned()
        };
        if fields.insert(key.into_owned(), value).is_some() {
            return None;
        }
    }
    Some((value.split('?').next()?.into(), fields))
}
