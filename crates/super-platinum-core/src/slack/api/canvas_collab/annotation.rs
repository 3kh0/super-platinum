//! Same existing one-attempt Canvas transport; no retry or reconstructed intent.
use super::*;
use crate::canvas::edit::annotation::{AcknowledgedAnnotation, PreparedAnnotation};
use crate::canvas::wire::EditAck;
use crate::slack::transport::CanvasEditFailure;

/// Caller must first durably record the exact prepared annotation. The consumed
/// value cannot be replayed after an ambiguous transport or acknowledgement.
pub async fn execute_canvas_annotation(
    transport: &Transport,
    client: &SlackClient,
    workspace: &WorkspaceSession,
    fresh: &CanvasCollabSnapshot,
    prepared: PreparedAnnotation,
) -> Result<AcknowledgedAnnotation, CanvasEditFailure> {
    let request = canvas_annotation(client, workspace, fresh, &prepared)
        .map_err(CanvasEditFailure::NotSent)?;
    let bytes = transport.execute_canvas_edit(request).await?;
    let ack = EditAck::decode(&bytes).map_err(|_| {
        CanvasEditFailure::Uncertain(Error::Api(
            "invalid_canvas_annotation_acknowledgement".into(),
        ))
    })?;
    prepared.validate_ack(&ack).map_err(|_| {
        CanvasEditFailure::Uncertain(Error::Api(
            "unconfirmed_canvas_annotation_acknowledgement".into(),
        ))
    })
}
pub fn canvas_annotation(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    fresh: &CanvasCollabSnapshot,
    prepared: &PreparedAnnotation,
) -> Result<PreparedRequest, Error> {
    if prepared.workspace_id() != workspace.team_id || prepared.user_id() != workspace.user_id {
        return Err(Error::Api("canvas_annotation_account_mismatch".into()));
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .and_then(|duration| u64::try_from(duration.as_micros()).ok())
        .ok_or_else(|| Error::Api("invalid_canvas_annotation_clock".into()))?;
    prepared
        .validate_snapshot(fresh, now)
        .map_err(|_| Error::Api("canvas_annotation_preflight_changed".into()))?;
    super::edit::form(
        client,
        workspace,
        fresh,
        prepared.thread_id(),
        prepared.document_id(),
        prepared.session_id(),
        1,
        prepared.document_data(),
        prepared.form_title(),
    )
}
