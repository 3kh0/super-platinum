//! Raw-object selectors used by 4ucY/l24D. Permission checks remain separate.
//! Store-owned names are explicit inputs; remote `facts` are always discarded.
use serde_json::{Value, json};

pub struct Names<'a> {
    /// VRXO.sE or dfPN.SB from the captured preference/event/locale owner.
    pub comparison: &'a str,
    /// pS2E.Oi from the actual members/name store for this MPIM.
    pub mpim: Option<&'a str>,
    pub custom: Option<&'a str>,
    pub custom_allowed: bool,
}
fn truth(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|v| v != 0.0),
        Value::String(s) => !s.is_empty(),
        _ => true,
    }
}
fn text(v: &Value) -> &str {
    v.as_str().unwrap_or("")
}
fn syntax(id: &str, prefixes: &[u8], at: bool) -> bool {
    let id = if at {
        id.strip_prefix('@').unwrap_or(id)
    } else {
        id
    };
    let b = id.as_bytes();
    b.len() >= 9
        && prefixes.contains(&b[0])
        && b[1..]
            .iter()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
}
fn member_id(id: &str) -> bool {
    matches!(id, "USLACKBOT" | "USEARCH" | "UCHANNELEXPERT" | "USLACK") || syntax(id, b"UW", true)
}
fn real(raw: &Value) -> &str {
    if truth(&raw["profile"]["real_name"]) {
        text(&raw["profile"]["real_name"])
    } else {
        text(&raw["real_name"])
    }
}
/// Input must be an actual object. The returned copy preserves normalized stored
/// name fields already produced under the renderer owner; it does not derive
/// them again from raw names or promote ranking facts into selection authority.
pub fn item(raw: &Value, names: Names<'_>) -> Option<Value> {
    let mut result = raw.as_object()?.clone();
    let id = raw["id"].as_str()?;
    let member = member_id(id);
    let im = truth(&raw["is_im"]);
    let mpim = truth(&raw["is_mpim"]);
    let private = (truth(&raw["is_private"]) || truth(&raw["is_group"])) && !mpim;
    let channel = truth(&raw["is_channel"]) && !private && !mpim;
    let kind = if im {
        Some("im")
    } else if mpim {
        Some("mpim")
    } else if private {
        Some("private")
    } else if channel {
        Some("channel")
    } else {
        None
    };
    let slackbot = truth(&raw["is_slackbot"]) || id == "USLACKBOT";
    let uslack = id == "USLACK";
    let deleted = truth(&raw["deleted"]);
    let bot = truth(&raw["is_app_user"]) || truth(&raw["is_bot"]) || uslack;
    // etUg.ls excludes ordinary bots AND Slackbot/security/system users. A
    // suspended account is its separate deactivated selector, not deleted.
    let active = !(truth(&raw["is_bot"])
        || slackbot
        || uslack
        || truth(&raw["is_slacksecuritybot"])
        || id == "USLACKSECURITY"
        || truth(&raw["is_restricted"])
        || truth(&raw["is_ultra_restricted"])
        || deleted
        || truth(&raw["is_invited_user"])
        || truth(&raw["is_forgotten"]));
    let display = text(&raw["profile"]["display_name"]);
    let best = if display.is_empty() {
        real(raw)
    } else {
        display
    };
    let count = raw["members"]
        .as_array()
        .map(|m| (m.len().max(1) - 1).max(2));
    let facts = json!({
        "member":member, "self":truth(&raw["is_self"]),
        "deactivated":truth(&raw["suspended"]), "active":active,
        "firstName":text(&raw["profile"]["first_name"]), "lastName":text(&raw["profile"]["last_name"]),
        "mpim":mpim, "im":im, "privateChannel":private, "channel":channel, "channelType":kind,
        "conversation":kind.is_some() && syntax(id,b"CGDZ",false) && !truth(&raw["isUnknown"]) && !truth(&raw["isNonExistent"]),
        "archived":truth(&raw["is_archived"]), "deleted":deleted, "external":truth(&raw["isExternal"]),
        "frozen":truth(&raw["is_frozen"]), "bot":bot, "slackbot":slackbot, "uslack":uslack,
        "channelMember":truth(&raw["is_member"]), "otherTeam":raw["fromAnotherTeam"],
        "open":truth(&raw["is_open"]), "memberCount":count, "team":raw["team_id"],
        "comparisonName":names.comparison, "bestName":best, "mpimName":names.mpim,
        "customName":names.custom, "customNameAllowed":names.custom_allowed
    });
    result.insert("facts".into(), facts);
    Some(Value::Object(result))
}

