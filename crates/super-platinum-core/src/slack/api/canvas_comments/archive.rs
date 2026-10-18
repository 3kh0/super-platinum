//! Source-proven channel_id/thread_ts mutation, not a public canvases API guess.
use super::*;
use crate::canvas::comments::archive::{ArchiveAccepted, ArchiveIntent};
use crate::slack::transport::CanvasEditFailure;

pub fn canvas_comment_archive(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    intent: &ArchiveIntent,
) -> Result<PreparedRequest, Error> {
    Ok(client.rest_form(
        workspace,
        intent.method(),
        vec![
            (
                "channel_id",
                comments::channel(intent.file()).map_err(invalid)?,
            ),
            ("thread_ts", intent.root_ts().into()),
        ],
    ))
}
pub async fn execute_canvas_comment_archive(
    transport: &Transport,
    client: &SlackClient,
    workspace: &WorkspaceSession,
    intent: ArchiveIntent,
) -> Result<ArchiveAccepted, CanvasEditFailure> {
    let request =
        canvas_comment_archive(client, workspace, &intent).map_err(CanvasEditFailure::NotSent)?;
    let bytes = transport.execute_canvas_comment_archive(request).await?;
    let response = serde_json::from_slice(&bytes).map_err(|_| {
        CanvasEditFailure::Uncertain(Error::Api("invalid_canvas_comment_response".into()))
    })?;
    intent.accept(&response).map_err(|_| {
        CanvasEditFailure::Uncertain(Error::Api("unconfirmed_canvas_comment_write".into()))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn resolved_state_selects_exact_method_and_only_channel_and_timestamp_fields() {
        let client = SlackClient::default();
        let workspace = WorkspaceSession {
            team_id: "T1".into(),
            enterprise_id: None,
            user_id: "U1".into(),
            name: "Fixture".into(),
            url: "https://fixture.slack.com".into(),
            token: "fixture-only".into(),
        };
        let file = serde_json::from_value(json!({"id":"F123","filetype":"quip","access":"write"}))
            .unwrap();
        for (archived, method) in [
            (false, "quip.thread.archive"),
            (true, "quip.thread.unarchive"),
        ] {
            let root=serde_json::from_value(json!({"type":"message","ts":"1700000000.000100","subtype":"document_comment_root","reply_count":1,"document_comment":{"thread_id":"thread-1","is_archived":archived}})).unwrap();
            let intent = ArchiveIntent::capture(&file, &root).unwrap();
            let request = canvas_comment_archive(&client, &workspace, &intent).unwrap();
            assert_eq!(
                url::Url::parse(&request.url).unwrap().path(),
                format!("/api/{method}")
            );
            let crate::slack::client::RequestBody::Form(fields) = request.body else {
                panic!("form");
            };
            assert_eq!(fields.len(), 3);
            assert!(fields.contains(&("token".into(), "fixture-only".into())));
            assert!(fields.contains(&("channel_id".into(), "C123".into())));
            assert!(fields.contains(&("thread_ts".into(), "1700000000.000100".into())));
        }
    }
}
