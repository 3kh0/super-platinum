//! Bounded ordinary-channel reduction of retained FS2H.E and +NpH.F.
//! Effective posting flags are acquired facts, not inferred from membership.
//! Joined, same-scope and nonexternal are this native slice's limits, not the
//! full Share picker policy. IM/member/app/external paths remain unsupported.
use crate::block_actions::posting_policy::Flags;
use serde_json::Value;

/// The caller owns canonical identity, raw observation and policy freshness.
/// `fromAnotherTeam` must be the actual canonical derived field.
pub fn joined_channel_eligible(raw: &Value, flags: Flags, team: &str, top_level: &str) -> bool {
    scope_id(team)
        && scope_id(top_level)
        && flags.permits_root_post()
        && joined_channel_candidate(raw)
}

/// Ordinary canonical candidate for matching/ranking, before fresh posting
/// validation. This is never permission to select or send to the channel.
pub fn joined_channel_candidate(raw: &Value) -> bool {
    if !raw["id"].as_str().is_some_and(channel_id)
        || raw["fromAnotherTeam"] != false
        || raw["is_member"] != true
    {
        return false;
    }
    for key in [
        "is_channel",
        "is_private",
        "is_group",
        "is_im",
        "is_mpim",
        "is_member",
        "is_archived",
        "is_frozen",
        "isUnknown",
        "isNonExistent",
        "is_ext_shared",
        "is_pending_ext_shared",
        "is_file",
    ] {
        if raw.get(key).is_some_and(|v| !v.is_boolean()) {
            return false;
        }
    }
    // zdO/.jq has a known conversation type and excludes unknown placeholders;
    // +NpH.F and 3Yi5.Zz exclude archived/frozen. The remaining FS2H member,
    // system and app branches require IM, which is outside this slice.
    if !(raw["is_channel"] == true || raw["is_private"] == true || raw["is_group"] == true)
        || [
            "is_im",
            "is_mpim",
            "is_archived",
            "is_frozen",
            "isUnknown",
            "isNonExistent",
            "is_ext_shared",
            "is_pending_ext_shared",
            "is_file",
        ]
        .iter()
        .any(|key| raw[*key] == true)
    {
        return false;
    }
    let properties = match raw.get("properties") {
        None | Some(Value::Null) => return true,
        Some(Value::Object(properties)) => properties,
        _ => return false,
    };
    // zdO/.cb independently rejects huddle channels even with posting flags.
    match properties.get("huddles") {
        None | Some(Value::Null) => true,
        Some(Value::Object(huddles)) => match huddles.get("is_huddle_channel") {
            None | Some(Value::Null) | Some(Value::Bool(false)) => true,
            _ => false,
        },
        _ => false,
    }
}

fn scope_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
}
fn channel_id(id: &str) -> bool {
    id.len() >= 9 && matches!(id.as_bytes()[0], b'C' | b'G') && scope_id(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn channel() -> Value {
        json!({"id":"CADDDEST01","is_channel":true,"is_member":true,"fromAnotherTeam":false})
    }
    fn allowed(raw: &Value) -> bool {
        joined_channel_eligible(
            raw,
            Flags {
                read_only: false,
                thread_only: false,
            },
            "T1",
            "E1",
        )
    }
    #[test]
    fn ordinary_public_and_private_require_actual_membership_and_scope() {
        assert!(allowed(&channel()));
        let private =
            json!({"id":"GPRIVATE1","is_private":true,"is_member":true,"fromAnotherTeam":false});
        assert!(allowed(&private));
        for key in ["is_member", "fromAnotherTeam"] {
            let mut raw = channel();
            raw.as_object_mut().unwrap().remove(key);
            assert!(!allowed(&raw));
        }
        let mut raw = channel();
        raw["is_member"] = json!(false);
        assert!(!allowed(&raw));
        raw = channel();
        raw["fromAnotherTeam"] = json!(true);
        assert!(!allowed(&raw));
    }
    #[test]
    fn known_posting_denials_never_become_membership_grants() {
        assert!(joined_channel_candidate(&channel()));
        for flags in [
            Flags {
                read_only: true,
                thread_only: false,
            },
            Flags {
                read_only: false,
                thread_only: true,
            },
        ] {
            assert!(!joined_channel_eligible(&channel(), flags, "T1", "E1"));
        }
        for key in [
            "is_archived",
            "is_frozen",
            "isUnknown",
            "isNonExistent",
            "is_im",
            "is_mpim",
        ] {
            let mut raw = channel();
            raw[key] = json!(true);
            assert!(!allowed(&raw), "{key}");
        }
    }
    #[test]
    fn unsupported_external_file_and_huddle_paths_are_not_fallback_grants() {
        for key in ["is_ext_shared", "is_pending_ext_shared", "is_file"] {
            let mut raw = channel();
            raw[key] = json!(true);
            assert!(!allowed(&raw), "{key}");
        }
        let mut raw = channel();
        raw["properties"] = json!({"huddles":{"is_huddle_channel":true}});
        assert!(!allowed(&raw));
        raw["properties"] = json!({"huddles":{"is_huddle_channel":false}});
        assert!(allowed(&raw));
    }
    #[test]
    fn malformed_identity_type_and_policy_inputs_are_refused() {
        for id in ["C1", "Cadddest01", "DADDDEST01", ""] {
            let mut raw = channel();
            raw["id"] = json!(id);
            assert!(!allowed(&raw));
        }
        for key in [
            "is_archived",
            "is_frozen",
            "is_ext_shared",
            "is_private",
            "isUnknown",
        ] {
            let mut raw = channel();
            raw[key] = json!("false");
            assert!(!allowed(&raw));
        }
        let mut raw = channel();
        raw["is_channel"] = json!(false);
        assert!(!allowed(&raw));
        raw = channel();
        raw["properties"] = json!([]);
        assert!(!allowed(&raw));
        raw["properties"] = json!({"huddles":{"is_huddle_channel":"false"}});
        assert!(!allowed(&raw));
    }
}
