//! Same bounded no-replay transport as checkbox edits, with text-specific proof.
use super::*;
use crate::canvas::{
    edit::text::{AcceptedText, PreparedTextEdit},
    wire::EditAck,
};
use crate::slack::transport::CanvasEditFailure;

pub async fn execute_canvas_text_edit(
    transport: &Transport,
    client: &SlackClient,
    workspace: &WorkspaceSession,
    fresh: &CanvasCollabSnapshot,
    prepared: PreparedTextEdit,
) -> Result<AcceptedText, CanvasEditFailure> {
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
    prepared: &PreparedTextEdit,
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
    use crate::{
        canvas::edit::{
            EditSession,
            text::{TextDraft, TextIntent},
        },
        slack::client::RequestBody,
    };
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
    fn prepared(snapshot: &CanvasCollabSnapshot) -> PreparedTextEdit {
        TextIntent::capture(snapshot, "item", 100)
            .unwrap()
            .prepare(
                snapshot,
                &mut EditSession::new(),
                &TextDraft::from_rtml("Changed <b>paragraph</b>").unwrap(),
                100,
            )
            .unwrap()
    }
    #[test]
    fn text_form_uses_the_bound_binary_title_and_exact_one_attempt_envelope() {
        let snapshot = crate::canvas::edit::text::tests::fixture();
        let prepared = prepared(&snapshot);
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
            panic!("form")
        };
        let fields = fields
            .into_iter()
            .collect::<std::collections::BTreeMap<_, _>>();
        assert_eq!(fields["retry_count"], "0");
        assert_eq!(fields["sequence"], "1");
        assert_eq!(fields["session"], prepared.session_id());
        assert_eq!(fields["title"], prepared.form_title());
        assert_eq!(
            decode_canvas_base64(&fields["data_binary"]).unwrap(),
            prepared.document_data().raw()
        );
        let mut other = workspace();
        other.user_id = "Other".into();
        assert!(request(&SlackClient::default(), &other, &snapshot, &prepared, 100).is_err());
    }
    #[test]
    fn formatted_canonical_title_is_decoded_in_the_actual_text_request_form() {
        let snapshot = crate::canvas::edit::text::tests::formatted_title::fixture_with_title(
            " <b>Plan &amp; Review</b> <i>&lt;draft&gt;</i> ",
        );
        let original = snapshot.sections.clone();
        let prepared = prepared(&snapshot);
        let result = request(
            &SlackClient::default(),
            &workspace(),
            &snapshot,
            &prepared,
            100,
        )
        .unwrap();
        let RequestBody::Form(fields) = result.body else {
            panic!("form")
        };
        let fields = fields
            .into_iter()
            .collect::<std::collections::BTreeMap<_, _>>();
        assert_eq!(fields["title"], "Plan & Review <draft>");
        assert_eq!(fields["retry_count"], "0");
        assert_eq!(fields["sequence"], "1");
        let binary = decode_canvas_base64(&fields["data_binary"]).unwrap();
        let data = RawMessage::decode(&binary).unwrap();
        assert_eq!(
            data.fields().iter().map(|f| f.tag()).collect::<Vec<_>>(),
            [1, 14]
        );
        let updates = data.messages(1).unwrap();
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].string(1).unwrap(), Some("item"));
        assert_eq!(
            updates[0]
                .message(12)
                .unwrap()
                .unwrap()
                .message(1)
                .unwrap()
                .unwrap()
                .string(1)
                .unwrap(),
            Some("Changed <b>paragraph</b>"),
        );
        let leaf = RawMessage::from_fields(&[
            EncodedField::bytes(1, b"text").unwrap(),
            EncodedField::bytes(2, b" Plan & Review <draft> ").unwrap(),
        ])
        .unwrap();
        let group = RawMessage::from_fields(&[
            EncodedField::bytes(1, b"rich_text_section").unwrap(),
            EncodedField::message(2, &leaf).unwrap(),
        ])
        .unwrap();
        let expected = RawMessage::from_fields(&[
            EncodedField::bytes(1, b"rich_text").unwrap(),
            EncodedField::message(2, &group).unwrap(),
        ])
        .unwrap();
        assert_eq!(data.message(14).unwrap().unwrap(), expected);
        assert_eq!(snapshot.sections, original);
        let replaced = crate::canvas::edit::text::tests::formatted_title::fixture_with_title(
            " <i>Plan &#38; Review</i> <b>&lt;draft&gt;</b> ",
        );
        assert!(
            request(
                &SlackClient::default(),
                &workspace(),
                &replaced,
                &prepared,
                100
            )
            .is_err()
        );
    }

    #[tokio::test]
    async fn known_offline_consumes_text_preparation_without_holding_or_retrying() {
        let snapshot = crate::canvas::edit::text::tests::fixture();
        let prepared = prepared(&snapshot);
        let transport = Transport::new("fixture-only").unwrap();
        transport.set_health(crate::slack::transport::Health::Offline);
        assert!(matches!(
            execute_canvas_text_edit(
                &transport,
                &SlackClient::default(),
                &workspace(),
                &snapshot,
                prepared
            )
            .await,
            Err(CanvasEditFailure::NotSent(Error::Offline(_)))
        ));
    }
}
