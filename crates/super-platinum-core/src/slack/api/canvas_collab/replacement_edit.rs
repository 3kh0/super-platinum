//! Same source-backed one-attempt form/transport; structural proof stays in core.
use super::*;
use crate::canvas::{
    edit::structure::{AcceptedReplacement, PreparedReplacement},
    wire::EditAck,
};
use crate::slack::transport::CanvasEditFailure;

pub async fn execute_canvas_replacement_edit(
    transport: &Transport,
    client: &SlackClient,
    workspace: &WorkspaceSession,
    fresh: &CanvasCollabSnapshot,
    prepared: PreparedReplacement,
) -> Result<AcceptedReplacement, CanvasEditFailure> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .and_then(|t| u64::try_from(t.as_micros()).ok())
        .ok_or_else(|| {
            CanvasEditFailure::NotSent(Error::Api("invalid_canvas_edit_clock".into()))
        })?;
    let request =
        request(client, workspace, fresh, &prepared, now).map_err(CanvasEditFailure::NotSent)?;
    let bytes = transport.execute_canvas_edit(request).await?;
    let ack = EditAck::decode(&bytes).map_err(|_| {
        CanvasEditFailure::Uncertain(Error::Api("invalid_canvas_edit_acknowledgement".into()))
    })?;
    prepared.validate_ack(&ack).map_err(|_| {
        CanvasEditFailure::Uncertain(Error::Api("unconfirmed_canvas_edit_acknowledgement".into()))
    })
}

fn request(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    fresh: &CanvasCollabSnapshot,
    prepared: &PreparedReplacement,
    now: u64,
) -> Result<PreparedRequest, Error> {
    if prepared.workspace_id() != workspace.team_id
        || prepared.user_id() != workspace.user_id
        || fresh.init.workspace_id() != workspace.team_id
        || fresh.init.options().string(33).map_err(wire_error)? != Some(workspace.user_id.as_str())
    {
        return Err(Error::Api("canvas_edit_account_mismatch".into()));
    }
    prepared
        .validate_snapshot(fresh, now)
        .map_err(|_| Error::Api("canvas_edit_preflight_changed".into()))?;
    super::edit::form(
        client,
        workspace,
        fresh,
        prepared.thread_id(),
        prepared.document_id(),
        prepared.session_id(),
        prepared.sequence(),
        prepared.document_data(),
        prepared.form_title(),
    )
}

#[cfg(test)]
mod tests;
