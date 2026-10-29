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
    let mut request = users_request(ctx, query, None)?;
    let super_platinum_core::slack::client::RequestBody::Json(body) = &mut request.body else {
        return Err(Error::Api("conversation_search_unavailable".into()));
    };
    body["include_profile_only_users"] = serde_json::json!(true);
    Ok(request)
}
pub(crate) fn prepare_users_page(
    ctx: &Context,
    query: &str,
    marker: Option<&str>,
) -> Result<super_platinum_core::slack::PreparedRequest, Error> {
    let mut request = prepare_users(ctx, query)?;
    if let Some(marker) = marker {
        let super_platinum_core::slack::client::RequestBody::Json(body) = &mut request.body else {
            return Err(Error::Api("conversation_search_unavailable".into()));
        };
        body["marker"] = serde_json::json!(marker);
    }
    Ok(request)
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
    let mut request = channels_request(ctx, query, None, Some(policy))?;
    let super_platinum_core::slack::client::RequestBody::Json(body) = &mut request.body else {
        return Err(Error::Api("conversation_search_unavailable".into()));
    };
    body["check_membership"] = serde_json::json!(true);
    Ok(request)
}
pub(crate) fn prepare_channels_page(
    ctx: &Context,
    query: &str,
    policy: &super_platinum_core::block_actions::form::home::ChannelTeamPolicy,
    marker: Option<&str>,
) -> Result<super_platinum_core::slack::PreparedRequest, Error> {
    let mut request = prepare_channels(ctx, query, policy)?;
    if let Some(marker) = marker {
        let super_platinum_core::slack::client::RequestBody::Json(body) = &mut request.body else {
            return Err(Error::Api("conversation_search_unavailable".into()));
        };
        body["marker"] = serde_json::json!(marker);
    }
    Ok(request)
}
pub(crate) fn public_channel_filter_matches(filter: &super::super::conversations::Filter) -> bool {
    !filter.default_channels
        && filter.spec
            == super_platinum_core::block_actions::form::ConversationFilter {
                include: Some(vec!["public".into()]),
                ..Default::default()
            }
}
pub(crate) fn conversation_mask_matches(filter: &super::super::conversations::Filter) -> bool {
    let Some(include) = filter.spec.include.as_ref() else {
        return false;
    };
    !filter.default_channels
        && filter.spec.only_from_team_id.is_none()
        && !include.is_empty()
        && include.iter().enumerate().all(|(i, kind)| {
            matches!(kind.as_str(), "im" | "public" | "private") && !include[..i].contains(kind)
        })
}
pub(crate) fn prepare_conversation_page(
    ctx: &Context,
    query: &str,
    filter: &super::super::conversations::Filter,
    users: bool,
    marker: Option<&str>,
) -> Result<super_platinum_core::slack::PreparedRequest, Error> {
    if !conversation_mask_matches(filter)
        || if users {
            !filter.allows("im")
        } else {
            !filter.allows("public") && !filter.allows("private")
        }
    {
        return Err(Error::Api("conversation_search_unavailable".into()));
    }
    let mut request = if users {
        users_request(ctx, query, Some(filter))?
    } else {
        channels_request(ctx, query, Some(filter), None)?
    };
    let super_platinum_core::slack::client::RequestBody::Json(body) = &mut request.body else {
        return Err(Error::Api("conversation_search_unavailable".into()));
    };
    body[if users {
        "include_profile_only_users"
    } else {
        "check_membership"
    }] = serde_json::json!(true);
    if let Some(marker) = marker {
        body["marker"] = serde_json::json!(marker);
    }
    Ok(request)
}
/// Only ordinary Modal/message Channel controls opt into this single-provider path.
pub(crate) fn prepare_public_channels_page(
    ctx: &Context,
    query: &str,
    filter: &super::super::conversations::Filter,
    marker: Option<&str>,
) -> Result<super_platinum_core::slack::PreparedRequest, Error> {
    if !public_channel_filter_matches(filter) {
        return Err(Error::Api("conversation_search_unavailable".into()));
    }
    let mut request = channels_request(ctx, query, Some(filter), None)?;
    let super_platinum_core::slack::client::RequestBody::Json(body) = &mut request.body else {
        return Err(Error::Api("conversation_search_unavailable".into()));
    };
    body["check_membership"] = serde_json::json!(true);
    if let Some(marker) = marker {
        body["marker"] = serde_json::json!(marker);
    }
    Ok(request)
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
    Ok(decode_channel_rows(results(value)?))
}
pub(crate) fn decode_channel_rows(rows: &[Value]) -> Vec<Channel> {
    rows.iter()
        .take(24)
        .filter_map(|v| serde_json::from_value(v.clone()).ok())
        .collect()
}
pub(crate) fn decode_users(value: &Value) -> Result<Vec<User>, Error> {
    Ok(decode_user_rows(results(value)?))
}
pub(crate) fn decode_user_rows(rows: &[Value]) -> Vec<User> {
    rows.iter()
        .take(24)
        .filter_map(|v| serde_json::from_value::<User>(v.clone()).ok())
        .collect()
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
    fn conversation_mask_wrappers_keep_default_mpim_team_and_duplicate_cases_legacy() {
        use super_platinum_core::block_actions::form::ConversationFilter;
        let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
        crate::app_home::fixture::install_inputs(&mut shell);
        let ctx = Context::capture(&shell).unwrap();
        for include in [
            vec!["im"],
            vec!["public"],
            vec!["private"],
            vec!["im", "public", "private"],
        ] {
            let filter = super::super::super::conversations::Filter {
                spec: ConversationFilter {
                    include: Some(include.iter().map(|s| s.to_string()).collect()),
                    exclude_bot_users: true,
                    exclude_external_shared_channels: true,
                    ..Default::default()
                },
                ..Default::default()
            };
            assert!(conversation_mask_matches(&filter));
            for users in [false, true] {
                let enabled = if users {
                    filter.allows("im")
                } else {
                    filter.allows("public") || filter.allows("private")
                };
                let request =
                    prepare_conversation_page(&ctx, " literal ", &filter, users, Some("next"));
                if !enabled {
                    assert!(request.is_err());
                    continue;
                }
                let request = request.unwrap();
                assert!(request.url.contains(if users {
                    "/users/search"
                } else {
                    "/channels/search"
                }));
                let super_platinum_core::slack::client::RequestBody::Json(body) = request.body
                else {
                    panic!("JSON")
                };
                assert_eq!(body["query"], " literal ");
                assert_eq!(body["count"], 24);
                assert_eq!(body["marker"], "next");
                assert!(body.get("external_team").is_none());
                assert!(body.get("allowed_internal_teams").is_none());
                if users {
                    assert_eq!(body["include_profile_only_users"], true);
                    assert_eq!(body["filter"], "NOT deactivated");
                } else {
                    assert_eq!(body["check_membership"], true);
                    assert_eq!(body["include_record_channels"], true);
                    assert_eq!(body["filter"], "NOT shared");
                }
            }
        }
        for include in [
            None,
            Some(vec![]),
            Some(vec!["mpim".into()]),
            Some(vec!["im".into(), "mpim".into()]),
            Some(vec!["public".into(), "public".into()]),
            Some(vec!["unknown".into()]),
        ] {
            let filter = super::super::super::conversations::Filter {
                spec: ConversationFilter {
                    include,
                    ..Default::default()
                },
                ..Default::default()
            };
            assert!(!conversation_mask_matches(&filter));
            assert!(prepare_conversation_page(&ctx, "literal", &filter, true, None).is_err());
            assert!(prepare_conversation_page(&ctx, "literal", &filter, false, None).is_err());
        }
        let filter = super::super::super::conversations::Filter {
            spec: ConversationFilter {
                include: Some(vec!["im".into(), "public".into()]),
                only_from_team_id: Some("TOTHER001".into()),
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(!conversation_mask_matches(&filter));
    }
    #[test]
    fn public_channel_page_rejects_broader_filters_without_changing_account_route() {
        use super_platinum_core::block_actions::form::ConversationFilter;
        let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
        crate::app_home::fixture::install_inputs(&mut shell);
        let mut ctx = Context::capture(&shell).unwrap();
        let public = super::super::super::conversations::Filter {
            spec: ConversationFilter {
                include: Some(vec!["public".into()]),
                ..Default::default()
            },
            ..Default::default()
        };
        for enterprise in [None, Some("EFIXTURE1".into())] {
            ctx.session.enterprise_id = enterprise.clone();
            let mut filter = public.clone();
            filter.enterprise_id = enterprise.clone();
            assert!(public_channel_filter_matches(&filter));
            let request =
                prepare_public_channels_page(&ctx, " literal ", &filter, Some("next")).unwrap();
            assert!(request.url.contains(&format!(
                "/cache/{}/channels/search",
                enterprise.as_ref().unwrap_or(&ctx.session.team_id)
            )));
            let super_platinum_core::slack::client::RequestBody::Json(body) = request.body else {
                panic!("JSON")
            };
            assert_eq!(body["query"], " literal ");
            assert_eq!(body["count"], 24);
            assert_eq!(body["fuzz"], 1);
            assert_eq!(body["marker"], "next");
            assert_eq!(body["include_record_channels"], true);
            assert_eq!(body["check_membership"], true);
            assert!(body.get("allowed_internal_teams").is_none());
            assert!(body.get("external_team").is_none());
        }
        for change in 0..6 {
            let mut filter = public.clone();
            match change {
                0 => filter.spec.include = None,
                1 => filter.spec.include = Some(vec!["public".into(), "private".into()]),
                2 => filter.spec.only_from_team_id = Some("TOTHER001".into()),
                3 => filter.spec.exclude_bot_users = true,
                4 => filter.spec.exclude_external_shared_channels = true,
                5 => filter.default_channels = true,
                _ => unreachable!(),
            }
            assert!(!public_channel_filter_matches(&filter));
            assert!(
                matches!(prepare_public_channels_page(&ctx,"literal",&filter,None),Err(Error::Api(message)) if message=="conversation_search_unavailable")
            );
        }
    }
    #[test]
    fn ordinary_selector_flags_do_not_expand_filtered_aggregate_requests() {
        use super_platinum_core::{
            block_actions::form::{ConversationFilter, home::ChannelTeamPolicy},
            slack::client::RequestBody,
        };
        let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
        crate::app_home::fixture::install_inputs(&mut shell);
        let mut ctx = Context::capture(&shell).unwrap();
        let body = |request: super_platinum_core::slack::PreparedRequest| {
            let RequestBody::Json(body) = request.body else {
                panic!("JSON")
            };
            body
        };
        for enterprise in [None, Some("EFIXTURE1".into())] {
            ctx.session.enterprise_id = enterprise;
            let filter = super::super::super::conversations::Filter {
                spec: ConversationFilter {
                    include: Some(vec!["im".into(), "public".into()]),
                    only_from_team_id: Some("TOTHER001".into()),
                    exclude_external_shared_channels: true,
                    ..Default::default()
                },
                ..Default::default()
            };
            for marker in [None, Some("next")] {
                let user = body(prepare_users_page(&ctx, " literal ", marker).unwrap());
                assert_eq!(user["include_profile_only_users"], true);
                assert_eq!(user["count"], 24);
                assert_eq!(user["query"], " literal ");
                let policy = ChannelTeamPolicy::new(true, "TCONTEXT1").unwrap();
                let channel =
                    body(prepare_channels_page(&ctx, " literal ", &policy, marker).unwrap());
                assert_eq!(channel["check_membership"], true);
                assert_eq!(
                    channel["allowed_internal_teams"],
                    serde_json::json!(["TCONTEXT1"])
                );
                assert_eq!(channel["should_allowed_internal_teams_match_any"], true);
                assert_eq!(channel["count"], 24);
                assert_eq!(channel["query"], " literal ");
                assert_eq!(
                    channel.get("marker"),
                    marker.map(|m| serde_json::json!(m)).as_ref()
                );
                assert_eq!(
                    user.get("marker"),
                    marker.map(|m| serde_json::json!(m)).as_ref()
                );
                assert!(channel.get("include_record_channels").is_none());
            }
            let requests = aggregate::prepare(&ctx, " literal ", Some(&filter));
            let user = body(requests.users.unwrap().unwrap());
            let channel = body(requests.channels.unwrap().unwrap());
            assert!(user.get("include_profile_only_users").is_none());
            assert!(channel.get("check_membership").is_none());
            assert_eq!(user["external_team"], "TOTHER001");
            assert_eq!(
                user["filter"],
                if ctx.session.enterprise_id.is_some() {
                    "NOT deactivated AND NOT org"
                } else {
                    "NOT deactivated AND NOT team"
                }
            );
            assert_eq!(channel["include_record_channels"], true);
            assert_eq!(channel["external_team"], "TOTHER001");
            assert_eq!(channel["filter"], "NOT shared");
            assert!(channel.get("allowed_internal_teams").is_none());
            assert!(requests.mpims.unwrap().is_none());
        }
    }
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
            let mut expected = serde_json::json!({"query":" Ada ","count":24,"fuzz":1,"enable_workspace_ranking":true,"filter":"NOT deactivated","include_profile_only_users":true});
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
