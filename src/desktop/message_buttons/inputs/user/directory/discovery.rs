//! Read-only search endpoints from WNgx / 53BY / vAgx. No DM-open/join call.
use super::*;
use super_platinum_core::slack::models::Channel;
pub(crate) mod aggregate;
fn results(value: &Value) -> Result<&Vec<Value>, Error> {
    value
        .get("results")
        .and_then(Value::as_array)
        .ok_or_else(|| Error::Api("conversation_search_invalid".into()))
}
/// Shared users-only read preparation. Surface owners supply dispatch/currentness.
pub(crate) fn prepare_users(
    ctx: &Context,
    query: &str,
) -> Result<super_platinum_core::slack::client::PreparedRequest, Error> {
    users_request(ctx, query, None)
}
fn users_request(
    ctx: &Context,
    query: &str,
    filter: Option<&super::super::conversations::Filter>,
) -> Result<super_platinum_core::slack::client::PreparedRequest, Error> {
    let mut body = serde_json::json!({"query":query,"count":24,"fuzz":1,"enable_workspace_ranking":true,"filter":"NOT deactivated"});
    if let Some(team) = filter.and_then(|f| f.only_from_team_id.as_ref()) {
        body["external_team"] = serde_json::json!(team);
        body["filter"] = serde_json::json!(if ctx.session.enterprise_id.is_some() {
            "NOT deactivated AND NOT org"
        } else {
            "NOT deactivated AND NOT team"
        });
    }
    ctx.client
        .edge_json(&ctx.session, "users/search", body)
        .map_err(|_| Error::Api("conversation_search_unavailable".into()))
}
/// Home channels use the original internal-team parameters, distinct from external_team.
pub(crate) fn prepare_channels(
    ctx: &Context,
    query: &str,
    policy: &super_platinum_core::block_actions::form::home::ChannelTeamPolicy,
) -> Result<super_platinum_core::slack::client::PreparedRequest, Error> {
    channels_request(ctx, query, None, Some(policy))
}
fn channels_request(
    ctx: &Context,
    query: &str,
    filter: Option<&super::super::conversations::Filter>,
    policy: Option<&super_platinum_core::block_actions::form::home::ChannelTeamPolicy>,
) -> Result<super_platinum_core::slack::client::PreparedRequest, Error> {
    let mut body = serde_json::json!({"query":query,"count":24,"fuzz":1});
    if let Some(f) = filter {
        body["include_record_channels"] = serde_json::json!(true);
        if f.exclude_external_shared_channels {
            body["filter"] = serde_json::json!("NOT shared");
        }
        if let Some(team) = &f.only_from_team_id {
            body["external_team"] = serde_json::json!(team);
        }
    }
    if let Some(policy) = policy {
        body.as_object_mut()
            .unwrap()
            .extend(policy.request_fields().as_object().unwrap().clone());
    }
    ctx.client
        .edge_json(&ctx.session, "channels/search", body)
        .map_err(|_| Error::Api("conversation_search_unavailable".into()))
}
pub(crate) fn decode_channels(value: &Value) -> Result<Vec<Channel>, Error> {
    Ok(results(value)?
        .iter()
        .take(24)
        .filter_map(|v| serde_json::from_value(v.clone()).ok())
        .collect())
}
pub(crate) fn decode_users(value: &Value) -> Result<Vec<User>, Error> {
    Ok(results(value)?
        .iter()
        .take(24)
        .filter_map(|v| serde_json::from_value::<User>(v.clone()).ok())
        .collect())
}
pub(super) async fn fetch(
    ctx: &Context,
    query: &str,
    filter: Option<&super::super::conversations::Filter>,
) -> Result<(Vec<User>, Vec<Channel>), Error> {
    let requests = aggregate::prepare(ctx, query, filter);
    aggregate::execute(ctx, requests).await
}

