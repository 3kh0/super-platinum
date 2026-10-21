use super::*;
use crate::slack_lists::comments::{
    Owner,
    own_delete_policy::{self, Observation},
};
use super_platinum_core::slack_lists::comments::OwnDeletePermissionQuery;
fn response(
    client: &super_platinum_core::slack::SlackClient,
    owner: &WorkspaceSession,
    request: &PreparedRequest,
    allowed: bool,
) -> Result<Value, Error> {
    let expected =
        api::conversations_generic_info(client, owner, &BTreeMap::from([("CCOMMENT01".into(), 0)]));
    if own_edit::same(request, &expected) {
        return Ok(
            json!({"ok":true,"channels":[{"id":"CCOMMENT01","is_channel":true,"is_file":true,"is_read_only":false,"is_thread_only":false,"context_team_id":"T1"}]}),
        );
    }
    let q = OwnDeletePermissionQuery::capture(&owner.team_id, &owner.user_id, "CCOMMENT01", false)
        .and_then(|q| q.with_enterprise_scope(owner.enterprise_id.as_deref()))
        .map_err(|_| refused())?;
    let expected = api::list_comment_delete_permissions(client, owner, &q)?;
    if !same_edge(request, &expected) {
        return Err(refused());
    }
    Ok(permission_response(allowed))
}
pub(super) fn permission_response(allowed: bool) -> Value {
    json!({"ok":true,"allowed":if allowed {vec!["DELETE_OWN_MESSAGE"]} else {vec![]},"permission_fixture_opaque":{"original":true}})
}
fn same_edge(a: &PreparedRequest, b: &PreparedRequest) -> bool {
    a.method == b.method
        && a.headers == b.headers
        && a.body == b.body
        && edge_url(&a.url)
            .zip(edge_url(&b.url))
            .is_some_and(|(a, b)| a == b)
}
fn edge_url(raw: &str) -> Option<(url::Url, BTreeMap<String, String>)> {
    let mut url = url::Url::parse(raw).ok()?;
    if url.scheme() != "https"
        || url.host_str() != Some("edgeapi.slack.com")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return None;
    }
    let mut query = BTreeMap::new();
    for (k, v) in url.query_pairs() {
        let v = match k.as_ref() {
            "_x_b3_spanid" | "_x_b3_traceid" | "fp" => {
                let n = if k == "_x_b3_spanid" { 16 } else { 32 };
                if v.len() != n
                    || !v
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                {
                    return None;
                }
                String::new()
            }
            "_x_app_name" | "_x_b3_sampled" | "_x_num_retries" => v.into_owned(),
            _ => return None,
        };
        if query.insert(k.into_owned(), v).is_some() {
            return None;
        }
    }
    if query.len() != 6 {
        return None;
    }
    url.set_query(None);
    Some((url, query))
}
fn wire(request: &PreparedRequest) -> Result<Value, Error> {
    if matches!(request.body, RequestBody::Form(_)) {
        return own_edit::wire(request);
    }
    let RequestBody::Json(raw) = &request.body else {
        return Err(refused());
    };
    let mut args = raw.clone();
    let args = args.as_object_mut().ok_or_else(refused)?;
    args.remove("token");
    args.remove("enterprise_token");
    let url = url::Url::parse(&request.url).map_err(|_| refused())?;
    Ok(json!({"method":request.method,"path":url.path(),"args":args}))
}
pub(crate) async fn policy(
    state: Signal<ShellState>,
    owner: &Owner,
    serial: u64,
) -> Result<Observation, Error> {
    model(&state.peek())?;
    own_delete_policy::acquire_with(state, owner, serial, move |request| async move {
        let shell = state.peek();
        let model = model(&shell)?;
        let owner = session(&shell)?;
        let mut l = model.0.lock().map_err(|_| refused())?;
        let allowed = !l
            .own_delete
            .as_ref()
            .ok_or_else(refused)?
            .permission_revoked;
        let raw = response(&shell.core.client, &owner, &request, allowed)?;
        let request = wire(&request)?;
        l.own_delete
            .as_mut()
            .ok_or_else(refused)?
            .policy
            .push(json!({"request":request,"response":raw}));
        Ok(raw)
    })
    .await
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn independent_edge_factories_match_but_admin_foreign_or_unknown_query_does_not() {
        let client = super_platinum_core::slack::SlackClient::default();
        let owner = WorkspaceSession {
            team_id: "T1".into(),
            user_id: completion::SELF.into(),
            ..super::super::super::session()
        };
        let q =
            OwnDeletePermissionQuery::capture("T1", completion::SELF, "CCOMMENT01", false).unwrap();
        let r = api::list_comment_delete_permissions(&client, &owner, &q).unwrap();
        assert_eq!(
            response(&client, &owner, &r, true).unwrap()["allowed"],
            json!(["DELETE_OWN_MESSAGE"])
        );
        let mut bad = r.clone();
        if let RequestBody::Json(raw) = &mut bad.body {
            raw["as_admin"] = json!(true);
        }
        assert!(response(&client, &owner, &bad, true).is_err());
        for suffix in ["&unknown=1", "&_x_app_name=client", "&_x_b3_spanid=bad"] {
            let mut bad = r.clone();
            bad.url.push_str(suffix);
            assert!(!same_edge(&r, &bad));
        }
        let mut bad = r.clone();
        bad.url = bad.url.replace("/cache/T1/", "/cache/TOTHER/");
        assert!(response(&client, &owner, &bad, true).is_err());
        assert!(response(&client, &owner, &bad, false).is_err());
        let original = response(&client, &owner, &r, true).unwrap();
        let revoked = response(&client, &owner, &r, false).unwrap();
        assert_eq!(revoked["allowed"], json!([]));
        assert_eq!(
            revoked["permission_fixture_opaque"],
            original["permission_fixture_opaque"]
        );
        assert_eq!(response(&client, &owner, &r, true).unwrap(), original);
        assert_eq!(original["allowed"], json!(["DELETE_OWN_MESSAGE"]));
    }
}
