//! Consume a sealed checkbox preparation in one source-backed canvas save.
use super::*;
use crate::canvas::{
    edit::{AcceptedCheckbox, PreparedCheckboxEdit},
    wire::EditAck,
};
use crate::slack::transport::CanvasEditFailure;
use std::time::{SystemTime, UNIX_EPOCH};

/// The prepared value is consumed even on failure. A caller must re-read and
/// create a new explicit intent rather than replaying an uncertain request.
pub async fn execute_canvas_checkbox_edit(
    transport: &Transport,
    client: &SlackClient,
    workspace: &WorkspaceSession,
    fresh: &CanvasCollabSnapshot,
    prepared: PreparedCheckboxEdit,
) -> Result<AcceptedCheckbox, CanvasEditFailure> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| u64::try_from(duration.as_micros()).ok())
        .ok_or_else(|| {
            CanvasEditFailure::NotSent(Error::Api("invalid_canvas_edit_clock".into()))
        })?;
    let request =
        request(client, workspace, fresh, &prepared, now).map_err(CanvasEditFailure::NotSent)?;
    let bytes = transport.execute_canvas_edit(request).await?;
    accepted(&prepared, &bytes)
}

fn request(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    fresh: &CanvasCollabSnapshot,
    prepared: &PreparedCheckboxEdit,
    now_usec: u64,
) -> Result<PreparedRequest, Error> {
    if prepared.workspace_id() != workspace.team_id
        || prepared.user_id() != workspace.user_id
        || fresh.init.workspace_id() != workspace.team_id
        || fresh.init.options().string(33).map_err(wire_error)? != Some(workspace.user_id.as_str())
    {
        return Err(Error::Api("canvas_edit_account_mismatch".into()));
    }
    prepared
        .validate_snapshot(fresh, now_usec)
        .map_err(|_| Error::Api("canvas_edit_preflight_changed".into()))?;
    form(
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

#[allow(clippy::too_many_arguments)]
pub(super) fn form(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    fresh: &CanvasCollabSnapshot,
    thread: &str,
    document: &str,
    session: &str,
    sequence: u64,
    data: &RawMessage,
    title: &str,
) -> Result<PreparedRequest, Error> {
    // Thread.secret_path106 is a string with default "" in the captured
    // descriptor; getSavePayloadAndOptions uses getSecretPathOrDefault().
    let secret_path = fresh.thread.string(106).map_err(wire_error)?.unwrap_or("");
    let mut request = client.rest_form(
        workspace,
        "",
        vec![
            ("thread", thread.into()),
            ("document", document.into()),
            ("retry_count", "0".into()),
            ("secret_path", secret_path.into()),
            ("session", session.into()),
            ("sequence", sequence.to_string()),
            // Source encodeBody calls String(value); absent native navigation and
            // search sessions correspond to the observed literal null values.
            ("nav_action_id", "null".into()),
            ("search_session_id", "null".into()),
            ("data_binary", encode_canvas_base64(data.raw())),
            ("title", title.into()),
        ],
    );
    let mut url = url::Url::parse(&request.url).map_err(|_| invalid())?;
    if url.scheme() != "https"
        || !url
            .host_str()
            .is_some_and(|host| host.ends_with(".slack.com"))
        || url.port_or_known_default() != Some(443)
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(Error::Api("invalid_canvas_edit_origin".into()));
    }
    let version = url
        .query_pairs()
        .find(|(key, _)| key == "_x_version_ts")
        .map(|(_, value)| value.into_owned())
        .ok_or_else(invalid)?;
    url.set_path("/canvas/-/edit-document");
    url.set_query(None);
    url.query_pairs_mut().append_pair("_x_version_ts", &version);
    request.url = url.into();
    Ok(request)
}

fn accepted(
    prepared: &PreparedCheckboxEdit,
    bytes: &[u8],
) -> Result<AcceptedCheckbox, CanvasEditFailure> {
    let acknowledgement = EditAck::decode(bytes).map_err(|_| {
        CanvasEditFailure::Uncertain(Error::Api("invalid_canvas_edit_acknowledgement".into()))
    })?;
    prepared.validate_ack(&acknowledgement).map_err(|_| {
        CanvasEditFailure::Uncertain(Error::Api("unconfirmed_canvas_edit_acknowledgement".into()))
    })
}

#[cfg(test)]
mod tests;
