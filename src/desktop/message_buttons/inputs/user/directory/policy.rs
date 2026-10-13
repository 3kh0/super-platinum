//! Captured fRHe defaults + 4wTh member predicates for HvtU users_select.
use super::*;
// Agentforce needs available-app/access caches (zpOE.Cq / aiApps); absent in
// Workspace. This is an unavailable discovery capability, not a bot exclusion.
pub(crate) fn eligible(_workspace: &Workspace, user: &User) -> bool {
    super_platinum_core::block_actions::form::valid_message_user(&user.id)
        && !user.deleted
        && !matches!(user.id.as_str(), "USEARCH" | "UCHANNELEXPERT" | "USLACK")
        && ![
            "is_workflow_bot",
            "is_profile_only_user",
            "is_agentforce_bot",
        ]
        .iter()
        .any(|key| user.extra.get(*key) == Some(&Value::Bool(true)))
        && !(user.is_restricted
            && user
                .extra
                .get("restricted_user_type")
                .and_then(Value::as_str)
                == Some("external_guest"))
}
