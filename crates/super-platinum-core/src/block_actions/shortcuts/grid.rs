//! UaAK channel scope and kH+p internal membership, separate from auth flags.
use super::{identifier, invalid};
use crate::{
    config::WorkspaceSession,
    slack::{Error, PreparedRequest, SlackClient, models::Channel},
};
use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChannelScope {
    org_shared: bool,
    all_internal: bool,
    context: Option<String>,
    internal: Vec<String>,
}
fn team_id(s: &str) -> bool {
    s.starts_with('T') && identifier(s)
}
fn ids(v: &Value) -> Option<Vec<String>> {
    let rows = v.as_array().filter(|r| r.len() <= 4096)?;
    rows.iter()
        .map(|v| v.as_str().filter(|s| team_id(s)).map(str::to_owned))
        .collect()
}
impl ChannelScope {
    pub fn capture(ch: &Channel) -> Option<Self> {
        if ch.is_archived {
            return None;
        }
        let flag = |name| match ch.extra.get(name) {
            None | Some(Value::Null) => Some(false),
            Some(Value::Bool(v)) => Some(*v),
            _ => None,
        };
        let org_shared = flag("is_org_shared")?;
        let all_internal = org_shared && (ch.is_im || ch.is_mpim || flag("is_global_shared")?);
        let context = if org_shared {
            // UaAK returns from both org-shared branches before reading context.
            None
        } else {
            match ch.extra.get("context_team_id") {
                None | Some(Value::Null) => None,
                Some(Value::String(v)) if team_id(v) => Some(v.clone()),
                _ => return None,
            }
        };
        let internal = if org_shared && !all_internal {
            match ch
                .extra
                .get("internalTeamIds")
                .filter(|v| !v.is_null())
                .or_else(|| ch.extra.get("internal_team_ids").filter(|v| !v.is_null()))
            {
                Some(v) => ids(v)?,
                None => Vec::new(),
            }
        } else {
            Vec::new()
        };
        Some(Self {
            org_shared,
            all_internal,
            context,
            internal,
        })
    }
    pub fn needs_membership(&self) -> bool {
        self.org_shared
    }
    pub fn relevant(&self, membership: Option<&Membership>) -> Option<Vec<String>> {
        if !self.org_shared {
            return self.context.clone().map(|v| vec![v]);
        }
        let members = &membership?.internal;
        if self.all_internal {
            return Some(members.clone());
        }
        let mut seen = std::collections::BTreeSet::new();
        Some(
            self.internal
                .iter()
                .filter(|id| members.contains(id) && seen.insert(id.as_str()))
                .cloned()
                .collect(),
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Membership {
    internal: Vec<String>,
}
impl Membership {
    /// A response is scoped to the exact authenticated team and user. The
    /// metadata vector is never promoted into Session credentials.
    pub fn decode(raw: Value, session: &WorkspaceSession) -> Result<Self, Error> {
        if raw.get("ok") != Some(&Value::Bool(true))
            || raw.to_string().len() > 4 * 1024 * 1024
            || raw.pointer("/self/id").and_then(Value::as_str) != Some(session.user_id.as_str())
            || raw.pointer("/team/id").and_then(Value::as_str) != Some(session.team_id.as_str())
        {
            return Err(invalid());
        }
        let rows = raw
            .get("workspaces")
            .and_then(Value::as_array)
            .filter(|rows| !rows.is_empty() && rows.len() <= 4096)
            .ok_or_else(invalid)?;
        let mut seen = std::collections::BTreeSet::new();
        let mut internal = Vec::new();
        for row in rows {
            let id = row
                .get("id")
                .and_then(Value::as_str)
                .filter(|id| team_id(id))
                .ok_or_else(invalid)?;
            if !seen.insert(id) {
                return Err(invalid());
            }
            if row.get("allow_external_users") != Some(&Value::Bool(true)) {
                internal.push(id.to_owned());
            }
        }
        Ok(Self { internal })
    }
}

pub fn discovery(client: &SlackClient, session: &WorkspaceSession) -> PreparedRequest {
    client.rest_form(
        session,
        "client.gantryBoot",
        vec![
            ("include_channels", "false".into()),
            ("_x_reason", "fetch-client-data".into()),
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn session() -> WorkspaceSession {
        WorkspaceSession {
            team_id: "E1".into(),
            enterprise_id: Some("E1".into()),
            user_id: "U1".into(),
            name: "Synthetic".into(),
            url: "https://example.test".into(),
            token: "synthetic".into(),
        }
    }
    fn raw() -> Value {
        json!({"ok":true,"self":{"id":"U1"},"team":{"id":"E1"},"workspaces":[
        {"id":"T2"},{"id":"T_EXT","allow_external_users":true},{"id":"T1","allow_external_users":false}]})
    }
    fn scope(extra: Value) -> ChannelScope {
        let mut ch: Channel = serde_json::from_value(extra).unwrap();
        ch.id = "C1".into();
        ChannelScope::capture(&ch).unwrap()
    }
    #[test]
    fn fresh_membership_and_channel_scope_match_unified_selectors() {
        let m = Membership::decode(raw(), &session()).unwrap();
        assert_eq!(
            scope(json!({"id":"C1","is_org_shared":true,"is_global_shared":true}))
                .relevant(Some(&m)),
            Some(vec!["T2".into(), "T1".into()])
        );
        for key in ["is_im", "is_mpim"] {
            let mut ch = json!({"id":"C1","is_org_shared":true});
            ch[key] = json!(true);
            assert_eq!(
                scope(ch).relevant(Some(&m)),
                Some(vec!["T2".into(), "T1".into()])
            );
        }
        assert_eq!(
            scope(json!({"id":"C1","is_org_shared":true,"internal_team_ids":["T1","T_EXT","T9"]}))
                .relevant(Some(&m)),
            Some(vec!["T1".into()])
        );
        assert_eq!(scope(json!({"id":"C1","is_org_shared":true,"internalTeamIds":["T9"],"internal_team_ids":["T1"]})).relevant(Some(&m)),Some(vec![]));
        assert_eq!(
            scope(json!({"id":"C1","is_global_shared":true,"context_team_id":"T1"})).relevant(None),
            Some(vec!["T1".into()])
        );
        assert_eq!(
            scope(json!({"id":"C1","is_org_shared":true})).relevant(None),
            None
        );
    }
    #[test]
    fn missing_empty_wrong_owner_and_duplicate_membership_refuse() {
        for v in [json!(null), json!([])] {
            let mut r = raw();
            r["workspaces"] = v;
            assert!(Membership::decode(r, &session()).is_err());
        }
        for pointer in ["self", "team"] {
            let mut r = raw();
            r[pointer]["id"] = json!("WRONG");
            assert!(Membership::decode(r, &session()).is_err());
        }
        let mut r = raw();
        r["workspaces"] = json!([{"id":"T1"},{"id":"T1"}]);
        assert!(Membership::decode(r, &session()).is_err());
        let mut r = raw();
        r["workspaces"] = json!([{"id":"T_EXT","allow_external_users":true}]);
        assert_eq!(
            Membership::decode(r, &session()).unwrap().internal,
            Vec::<String>::new()
        );
    }
    #[test]
    fn membership_form_is_source_backed_read_only() {
        let r = discovery(&SlackClient::default(), &session());
        assert!(r.url.contains("/api/client.gantryBoot?"));
        let crate::slack::client::RequestBody::Form(fields) = r.body else {
            panic!("form")
        };
        let fields: std::collections::BTreeMap<_, _> =
            fields.into_iter().filter(|(k, _)| k != "token").collect();
        assert_eq!(
            fields,
            [
                ("include_channels".into(), "false".into()),
                ("_x_reason".into(), "fetch-client-data".into())
            ]
            .into()
        );
    }
    #[test]
    fn empty_internal_intersection_preserves_global_and_message_difference() {
        let membership = Membership::decode(raw(), &session()).unwrap();
        let related = scope(json!({"id":"C1","is_org_shared":true,"internal_team_ids":["T_EXT"]}))
            .relevant(Some(&membership))
            .unwrap();
        assert!(related.is_empty());
        let catalog = super::super::Catalog::decode(json!({"ok":true,"app_actions":[
            {"app_id":"A1","app_name":"Fixture","teams":["T_EXT"],"actions":[
                {"action_id":"global","type":"global_action","name":"Global"},
                {"action_id":"message","type":"message_action","name":"Message"}]}]}))
        .unwrap();
        let global = super::super::prepare(
            &SlackClient::default(),
            &session(),
            &catalog,
            &catalog.actions[0],
            &super::super::Context {
                channel: Some("C1".into()),
                message_ts: None,
            },
            &related,
            None,
            "fixture-token",
        )
        .unwrap();
        let crate::slack::client::RequestBody::Form(fields) = global.body else {
            panic!("form")
        };
        assert!(!fields.iter().any(|(key, _)| key == "team_id"));
        assert!(
            super::super::prepare(
                &SlackClient::default(),
                &session(),
                &catalog,
                &catalog.actions[1],
                &super::super::Context {
                    channel: Some("C1".into()),
                    message_ts: Some("1700.1".into())
                },
                &related,
                None,
                "fixture-token"
            )
            .is_err()
        );
    }
    #[test]
    fn org_shared_ignores_local_context_and_intersection_is_unique_in_channel_order() {
        let membership = Membership::decode(raw(), &session()).unwrap();
        let ch = scope(
            json!({"id":"C1","is_org_shared":true,"context_team_id":"E1",
            "internal_team_ids":["T1","T1","T_EXT","T2","T1"]}),
        );
        assert_eq!(
            ch.relevant(Some(&membership)),
            Some(vec!["T1".into(), "T2".into()])
        );
        let ch = scope(
            json!({"id":"C1","is_org_shared":true,"is_global_shared":true,
            "context_team_id":"E1"}),
        );
        assert_eq!(
            ch.relevant(Some(&membership)),
            Some(vec!["T2".into(), "T1".into()])
        );
        let ch: Channel =
            serde_json::from_value(json!({"id":"C1","context_team_id":"E1"})).unwrap();
        assert!(ChannelScope::capture(&ch).is_none());
    }
}
