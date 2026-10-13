use super::*;
use crate::slack::transport::ListDeleteFailure;
use crate::slack_lists::{
    AcceptedDeletion, AcceptedDeletionState, DeletionIntent, DeletionStateIntent,
    ListRecordReadback,
};

/// Captured B 78M5 -> V z8Uw. The endpoint also deletes associated subtasks;
/// capture the intent only after explicit confirmation of that scope.
pub fn list_record_delete(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    intent: &DeletionIntent,
) -> PreparedRequest {
    client.rest_form(
        workspace,
        "lists.records.delete",
        vec![
            ("list_id", intent.list_id().into()),
            ("id", intent.record_id().into()),
            ("_x_reason", "delete-record".into()),
        ],
    )
}

/// Captured B fetchIndividually -> V lY+n. Unlike a page/projection, this can
/// return the exact record_deleted evidence for the requested item.
pub fn list_record_info(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    list_id: &str,
    record_id: &str,
) -> PreparedRequest {
    client.rest_form(
        workspace,
        "lists.records.info",
        vec![
            ("list_id", list_id.into()),
            ("id", record_id.into()),
            ("include_is_subscribed", "true".into()),
            ("_x_reason", "get-list-record".into()),
        ],
    )
}

pub async fn fetch_list_record_info(
    transport: &Transport,
    client: &SlackClient,
    workspace: &WorkspaceSession,
    list_id: &str,
    record_id: &str,
) -> Result<ListRecordReadback, Error> {
    let response = transport
        .execute_list_record_info(list_record_info(client, workspace, list_id, record_id))
        .await?;
    ListRecordReadback::from_response(list_id, record_id, &response)
        .map_err(|error| Error::Api(error.to_string()))
}

/// Consumes one explicitly confirmed intent after caller-owned fresh reads.
/// Account/transport/visit ownership must be checked by the serial dispatcher
/// immediately before this future is polled. No automatic retry is performed.
#[allow(clippy::too_many_arguments)]
pub async fn delete_list_record(
    transport: &Transport,
    client: &SlackClient,
    workspace: &WorkspaceSession,
    document: &Document,
    page: &RecordsPage,
    allow_lists: Option<&str>,
    personal: Option<&PersonalTaskLists>,
    intent: DeletionIntent,
) -> Result<AcceptedDeletion, ListDeleteFailure> {
    intent
        .revalidate(document, page, allow_lists, personal)
        .map_err(|error| ListDeleteFailure::NotSent(Error::Api(error.to_string())))?;
    let response = transport
        .execute_list_record_delete(list_record_delete(client, workspace, &intent))
        .await?;
    intent
        .validate_response(&response)
        .map_err(|error| ListDeleteFailure::Uncertain(Error::Api(error.to_string())))
}

/// Captured J2lf -> Z4eT: preserve the original ID, never recreate or resend fields.
pub fn list_record_update_deletion(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    intent: &DeletionStateIntent,
) -> PreparedRequest {
    client.rest_form(
        workspace,
        "lists.records.updateDeletion",
        vec![
            ("list_id", intent.list_id().into()),
            ("id", intent.record_id().into()),
            ("deleted", intent.deleted().to_string()),
            ("_x_reason", "update-record-deletion-state".into()),
        ],
    )
}

/// A new explicit Undo/Redo only. Fresh scope ownership must be checked by the
/// caller immediately before polling; this path never waits offline or retries.
#[allow(clippy::too_many_arguments)]
pub async fn update_list_record_deletion(
    transport: &Transport,
    client: &SlackClient,
    workspace: &WorkspaceSession,
    document: &Document,
    page: &RecordsPage,
    readback: &ListRecordReadback,
    allow_lists: Option<&str>,
    personal: Option<&PersonalTaskLists>,
    intent: DeletionStateIntent,
) -> Result<AcceptedDeletionState, ListDeleteFailure> {
    intent
        .revalidate(document, page, readback, allow_lists, personal)
        .map_err(|error| ListDeleteFailure::NotSent(Error::Api(error.to_string())))?;
    let response = transport
        .execute_list_record_update_deletion(list_record_update_deletion(
            client, workspace, &intent,
        ))
        .await?;
    intent
        .validate_response(&response)
        .map_err(|error| ListDeleteFailure::Uncertain(Error::Api(error.to_string())))
}

#[cfg(test)]
mod tests;