pub(crate) fn normalize_mpim_query(query: &str) -> String {
    let mut result = String::new();
    let mut chars = query.chars().peekable();
    while let Some(c) = chars.next() {
        result.push(c);
        if c == ',' && chars.peek().is_some_and(|c| js_whitespace(*c)) {
            chars.next();
        }
    }
    result
}
fn js_whitespace(c: char) -> bool {
    matches!(c,'\u{0009}'..='\u{000D}'|'\u{0020}'|'\u{00A0}'|'\u{1680}'|'\u{2000}'..='\u{200A}'|'\u{2028}'|'\u{2029}'|'\u{202F}'|'\u{205F}'|'\u{3000}'|'\u{FEFF}')
}

pub(crate) fn mpim(value: &Value) -> Option<Channel> {
    let details = value.get("details")?.as_object()?;
    if details.get("is_mpim") != Some(&Value::Bool(true)) {
        return None;
    }
    let members = value.get("members")?.as_array()?;
    if members.len() > 9 || members.iter().any(|v| !v.is_string()) {
        return None;
    }
    let mut raw = details.clone();
    for key in ["id", "name", "members"] {
        if let Some(v) = value.get(key) {
            raw.insert(key.into(), v.clone());
        }
    }
    for (key, flag) in [
        ("is_mpim", true),
        ("is_im", false),
        ("is_private", true),
        ("is_member", true),
        ("is_archived", false),
    ] {
        raw.insert(key.into(), Value::Bool(flag));
    }
    serde_json::from_value(Value::Object(raw)).ok()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shared_users_request_preserves_account_route_and_bounded_decode() {
        let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
        crate::app_home::fixture::install_inputs(&mut shell);
        let mut ctx = Context::capture(&shell).unwrap();
        for enterprise in [None, Some("EFIXTURE1".to_owned())] {
            ctx.session.enterprise_id = enterprise.clone();
            let req = prepare_users(&ctx, " Ada ").unwrap();
            assert_eq!(req.method, "POST");
            let cache = enterprise.as_deref().unwrap_or(&ctx.session.team_id);
            assert_eq!(
                req.url.split('?').next().unwrap(),
                format!("https://edgeapi.slack.com/cache/{cache}/users/search")
            );
            let super_platinum_core::slack::client::RequestBody::Json(mut body) = req.body else {
                panic!("Expected JSON")
            };
            assert_eq!(body["token"], ctx.session.token);
            body.as_object_mut().unwrap().remove("token");
            body.as_object_mut().unwrap().remove("enterprise_token");
            let mut expected = serde_json::json!({"query":" Ada ","count":24,"fuzz":1,"enable_workspace_ranking":true,"filter":"NOT deactivated"});
            if let Some(id) = enterprise {
                expected["enterprise_id"] = serde_json::json!(id);
            }
            assert_eq!(body, expected);
        }
        let row = serde_json::json!({"id":"UREMOTE01","profile":{"display_name":"Remote"},"opaque":{"keep":true}});
        let result = decode_users(&serde_json::json!({"results":vec![row.clone();25]})).unwrap();
        assert_eq!(result.len(), 24);
        assert_eq!(result[0].extra["opaque"], row["opaque"]);
        assert!(decode_users(&serde_json::json!({"results":null})).is_err());
    }
    #[test]
    fn mpim_requires_typed_metadata_and_preserves_members() {
        assert_eq!(
            normalize_mpim_query("Ada,\u{feff}Blair,\u{0085}Casey"),
            "Ada,Blair,\u{0085}Casey"
        );
        let raw = serde_json::json!({"id":"G00000001","members":["U00000001","U00000002"],"details":{"is_mpim":true,"is_ext_shared":true}});
        let c = mpim(&raw).unwrap();
        assert!(c.is_mpim && c.is_ext_shared && !c.is_im);
        assert_eq!(c.extra["members"], raw["members"]);
        assert!(mpim(&serde_json::json!({"id":"G00000001","members":[]})).is_none());
        assert_eq!(normalize_mpim_query("Ada, Blair,Casey"), "Ada,Blair,Casey");
    }
}
