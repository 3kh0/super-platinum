//! Two closed prerequisite reads. Raw initial grants are not ready selections.
use super::*;
use protocol::settings_acquisition::{Read, SearchKind};
use std::time::Instant;
use super_platinum_core::block_actions::directory_endpoint::{
    DirectoryEndpoint, channel_info, member_info,
};
pub(super) fn requested() -> bool {
    matches!(
        case().as_str(),
        "run-cold-targets" | "run-everyone-external"
    )
}
pub(super) fn original_run() -> Option<Value> {
    original_for(&case())
}
pub(super) fn original_for(name: &str) -> Option<Value> {
    match name {
        "run-cold-targets" => Some(
            json!({"ok":true,"permission_type":"named_entities","user_ids":["UMISSING01"],"channel_ids":["CMISSING01"],"team_ids":["TMISSING01"],"org_ids":[]}),
        ),
        "run-everyone-external" => Some(
            json!({"ok":true,"permission_type":"named_entities","user_ids":["UWORKFLOW"],"channel_ids":[],"team_ids":["TRUNEXTER"],"org_ids":[]}),
        ),
        _ => None,
    }
}
fn user() -> Value {
    let mut v = seed::person();
    v["id"] = json!("UMISSING01");
    v["name"] = json!("cold_runner");
    v["real_name"] = json!("Cold Runner");
    for k in [
        "real_name",
        "real_name_normalized",
        "display_name",
        "display_name_normalized",
    ] {
        v["profile"][k] = json!("Cold Runner");
    }
    v
}
fn channel() -> Value {
    let mut v = seed::channel();
    v["id"] = json!("CMISSING01");
    v["name"] = json!("cold-run-team");
    v
}
fn team() -> Value {
    json!({"id":"TMISSING01","name":"Cold internal","enterprise_id":"ERUNORG01"})
}
pub(super) async fn picker(
    mut state: Signal<ShellState>,
    request: &PreparedRequest,
) -> Result<(Value, Option<Vec<String>>), Error> {
    let (model, serial, hold, result) = {
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
        let endpoint =
            DirectoryEndpoint::decode(&ctx.session, &super::picker::discovery_raw(), now)?;
        let (method, kind, expected, raw) = if case() == "run-cold-targets" {
            let mr = member_info::Read::capture(vec!["UMISSING01".into()])?;
            let cr = channel_info::Read::capture(vec!["CMISSING01".into()], None)?;
            let tr = Read::teams_by_ids(&ctx.session, &["TMISSING01".into()], None)?;
            let member = endpoint.member_info(&ctx.client, &ctx.session, now, &mr)?;
            let channel_request = endpoint.channel_info(&ctx.client, &ctx.session, now, &cr)?;
            if wire::same_request(request, &member).is_ok() {
                let raw = json!({"ok":true,"results":[user()],"can_interact":{"UMISSING01":true},"next_marker":""});
                member_info::Page::decode(&mr, raw.clone(), None)?;
                ("users/info", "MemberInfo", member, raw)
            } else if wire::same_request(request, &channel_request).is_ok() {
                let raw = json!({"ok":true,"results":[channel()],"member_channels":["CMISSING01"],"next_marker":""});
                channel_info::Page::decode(&cr, raw.clone(), None)?;
                ("channels/info", "ChannelInfo", channel_request, raw)
            } else {
                let expected = tr.request(&ctx.client, &ctx.session, None, now)?;
                wire::same_request(request, &expected)?;
                let raw = json!({"ok":true,"teams":[team()]});
                tr.decode(raw.clone())?;
                ("team.info", "TeamsById", expected, raw)
            }
        } else {
            let read = Read::new(&ctx.session, SearchKind::AvailableExternal, "", None)?;
            let expected = read.request(&ctx.client, &ctx.session, Some(&endpoint), now)?;
            wire::same_request(request, &expected)?;
            let raw = json!({"ok":true,"results":[seed::teams()[2]],"next_marker":""});
            read.decode(raw.clone())?;
            ("externalTeams/list", "AvailableExternal", expected, raw)
        };
        wire::same_request(request, &expected)?;
        let witness = owner(&s);
        let body = match &request.body {
            RequestBody::Json(v) => {
                let mut v = v.clone();
                let m = v.as_object_mut().ok_or_else(closed)?;
                m.remove("token");
                m.remove("enterprise_token");
                v
            }
            RequestBody::Form(f) => {
                json!(f.iter().filter(|(k, _)| k != "token").collect::<Vec<_>>())
            }
        };
        let (serial, hold, result) = {
            let mut m = model.0.lock().unwrap();
            let count = m.requests.iter().filter(|r| r["method"] == method).count();
            let offline = case() == "run-cold-targets" && method == "users/info" && count == 0;
            let hold = !offline
                && !m.held_once
                && (method == "users/info" || method == "externalTeams/list");
            if (method == "users/info" && count >= 2) || (method != "users/info" && count >= 1) {
                return Err(closed());
            }
            let serial = m.requests.len() + 1;
            m.requests.push(json!({"serial":serial,"phase":"picker","method":method,"kind":kind,"body":body,"query":body,"owner":witness,"retrySafe":request.retry_safe(),"applied":false}));
            (
                serial,
                hold,
                if offline {
                    Err(Error::Offline(
                        "synthetic original target unavailable".into(),
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
        (model, serial, hold, result)
    };
    deliver(model, serial, hold, result)
        .await
        .map(|v| (v, None))
}
