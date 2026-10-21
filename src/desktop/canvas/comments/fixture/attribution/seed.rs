use super::*;
use super_platinum_core::block_actions::workflow_management::member_hydration::{
    Flags, RendererReply,
};
/// All fixture normalization inputs are explicitly ASCII. Production canonical
/// intake/adoption still owns the original raw rows and typed projection.
pub(super) fn members(shell: &mut ShellState, session: &WorkspaceSession) -> Result<Value, Error> {
    let account = shell.core.active_account.clone().ok_or_else(refused)?;
    let epoch = shell.core.account_epoch;
    let w = shell
        .core
        .workspaces
        .get_mut(&session.team_id)
        .ok_or_else(refused)?;
    let assignments = json!({"ok":true,"assignments":{}});
    let flags = Flags::decode(&assignments).map_err(|_| refused())?;
    let pending = w.directory_members.prepare(
        &account,
        session,
        epoch,
        "canvas-attribution-fixture",
        flags.clone(),
        &w.users,
    )?;
    if !pending.fields().iter().all(|s| s.is_ascii()) {
        return Err(refused());
    }
    let reply = RendererReply {
        generation: pending.generation().into(),
        operation: "ordinary-toLowerCase".into(),
        fields: pending.fields().to_vec(),
        lowered: pending
            .fields()
            .iter()
            .map(|s| s.to_ascii_lowercase())
            .collect(),
    };
    let proof = json!({"assignments":assignments,"fields":reply.fields,"lowered":reply.lowered,"operation":reply.operation});
    let commit = w.directory_members.adopt(
        pending,
        reply,
        &flags,
        &account,
        session,
        epoch,
        "canvas-attribution-fixture",
        &w.users,
    )?;
    w.users = commit.into_parts().1;
    Ok(proof)
}