/// Rank an effective user-group record from the caller's captured group store.
/// An S ID validates shape only: the caller still owns intake/source, policy,
/// account and query custody. This helper does not authorize a mention.
/// Group category identity cannot be changed by wire `facts` or shape aliases.
pub fn group(raw: &Value) -> Option<Value> {
    let id = raw["id"].as_str()?;
    if !(9..=255).contains(&id.len())
        || !id.starts_with('S')
        || !id.bytes().all(|c| c.is_ascii_alphanumeric())
        || ["name", "handle"]
            .iter()
            .any(|key| truth(&raw[*key]) && !raw[*key].is_string())
    {
        return None;
    }
    let mut result = item(
        raw,
        Names {
            comparison: text(&raw["name"]),
            mpim: None,
            custom: None,
            custom_allowed: false,
        },
    )?;
    for key in [
        "member",
        "self",
        "mpim",
        "im",
        "privateChannel",
        "channel",
        "conversation",
        "channelMember",
        "bot",
        "slackbot",
        "uslack",
    ] {
        result["facts"][key] = json!(false);
    }
    result["facts"]["channelType"] = Value::Null;
    result["facts"]["userGroup"] = json!(true);
    Some(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn observed(raw: Value) -> Value {
        item(
            &raw,
            Names {
                comparison: "Observed",
                mpim: None,
                custom: None,
                custom_allowed: false,
            },
        )
        .unwrap()
    }
    #[test]
    fn supplied_wire_facts_never_become_selector_observations() {
        let a = observed(
            json!({"id":"D12345678","is_im":true,"isUnknown":true,"facts":{"member":true,"conversation":true,"comparisonName":"Injected"}}),
        );
        assert_eq!(a["facts"]["member"], false);
        assert_eq!(a["facts"]["channelType"], "im");
        assert_eq!(a["facts"]["conversation"], false);
        assert_eq!(a["facts"]["comparisonName"], "Observed");
        let b = observed(json!({"id":"D1","is_im":true}));
        assert_eq!(b["facts"]["channelType"], "im");
        assert_eq!(b["facts"]["conversation"], false);
    }
    #[test]
    fn raw_external_team_and_stored_names_are_not_inferred_or_rederived() {
        let a = observed(
            json!({"id":"U12345678","team_id":"TOTHER","profile":{"real_name":"Raw"},"_real_name_normalized_lc":"stored"}),
        );
        assert_eq!(a["facts"]["member"], true);
        assert_eq!(a["facts"]["external"], false);
        assert!(a["facts"]["otherTeam"].is_null());
        assert_eq!(a["_real_name_normalized_lc"], "stored");
        let b =
            observed(json!({"id":"G12345678","is_mpim":true,"members":[],"is_ext_shared":true}));
        assert_eq!(b["facts"]["memberCount"], 2);
        assert_eq!(b["facts"]["external"], false);
    }
    #[test]
    fn effective_groups_cannot_spoof_other_category_facts_or_shape_aliases() {
        let raw = json!({"id":"S00000001","name":" Raw name ","handle":"@@raw",
            "is_usergroup":true,"is_channel":true,"is_im":true,"is_mpim":true,
            "is_group":true,"is_self":true,"is_bot":true,"opaque":{"keep":7},
            "facts":{"member":true,"channel":true,"conversation":true,"comparisonName":"Injected"}});
        let group = group(&raw).unwrap();
        assert_eq!(group["name"], raw["name"]);
        assert_eq!(group["handle"], raw["handle"]);
        assert_eq!(group["opaque"], raw["opaque"]);
        for key in [
            "member",
            "self",
            "channel",
            "privateChannel",
            "conversation",
            "im",
            "mpim",
            "bot",
        ] {
            assert_eq!(group["facts"][key], false, "{key}");
        }
        assert!(group["facts"]["channelType"].is_null());
        assert_eq!(group["facts"]["userGroup"], true);
        assert_eq!(group["facts"]["comparisonName"], " Raw name ");
    }
    #[test]
    fn group_shape_validation_is_not_a_permission_or_active_filter() {
        for raw in [
            Value::Null,
            json!({"id":"U00000001"}),
            json!({"id":"Sshort"}),
            json!({"id":"S00000001","name":{"unobserved":true}}),
        ] {
            assert!(group(&raw).is_none());
        }
        // Actual active/permission and source readiness are separate observations.
        let row = group(&json!({"id":"S00000001","isUnknown":true,"date_delete":7})).unwrap();
        assert_eq!(row["isUnknown"], true);
        assert_eq!(row["date_delete"], 7);
    }
}
