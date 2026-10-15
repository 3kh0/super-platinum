//! HCA9 batchEmojiChanged invalidates added names, removes deleted/renamed old
//! names, and refreshes aliases affected by a rename. The desktop adapter uses
//! its existing complete emoji.list refresh instead of the source Flannel cache.
use crate::{slack::events::RawEvent, state::Workspace};
use serde_json::Value;

pub fn apply(workspace: &mut Workspace, event: &RawEvent) -> bool {
    if event.kind != "emoji_changed" {
        return false;
    }
    match event.rest.get("subtype").and_then(Value::as_str) {
        Some("add") if event.rest.get("name").and_then(Value::as_str).is_some() => {}
        Some("remove") => {
            let Some(names) = event.rest.get("names").and_then(Value::as_array) else {
                return false;
            };
            for name in names.iter().filter_map(Value::as_str) {
                workspace.custom_emoji.remove(name);
            }
        }
        Some("rename") => {
            let Some(old) = event.rest.get("old_name").and_then(Value::as_str) else {
                return false;
            };
            workspace.custom_emoji.remove(old);
        }
        _ => return false,
    }
    workspace.custom_emoji_revision = workspace.custom_emoji_revision.wrapping_add(1);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn removed_target_and_renamed_target_retire_alias_choices_before_refresh() {
        let mut workspace = Workspace::from_session(&crate::config::WorkspaceSession {
            team_id: "T".into(),
            user_id: "U".into(),
            name: "Fixture".into(),
            url: "https://example.invalid".into(),
            token: String::new(),
            enterprise_id: None,
        });
        workspace.apply_emojis(vec![
            crate::slack::models::Emoji {
                name: "custom".into(),
                value: "https://example.invalid/a.png".into(),
                ..Default::default()
            },
            crate::slack::models::Emoji {
                name: "alias".into(),
                value: "alias:custom".into(),
                ..Default::default()
            },
        ]);
        let revision = workspace.custom_emoji_revision;
        let event=serde_json::from_value(serde_json::json!({"type":"emoji_changed","subtype":"rename","old_name":"custom","new_name":"new"})).unwrap();
        assert!(apply(&mut workspace, &event));
        assert_eq!(workspace.custom_emoji_revision, revision + 1);
        assert!(!workspace.custom_emoji.contains_key("custom"));
        assert!(crate::state::custom_emoji_source(&workspace.custom_emoji, "alias").is_none());
    }
}
