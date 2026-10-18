use super::*;
use crate::canvas::comments::bootstrap::{BootstrapAccepted, BootstrapIntent};
use crate::slack::transport::CanvasEditFailure;
pub fn canvas_comment_bootstrap(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    intent: &BootstrapIntent,
) -> Result<PreparedRequest, Error> {
    let record = intent.record();
    let scope = record.annotation().scope();
    if scope.workspace != workspace.team_id || scope.user != workspace.user_id {
        return Err(Error::Api("canvas_comment_owner_changed".into()));
    }
    record.validate().map_err(invalid)?;
    let mut request = super::super::conversations_replies(
        client,
        workspace,
        RepliesArgs {
            channel: comments::channel(&scope.file).map_err(invalid)?,
            ts: record.qpc(),
            latest: Some(record.latest().into()),
            limit: Some(28),
            inclusive: true,
            ..Default::default()
        },
    );
    let crate::slack::client::RequestBody::Form(fields) = &mut request.body else {
        unreachable!("replies form")
    };
    fields.push((
        "content_snippet".into(),
        serde_json::to_string(&record.wire_snippet())
            .map_err(|_| Error::Api("invalid_canvas_snippet".into()))?,
    ));
    fields.push(("cached_latest_updates".into(), "{}".into()));
    Ok(request)
}
pub async fn execute_canvas_comment_bootstrap(
    transport: &Transport,
    client: &SlackClient,
    workspace: &WorkspaceSession,
    intent: BootstrapIntent,
) -> Result<BootstrapAccepted, CanvasEditFailure> {
    let request =
        canvas_comment_bootstrap(client, workspace, &intent).map_err(CanvasEditFailure::NotSent)?;
    let bytes = transport.execute_canvas_comment_bootstrap(request).await?;
    let raw = serde_json::from_slice(&bytes).map_err(|_| {
        CanvasEditFailure::Uncertain(Error::Api("invalid_canvas_bootstrap_response".into()))
    })?;
    intent.accept(&raw).map_err(|_| {
        CanvasEditFailure::Uncertain(Error::Api("unconfirmed_canvas_bootstrap".into()))
    })
}
