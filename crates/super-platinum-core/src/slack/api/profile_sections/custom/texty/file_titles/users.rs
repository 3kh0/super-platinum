//! bQFi.userToText / hWCs.Gt, independently observed QFH5 policy.
use super::Unavailable;
use crate::block_actions::name_policy;
use serde_json::Value;
use std::collections::BTreeSet;

pub(super) fn id(leaf: &Value) -> Result<&str, Unavailable> {
    leaf.get("user_id")
        .and_then(Value::as_str)
        .filter(|id| {
            !id.is_empty() && id.len() <= 128 && id.bytes().all(|b| b.is_ascii_alphanumeric())
        })
        .ok_or(Unavailable::UnsupportedTitle)
}

/// Dependencies only for the bounded rich_text/section/text-user-link grammar.
/// No flattening unsupported semantic leaves or plain-title fallback.
pub fn user_ids(effective: &Value) -> Result<BTreeSet<String>, Unavailable> {
    user_ids_with_emoji_context(effective, None)
}

/// The same explicit emoji admission used by extraction; emojis add no users.
pub fn user_ids_with_emoji_context(
    effective: &Value,
    emoji: Option<super::EmojiText>,
) -> Result<BTreeSet<String>, Unavailable> {
    super::dependencies::scan(effective, emoji, false, false).map(|v| v.users)
}

/// hWCs.Gt is mention spelling, independent of display-real-name preference.
/// Known canonical rows only; absence/staleness is an acquisition concern.
pub fn member_label(
    row: &Value,
    policy: &name_policy::Snapshot,
    hidden_text: &str,
) -> Result<String, Unavailable> {
    let member = row
        .get("id")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .ok_or(Unavailable::InvalidMetadata)?;
    if ["isUnknown", "isNonExistent"].iter().any(|k| {
        row.get(k)
            .is_some_and(|v| !matches!(v, Value::Null | Value::Bool(false)))
    }) {
        return Err(Unavailable::InvalidMetadata);
    }
    if policy.hidden(member) {
        return Ok(hidden_text.to_owned());
    }
    if !row["profile"].is_null() && !row["profile"].is_object() {
        return Err(Unavailable::InvalidMetadata);
    }
    for name in [
        &row["profile"]["display_name"],
        &row["profile"]["real_name"],
        &row["real_name"],
    ] {
        match name {
            Value::Null => (),
            Value::String(s) if s.is_empty() => (),
            Value::String(s) => return Ok(format!("@{s}")),
            _ => return Err(Unavailable::InvalidMetadata),
        }
    }
    Ok(format!("@{member}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn policy(hidden: &str) -> name_policy::Snapshot {
        let membership = name_policy::Membership::observe(
            None,
            &["T1".into()],
            &[("T1".into(), json!({"id":"T1"}))].into(),
        )
        .unwrap();
        let user = name_policy::UserPrefs::decode(
            &json!({"ok":true,"prefs":{"hidden_users":hidden,"display_real_names_override":1}}),
        )
        .unwrap();
        name_policy::Snapshot::new(name_policy::OrdinaryClientV2, membership, user, None).unwrap()
    }
    #[test]
    fn actual_names_hidden_and_nameless_fallback() {
        let row = json!({"id":"U1","name":"not-a-fallback","real_name":"Top","profile":{"display_name":"  Cafe\u{301}  ","real_name":"Real"}});
        assert_eq!(
            member_label(&row, &policy(""), "Hidden user").unwrap(),
            "@  Cafe\u{301}  "
        );
        assert_eq!(
            member_label(&row, &policy("U1"), "Hidden user").unwrap(),
            "Hidden user"
        );
        assert_eq!(
            member_label(
                &json!({"id":"U1","name":"ignored","profile":{}}),
                &policy(""),
                "Hidden user"
            )
            .unwrap(),
            "@U1"
        );
        assert!(
            member_label(
                &json!({"id":"U1","isUnknown":true}),
                &policy(""),
                "Hidden user"
            )
            .is_err()
        );
    }
    #[test]
    fn user_title_bypasses_unavailable_or_present_quip_without_mutation() {
        use super::super::{Kind, QuipTitle, Status, label_with_users};
        let row = json!({"title":"Wrong","title_blocks":[{"type":"rich_text","elements":[{"type":"rich_text_section","elements":[{"type":"text","text":"Review "},{"type":"user","user_id":"U1","opaque":true},{"type":"text","text":" &amp; "},{"type":"user","user_id":"U1"}]}]}]});
        let before = row.clone();
        assert_eq!(user_ids(&row).unwrap(), ["U1".to_owned()].into());
        for quip in [
            QuipTitle::Unavailable,
            QuipTitle::Text("Wrong override"),
            QuipTitle::Nullish,
        ] {
            assert_eq!(
                label_with_users(Kind::Canvas, Status::Normal, &row, quip, |id| Ok(format!(
                    "@{id}"
                )))
                .unwrap(),
                "Review @U1 &amp; @U1"
            );
        }
        assert_eq!(row, before);
        assert!(
            label_with_users(
                Kind::Canvas,
                Status::Normal,
                &row,
                QuipTitle::Text("Wrong"),
                |_| Err(Unavailable::InvalidMetadata)
            )
            .is_err()
        );
    }
    #[test]
    fn malformed_and_other_semantic_leaves_remain_unavailable() {
        for leaf in [
            json!({"type":"user","user_id":""}),
            json!({"type":"emoji","name":"wave"}),
            json!({"type":"text","text":3}),
        ] {
            assert!(user_ids(&json!({"title_blocks":[{"type":"rich_text","elements":[{"type":"rich_text_section","elements":[leaf]}]}]})).is_err());
        }
    }
}
