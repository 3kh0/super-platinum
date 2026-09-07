use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UserGroup {
    pub id: String,
    #[serde(default)]
    pub team_id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub handle: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub users: Vec<String>,
    #[serde(default)]
    pub members_loaded: bool,
    #[serde(default)]
    pub prefs: UserGroupPrefs,
    #[serde(default)]
    pub date_delete: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UserGroupPrefs {
    #[serde(default)]
    pub channels: Vec<String>,
    #[serde(default)]
    pub groups: Vec<String>,
}

impl UserGroup {
    pub fn includes(&self, user: &str) -> bool {
        self.date_delete == 0 && self.users.iter().any(|id| id == user)
    }

    pub fn mentioned_in(&self, message: &super::Message) -> bool {
        fn contains(value: &serde_json::Value, id: &str) -> bool {
            match value {
                serde_json::Value::Object(map) => {
                    (map.get("type").and_then(|v| v.as_str()) == Some("usergroup")
                        && map.get("usergroup_id").and_then(|v| v.as_str()) == Some(id))
                        || map.values().any(|v| contains(v, id))
                }
                serde_json::Value::Array(values) => values.iter().any(|v| contains(v, id)),
                serde_json::Value::String(text) => text_mentions(text, id),
                _ => false,
            }
        }
        message
            .text
            .as_deref()
            .is_some_and(|text| text_mentions(text, &self.id))
            || message.blocks.iter().any(|v| contains(v, &self.id))
    }
}

pub fn apply_group_event(
    groups: &mut std::collections::HashMap<String, UserGroup>,
    self_user: &str,
    event: &super::super::events::RawEvent,
) {
    match event.kind.as_str() {
        "subteam_created" | "subteam_updated" => {
            if let Some(value) = event.rest.get("subteam")
                && let Ok(mut group) = serde_json::from_value::<UserGroup>(value.clone())
            {
                if !value.get("users").is_some() {
                    if let Some(old) = groups.get(&group.id) {
                        group.users = old.users.clone();
                        group.members_loaded = old.members_loaded;
                    }
                } else {
                    group.members_loaded = true;
                }
                groups.insert(group.id.clone(), group);
            }
        }
        "subteam_members_changed" | "subteam_self_added" | "subteam_self_removed" => {
            let Some(id) = event.rest.get("subteam_id").and_then(|v| v.as_str()) else {
                return;
            };
            let group = groups.entry(id.into()).or_insert_with(|| UserGroup {
                id: id.into(),
                ..Default::default()
            });
            let strings = |key: &str| {
                event
                    .rest
                    .get(key)
                    .and_then(|v| v.as_array())
                    .into_iter()
                    .flatten()
                    .filter_map(|v| v.as_str())
                    .collect::<Vec<_>>()
            };
            let mut removed = strings("removed_users");
            let mut added = strings("added_users");
            if event.kind == "subteam_self_added" {
                added.push(self_user);
            }
            if event.kind == "subteam_self_removed" {
                removed.push(self_user);
            }
            group.users.retain(|user| !removed.contains(&user.as_str()));
            for user in added {
                if !group.users.iter().any(|u| u == user) {
                    group.users.push(user.into());
                }
            }
        }
        _ => {}
    }
}

fn text_mentions(text: &str, id: &str) -> bool {
    text.split("<!subteam^").skip(1).any(|tail| {
        tail.split_once('>')
            .is_some_and(|(token, _)| token.split('|').next() == Some(id))
    })
}

pub fn collect_group_ids(message: &super::Message, ids: &mut std::collections::HashSet<String>) {
    fn text_ids(text: &str, ids: &mut std::collections::HashSet<String>) {
        for tail in text.split("<!subteam^").skip(1) {
            if let Some((token, _)) = tail.split_once('>') {
                let id = token.split('|').next().unwrap_or_default();
                if !id.is_empty() {
                    ids.insert(id.into());
                }
            }
        }
    }
    fn visit(value: &serde_json::Value, ids: &mut std::collections::HashSet<String>) {
        match value {
            serde_json::Value::Object(map) => {
                if map.get("type").and_then(|v| v.as_str()) == Some("usergroup")
                    && let Some(id) = map.get("usergroup_id").and_then(|v| v.as_str())
                {
                    ids.insert(id.into());
                }
                for value in map.values() {
                    visit(value, ids);
                }
            }
            serde_json::Value::Array(values) => {
                for value in values {
                    visit(value, ids);
                }
            }
            serde_json::Value::String(text) => text_ids(text, ids),
            _ => {}
        }
    }
    if let Some(text) = message.text.as_deref() {
        text_ids(text, ids);
    }
    for value in &message.blocks {
        visit(value, ids);
    }
}

#[derive(Deserialize)]
pub struct UserGroupsPage {
    pub results: Vec<UserGroup>,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn membership_deltas_and_metadata_updates_preserve_each_other() {
        let mut groups = std::collections::HashMap::from([(
            "S1".into(),
            UserGroup {
                id: "S1".into(),
                handle: "crew".into(),
                users: vec!["U1".into(), "U2".into()],
                members_loaded: true,
                ..Default::default()
            },
        )]);
        for value in [
            serde_json::json!({"type":"subteam_updated","subteam":{"id":"S1","handle":"new-name"}}),
            serde_json::json!({"type":"subteam_members_changed","subteam_id":"S1","added_users":["U3","U3"],"removed_users":["U2"]}),
        ] {
            apply_group_event(&mut groups, "U1", &serde_json::from_value(value).unwrap());
        }
        assert_eq!(groups["S1"].handle, "new-name");
        assert_eq!(groups["S1"].users, ["U1", "U3"]);
        assert!(groups["S1"].members_loaded);
        apply_group_event(
            &mut groups,
            "U1",
            &serde_json::from_value(
                serde_json::json!({"type":"subteam_self_removed","subteam_id":"S1"}),
            )
            .unwrap(),
        );
        assert!(!groups["S1"].includes("U1"));
    }

    #[test]
    fn collects_group_ids_in_both_encodings_without_duplicates() {
        let message = super::super::Message {
            text: Some("<!subteam^S1|@crew> <!subteam^S2>".into()),
            blocks: vec![
                serde_json::json!({"type":"rich_text", "elements":[{"type":"usergroup", "usergroup_id":"S1"}]}),
            ],
            ..Default::default()
        };
        let mut ids = std::collections::HashSet::new();
        collect_group_ids(&message, &mut ids);
        assert_eq!(
            ids,
            std::collections::HashSet::from(["S1".into(), "S2".into()])
        );
    }
    #[test]
    fn group_membership_and_mentions_use_exact_ids() {
        let mut group = UserGroup {
            id: "S1".into(),
            users: vec!["U1".into()],
            ..Default::default()
        };
        assert!(group.includes("U1"));
        assert!(!group.includes("U2"));
        for text in ["<!subteam^S1>", "<!subteam^S1|@crew>"] {
            assert!(group.mentioned_in(&super::super::Message {
                text: Some(text.into()),
                ..Default::default()
            }));
        }
        assert!(!group.mentioned_in(&super::super::Message {
            text: Some("<!subteam^S10> <!subteam^S1".into()),
            ..Default::default()
        }));
        assert!(group.mentioned_in(&super::super::Message { blocks: vec![serde_json::json!({"type":"rich_text", "elements":[{"type":"usergroup", "usergroup_id":"S1"}]})], ..Default::default() }));
        group.date_delete = 1;
        assert!(!group.includes("U1"));
    }
}
