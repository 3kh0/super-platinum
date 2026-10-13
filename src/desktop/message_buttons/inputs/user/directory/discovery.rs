//! Read-only search endpoints from WNgx / 53BY / vAgx. No DM-open/join call.
use super::*;
use super_platinum_core::slack::models::Channel;
async fn request(ctx: &Context, path: &str, body: Value) -> Result<Value, Error> {
    let req = ctx
        .client
        .edge_json(&ctx.session, path, body)
        .map_err(|_| Error::Api("conversation_search_unavailable".into()))?;
    ctx.transport.execute(req).await
}
fn results(value: &Value) -> Result<&Vec<Value>, Error> {
    value
        .get("results")
        .and_then(Value::as_array)
        .ok_or_else(|| Error::Api("conversation_search_invalid".into()))
}
pub(super) async fn fetch(
    ctx: &Context,
    query: &str,
    filter: Option<&super::super::conversations::Filter>,
) -> Result<(Vec<User>, Vec<Channel>), Error> {
    let users = async {
        if filter.is_some_and(|f| !f.allows("im")) {
            return Ok(Vec::new());
        }
        let mut body = serde_json::json!({"query":query,"count":24,"fuzz":1,"enable_workspace_ranking":true,"filter":"NOT deactivated"});
        if let Some(team) = filter.and_then(|f| f.only_from_team_id.as_ref()) {
            body["external_team"] = serde_json::json!(team);
            body["filter"] = serde_json::json!(if ctx.session.enterprise_id.is_some() {
                "NOT deactivated AND NOT org"
            } else {
                "NOT deactivated AND NOT team"
            });
        }
        let value = request(ctx, "users/search", body).await?;
        Ok(results(&value)?
            .iter()
            .take(24)
            .filter_map(|v| serde_json::from_value::<User>(v.clone()).ok())
            .collect::<Vec<_>>())
    };
    let channels = async {
        let Some(f) = filter.filter(|f| f.allows("public") || f.allows("private")) else {
            return Ok(Vec::new());
        };
        let mut body =
            serde_json::json!({"query":query,"count":24,"fuzz":1,"include_record_channels":true});
        if f.exclude_external_shared_channels {
            body["filter"] = serde_json::json!("NOT shared");
        }
        if let Some(team) = &f.only_from_team_id {
            body["external_team"] = serde_json::json!(team);
        }
        let value = request(ctx, "channels/search", body).await?;
        Ok(results(&value)?
            .iter()
            .take(24)
            .filter_map(|v| serde_json::from_value::<Channel>(v.clone()).ok())
            .collect::<Vec<_>>())
    };
    let mpims = async {
        if filter.is_none_or(|f| !f.allows("mpim")) {
            return Ok(Vec::new());
        }
        // vAgx normalizes a comma followed by whitespace, not arbitrary spaces.
        let query = normalize_mpim_query(query);
        let value = request(
            ctx,
            "mpims/search",
            serde_json::json!({"query":query,"count":24,"fuzz":1}),
        )
        .await?;
        let list = value
            .get("mpims")
            .or_else(|| value.get("results"))
            .and_then(Value::as_array)
            .ok_or_else(|| Error::Api("conversation_search_invalid".into()))?;
        Ok(list.iter().take(24).filter_map(mpim).collect::<Vec<_>>())
    };
    let (users, channels, mpims) = tokio::join!(users, channels, mpims);
    // Multiple independent endpoints may cool down together. Keep the longest
    // server deadline rather than losing it to an unrelated branch's failure.
    let delay = [
        users.as_ref().err(),
        channels.as_ref().err(),
        mpims.as_ref().err(),
    ]
    .into_iter()
    .flatten()
    .filter_map(|e| {
        if let Error::RateLimited {
            retry_after_secs: Some(seconds),
            ..
        } = e
        {
            Some(*seconds)
        } else {
            None
        }
    })
    .max();
    if let Some(seconds) = delay {
        return Err(Error::RateLimited {
            retry_after_secs: Some(seconds),
        });
    }
    let mut channels = channels?;
    channels.extend(mpims?);
    Ok((users?, channels))
}
fn normalize_mpim_query(query: &str) -> String {
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

fn mpim(value: &Value) -> Option<Channel> {
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
