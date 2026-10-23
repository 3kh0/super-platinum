//! Complete emoji.list receipt. Ownership and currentness belong to the caller;
//! partial emojis/info and a provisional picker map cannot construct this type.
use super::{Error, PreparedRequest, SlackClient};
use crate::config::WorkspaceSession;
use serde_json::Value;
use std::collections::BTreeSet;

/// Captures the exact workspace credentials without exposing them in Debug.
pub struct Read {
    owner: WorkspaceSession,
}
impl Read {
    pub fn new(owner: &WorkspaceSession) -> Self {
        Self {
            owner: owner.clone(),
        }
    }
    pub fn request(
        &self,
        client: &SlackClient,
        owner: &WorkspaceSession,
    ) -> Result<PreparedRequest, Error> {
        let original = &self.owner;
        if original.team_id != owner.team_id
            || original.user_id != owner.user_id
            || original.enterprise_id != owner.enterprise_id
            || original.url != owner.url
            || original.name != owner.name
            || original.token != owner.token
        {
            return Err(Error::Api("emoji_catalog_owner_changed".into()));
        }
        Ok(client.rest_form(owner, "emoji.list", Vec::new()))
    }
}

/// A positive complete response, including actual aliases and an actually empty
/// map. The raw response is immutable; no alias/image projection changes names.
#[derive(Clone, Debug, PartialEq)]
pub struct Complete {
    raw: Value,
    names: BTreeSet<String>,
}
impl Complete {
    pub fn decode(raw: Value) -> Result<Self, Error> {
        if raw.get("ok").and_then(Value::as_bool) != Some(true)
            || raw.get("error").is_some_and(|e| !e.is_null())
        {
            return Err(Error::Api("emoji_catalog_unresolved".into()));
        }
        let map = raw
            .get("emoji")
            .and_then(Value::as_object)
            .ok_or_else(|| Error::Api("emoji_catalog_unresolved".into()))?;
        if !map.values().all(Value::is_string) {
            return Err(Error::Api("emoji_catalog_unresolved".into()));
        }
        let names = map.keys().cloned().collect();
        Ok(Self { raw, names })
    }
    pub fn raw(&self) -> &Value {
        &self.raw
    }
    pub fn names(&self) -> &BTreeSet<String> {
        &self.names
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::slack::client::RequestBody;
    use serde_json::json;

    #[test]
    fn original_workspace_factory_is_exact_and_refuses_replacement() {
        let mut owner = WorkspaceSession {
            team_id: "T1".into(),
            user_id: "U1".into(),
            enterprise_id: None,
            name: "Fixture".into(),
            url: "https://fixture.slack.com".into(),
            token: "fixture".into(),
        };
        let read = Read::new(&owner);
        let request = read.request(&SlackClient::default(), &owner).unwrap();
        assert_eq!(request.method, "POST");
        assert_eq!(
            request.url.split('?').next(),
            Some("https://fixture.slack.com/api/emoji.list")
        );
        assert_eq!(
            request.body,
            RequestBody::Form(vec![("token".into(), "fixture".into())])
        );
        owner.token = "replacement".into();
        assert!(read.request(&SlackClient::default(), &owner).is_err());
    }
    #[test]
    fn full_raw_aliases_and_empty_are_positive_but_missing_and_errors_are_not() {
        let raw = json!({"ok":true,"emoji":{"party":"https://fixture/party","party_alias":"alias:party"},"future":{"retained":true}});
        let receipt = Complete::decode(raw.clone()).unwrap();
        assert_eq!(receipt.raw(), &raw);
        assert_eq!(
            receipt.names(),
            &BTreeSet::from(["party".into(), "party_alias".into()])
        );
        assert!(
            Complete::decode(json!({"ok":true,"emoji":{}}))
                .unwrap()
                .names()
                .is_empty()
        );
        for raw in [
            json!({}),
            json!({"ok":true}),
            json!({"ok":true,"emoji":null}),
            json!({"ok":true,"emoji":[]}),
            json!({"ok":true,"emoji":{"x":null}}),
            json!({"ok":false,"emoji":{}}),
            json!({"ok":true,"error":"offline","emoji":{}}),
        ] {
            assert!(Complete::decode(raw).is_err());
        }
    }

    #[test]
    fn parsed_shortcode_checks_exact_alias_membership_without_following_image_aliases() {
        use crate::slack_lists::comments::edit_seed::prepare::emoji_name;
        let receipt =
            Complete::decode(json!({"ok":true,"emoji":{"parrot_alias":"alias:unacquired_target"}}))
                .unwrap();
        let name = emoji_name(":parrot_alias::skin-tone-4:").unwrap().unwrap();
        assert_eq!(name, "parrot_alias");
        assert!(receipt.names().contains(&name));
        assert!(!receipt.names().contains("unacquired_target"));
        assert!(emoji_name("constructor").is_err());
    }
}
