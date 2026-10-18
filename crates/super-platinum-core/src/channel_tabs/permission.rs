use super::*;
use crate::slack::{
    file_share_policy::{self, Eligibility, PostingFacts},
    models::User,
};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Permission {
    Allowed,
    Denied,
    Unavailable,
}
/// The caller supplies current-account ingress facts. Role grants are not
/// inferred from missing manager/enterprise stores.
pub fn permission(
    raw: &Value,
    member: &User,
    facts: &PostingFacts,
    enterprise: bool,
    plan: Option<&str>,
) -> Permission {
    if member.deleted || member.id.is_empty() {
        return Permission::Unavailable;
    }
    if raw["is_im"] == true || raw["is_mpim"] == true {
        return Permission::Unavailable;
    }
    match file_share_policy::destination(raw, facts, None) {
        Eligibility::Allowed => {}
        Eligibility::Denied(_) => return Permission::Denied,
        Eligibility::Unsupported(_) => return Permission::Unavailable,
    }
    let policy = raw.pointer("/properties/who_can_manage_tabs");
    match policy.and_then(Value::as_str) {
        Some("everyone") => Permission::Allowed,
        Some("managers")
            if plan == Some("")
                && (raw["creator"].as_str() == Some(member.id.as_str())
                    || (!enterprise && member.is_admin)) =>
        {
            Permission::Allowed
        }
        // Channel-manager and enterprise ACL stores are separate from Channel.
        // No synthetic `channel_managers` property is accepted as authority.
        Some("managers") => Permission::Unavailable,
        None if policy.is_none() && !enterprise => Permission::Allowed,
        _ => Permission::Unavailable,
    }
}
