//! Admission of optional display names from an in-flight user lookup.
use std::collections::{BTreeMap, BTreeSet};
use super_platinum_core::{
    slack::models::{User, UserProfile},
    state::Workspace,
};

pub(crate) type Snapshots = BTreeMap<String, Option<serde_json::Value>>;

pub(crate) fn needs_name(workspace: &Workspace, id: &str) -> bool {
    !id.trim().is_empty()
        && super_platinum_core::state::display_name(workspace.users.get(id), id) == id
}

pub(crate) fn capture(workspace: &Workspace, ids: &[String]) -> Snapshots {
    ids.iter()
        .filter(|id| needs_name(workspace, id))
        .filter_map(|id| {
            let snapshot = workspace
                .users
                .get(id)
                .map(serde_json::to_value)
                .transpose()
                .ok()?;
            Some((id.clone(), snapshot))
        })
        .collect()
}

pub(crate) fn apply(
    workspace: &mut Workspace,
    snapshots: &Snapshots,
    eligible: &BTreeSet<String>,
    users: Vec<User>,
) {
    for user in users {
        let Some(expected) = snapshots.get(&user.id) else {
            continue;
        };
        if !eligible.contains(&user.id) || !needs_name(workspace, &user.id) {
            continue;
        }
        let current = workspace
            .users
            .get(&user.id)
            .map(serde_json::to_value)
            .transpose();
        // Equality preserves observed serialized changes, not server freshness
        // or change-and-revert history.
        if current.ok().as_ref() != Some(expected) {
            continue;
        }
        let update = if let Some(existing) = workspace.users.get(&user.id) {
            // A sparse name lookup cannot reset cached role/deletion facts or
            // unrelated profile fields. The core merge preserves omitted names.
            let mut patch = existing.clone();
            patch.name = user.name;
            patch.real_name = user.real_name;
            patch.profile = user.profile.map(|profile| UserProfile {
                display_name: profile.display_name,
                real_name: profile.real_name,
                ..Default::default()
            });
            patch
        } else {
            user
        };
        workspace.apply_user_update(update);
    }
}
