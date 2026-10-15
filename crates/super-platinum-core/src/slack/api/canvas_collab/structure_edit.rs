//! Same source-backed one-attempt form/transport; structural proof stays in core.
use super::*;
use crate::canvas::{
    edit::structure::{AcceptedStructure, PreparedStructureEdit},
    wire::EditAck,
};
use crate::slack::transport::CanvasEditFailure;

pub async fn execute_canvas_structure_edit(
    transport: &Transport,
    client: &SlackClient,
    workspace: &WorkspaceSession,
    fresh: &CanvasCollabSnapshot,
    prepared: PreparedStructureEdit,
) -> Result<AcceptedStructure, CanvasEditFailure> {
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
    prepared: &PreparedStructureEdit,
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
mod tests {
    use super::*;
    use crate::canvas::edit::{
        structure::{Side, StructureIntent, tests::fixture},
        text::TextDraft,
    };
    use crate::slack::client::RequestBody;
    fn workspace() -> WorkspaceSession {
        WorkspaceSession {
            team_id: "Ttest".into(),
            user_id: "Utest".into(),
            enterprise_id: None,
            name: "Fixture".into(),
            url: "https://fixture.slack.com".into(),
            token: "fixture-only".into(),
        }
    }
    fn prepare(snapshot: &CanvasCollabSnapshot) -> PreparedStructureEdit {
        StructureIntent::insert(snapshot, "item", Side::After, &TextDraft::default(), 100)
            .unwrap()
            .prepare(snapshot, 100)
            .unwrap()
    }
    #[test]
    fn structural_form_uses_exact_owner_title_payload_and_one_attempt() {
        let snapshot = fixture();
        let prepared = prepare(&snapshot);
        let result = request(
            &SlackClient::default(),
            &workspace(),
            &snapshot,
            &prepared,
            100,
        )
        .unwrap();
        assert_eq!(
            url::Url::parse(&result.url).unwrap().path(),
            "/canvas/-/edit-document"
        );
        let RequestBody::Form(fields) = result.body else {
            panic!("form");
        };
        let fields: std::collections::BTreeMap<_, _> = fields.into_iter().collect();
        assert_eq!(fields["retry_count"], "0");
        assert_eq!(fields["sequence"], "1");
        assert_eq!(fields["session"], prepared.session_id());
        assert_eq!(fields["title"], prepared.form_title());
        assert_eq!(
            decode_canvas_base64(&fields["data_binary"]).unwrap(),
            prepared.document_data().raw()
        );
        let mut other = workspace();
        other.user_id = "Uother".into();
        assert!(request(&SlackClient::default(), &other, &snapshot, &prepared, 100).is_err());
        other = workspace();
        other.team_id = "Tother".into();
        assert!(request(&SlackClient::default(), &other, &snapshot, &prepared, 100).is_err());
    }
    #[tokio::test]
    async fn offline_structure_attempt_is_not_sent_and_never_queued_or_replayed() {
        let snapshot = fixture();
        let transport = Transport::new("fixture-only").unwrap();
        transport.set_health(crate::slack::transport::Health::Offline);
        assert!(matches!(
            execute_canvas_structure_edit(
                &transport,
                &SlackClient::default(),
                &workspace(),
                &snapshot,
                prepare(&snapshot)
            )
            .await,
            Err(CanvasEditFailure::NotSent(Error::Offline(_)))
        ));
    }
}
