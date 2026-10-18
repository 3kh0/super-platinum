use super::*;
use crate::canvas::comments::reply::{ReplyAccepted, ReplyIntent};
use crate::slack::transport::CanvasEditFailure;
pub fn canvas_comment_reply(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    intent: &ReplyIntent,
) -> Result<PreparedRequest, Error> {
    let record = intent.record();
    if record.workspace() != workspace.team_id || record.user() != workspace.user_id {
        return Err(Error::Api("canvas_comment_owner_changed".into()));
    }
    let mut request = super::super::chat_post_message(
        client,
        workspace,
        comments::channel(record.file()).map_err(invalid)?,
        record.text().into(),
        Some(record.root_ts().into()),
    );
    let crate::slack::client::RequestBody::Form(fields) = &mut request.body else {
        unreachable!("chat form")
    };
    if let Some(blocks) = record.blocks() {
        fields.push((
            "blocks".into(),
            serde_json::to_string(blocks)
                .map_err(|_| Error::Api("invalid_canvas_reply_content".into()))?,
        ));
    }
    fields.push(("client_msg_id".into(), record.client_msg_id().into()));
    fields.push(("reply_broadcast".into(), "false".into()));
    Ok(request)
}
pub async fn execute_canvas_comment_reply(
    transport: &Transport,
    client: &SlackClient,
    workspace: &WorkspaceSession,
    intent: ReplyIntent,
) -> Result<ReplyAccepted, CanvasEditFailure> {
    let request =
        canvas_comment_reply(client, workspace, &intent).map_err(CanvasEditFailure::NotSent)?;
    let bytes = transport.execute_canvas_comment_reply(request).await?;
    let raw = serde_json::from_slice(&bytes).map_err(|_| {
        CanvasEditFailure::Uncertain(Error::Api("invalid_canvas_reply_response".into()))
    })?;
    intent
        .accept(&raw)
        .map_err(|_| CanvasEditFailure::Uncertain(Error::Api("unconfirmed_canvas_reply".into())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn exact_existing_root_payload_and_owner_refusal() {
        let client = SlackClient::default();
        let mut workspace = WorkspaceSession {
            team_id: "T1".into(),
            enterprise_id: None,
            user_id: "U1".into(),
            name: "Fixture".into(),
            url: "https://fixture.slack.com".into(),
            token: "fixture-only".into(),
        };
        let file =
            serde_json::from_value(json!({"id":"F123","filetype":"quip","access":"comment"}))
                .unwrap();
        let root=serde_json::from_value(json!({"type":"message","ts":"1700000000.000100","subtype":"document_comment_root","document_comment":{"thread_id":"quip-root"}})).unwrap();
        let id = uuid::Uuid::new_v4().to_string();
        let intent =
            ReplyIntent::capture("T1", "U1", &file, &root, "Exact \u{65e5}\nreply", &id).unwrap();
        let request = canvas_comment_reply(&client, &workspace, &intent).unwrap();
        assert_eq!(request.method, "POST");
        assert_eq!(
            url::Url::parse(&request.url).unwrap().path(),
            "/api/chat.postMessage"
        );
        let crate::slack::client::RequestBody::Form(fields) = request.body else {
            panic!("form")
        };
        assert_eq!(fields.len(), 6);
        let map = fields
            .into_iter()
            .collect::<std::collections::BTreeMap<_, _>>();
        assert_eq!(map.len(), 6);
        assert_eq!(map["channel"], "C123");
        assert_eq!(map["thread_ts"], "1700000000.000100");
        assert_eq!(map["client_msg_id"], id);
        assert_eq!(map["reply_broadcast"], "false");
        assert_eq!(map["text"], intent.record().text());
        assert_eq!(map["token"], "fixture-only");
        workspace.user_id = "U2".into();
        assert!(canvas_comment_reply(&client, &workspace, &intent).is_err());
        workspace.user_id = "U1".into();
        workspace.team_id = "T2".into();
        assert!(canvas_comment_reply(&client, &workspace, &intent).is_err());
    }
}
