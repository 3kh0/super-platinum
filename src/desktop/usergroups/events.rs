//! Reserve group socket ingress before the serial native reducer can change it.
use crate::{directory_member_fetches::Context, state::ShellState};
use std::sync::Arc;
use super_platinum_core::{slack::events::RtEvent, state::directory_usergroups::Operation};

pub(crate) fn prepare(
    shell: &mut ShellState,
    team: &str,
    generation: u64,
    event: &RtEvent,
) -> Option<super::source::Work> {
    let RtEvent::Unknown(raw) = event else {
        return None;
    };
    if !matches!(
        raw.kind.as_str(),
        "subteam_created"
            | "subteam_updated"
            | "subteam_members_changed"
            | "subteam_self_added"
            | "subteam_self_removed"
    ) || shell.core.workspaces.get(team)?.rt_generation != generation
    {
        return None;
    }
    let owner = Arc::new(Context::capture(shell, team).ok()?);
    let operation = Operation::Event {
        kind: raw.kind.clone(),
        raw: serde_json::Value::Object(raw.rest.clone().into_iter().collect()),
        // Same explicit ordinary client-v2 source contract as Profile name
        // policy. Its members_changed branch is skipped; self events remain.
        sonic: true,
    };
    match super::source::enqueue(shell, owner, operation) {
        super::source::Admission::Tracked(work) => Some(work),
        super::source::Admission::Unavailable => None,
    }
}
