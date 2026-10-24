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

/// Opaque request prepared off the renderer. Caller must recheck current
/// account/source/attempt after preparation, before marking dispatch.
pub struct PreparedCanvasStructureRequest {
    request: PreparedRequest,
    edit: PreparedStructureEdit,
}
impl PreparedCanvasStructureRequest {
    pub fn edit(&self) -> &PreparedStructureEdit {
        &self.edit
    }
}
fn clock() -> Result<u64, CanvasEditFailure> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .and_then(|t| u64::try_from(t.as_micros()).ok())
        .ok_or_else(|| CanvasEditFailure::NotSent(Error::Api("invalid_canvas_edit_clock".into())))
}
pub async fn prepare_canvas_structure_request(
    client: SlackClient,
    workspace: WorkspaceSession,
    fresh: std::sync::Arc<CanvasCollabSnapshot>,
    prepared: PreparedStructureEdit,
) -> Result<PreparedCanvasStructureRequest, CanvasEditFailure> {
    tokio::task::spawn_blocking(move || {
        let request = request(&client, &workspace, &fresh, &prepared, clock()?)
            .map_err(CanvasEditFailure::NotSent)?;
        Ok(PreparedCanvasStructureRequest {
            request,
            edit: prepared,
        })
    })
    .await
    .map_err(|_| {
        CanvasEditFailure::NotSent(Error::Api("canvas_edit_preparation_worker_failed".into()))
    })?
}
/// No preparation await can separate the caller's final fence from transport.
pub async fn execute_prepared_canvas_structure_edit(
    transport: &Transport,
    sealed: PreparedCanvasStructureRequest,
) -> Result<AcceptedStructure, CanvasEditFailure> {
    sealed
        .edit
        .validate_dispatch_expiry(clock()?)
        .map_err(|_| {
            CanvasEditFailure::NotSent(Error::Api("canvas_edit_preflight_changed".into()))
        })?;
    let bytes = transport.execute_canvas_edit(sealed.request).await?;
    validate_worker(sealed.edit, bytes).await
}
async fn validate_worker(
    prepared: PreparedStructureEdit,
    bytes: Vec<u8>,
) -> Result<AcceptedStructure, CanvasEditFailure> {
    tokio::task::spawn_blocking(move || {
        let ack = EditAck::decode(&bytes).map_err(|_| {
            CanvasEditFailure::Uncertain(Error::Api("invalid_canvas_edit_acknowledgement".into()))
        })?;
        prepared.validate_ack(&ack).map_err(|_| {
            CanvasEditFailure::Uncertain(Error::Api(
                "unconfirmed_canvas_edit_acknowledgement".into(),
            ))
        })
    })
    .await
    .map_err(|_| {
        CanvasEditFailure::Uncertain(Error::Api(
            "canvas_edit_acknowledgement_worker_failed".into(),
        ))
    })?
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
    #[tokio::test]
    async fn sealed_structure_request_is_exact_and_offline_is_not_sent() {
        let snapshot = fixture();
        let edit = prepare(&snapshot);
        let expected =
            request(&SlackClient::default(), &workspace(), &snapshot, &edit, 100).unwrap();
        let sealed = prepare_canvas_structure_request(
            SlackClient::default(),
            workspace(),
            std::sync::Arc::new(snapshot),
            edit,
        )
        .await
        .unwrap();
        assert_eq!(sealed.request, expected);
        assert_eq!(sealed.edit().sequence(), 1);
        let transport = Transport::new("fixture-only").unwrap();
        transport.set_health(crate::slack::transport::Health::Offline);
        assert!(matches!(
            execute_prepared_canvas_structure_edit(&transport, sealed).await,
            Err(CanvasEditFailure::NotSent(Error::Offline(_)))
        ));
    }
    #[tokio::test]
    async fn structure_request_worker_refuses_changed_account_and_inventory() {
        for account in [false, true] {
            let mut snapshot = fixture();
            let edit = prepare(&snapshot);
            let mut workspace = workspace();
            if account {
                workspace.user_id = "other".into();
            } else {
                snapshot.sections.pop();
            }
            assert!(matches!(
                prepare_canvas_structure_request(
                    SlackClient::default(),
                    workspace,
                    std::sync::Arc::new(snapshot),
                    edit
                )
                .await,
                Err(CanvasEditFailure::NotSent(_))
            ));
        }
    }
    #[tokio::test]
    async fn structural_ack_worker_success_and_bad_receipts_keep_delivery_classification() {
        use crate::canvas::wire::{EncodedField as F, Section};
        let snapshot = fixture();
        let edit = prepare(&snapshot);
        let update = edit.document_data().message(1).unwrap().unwrap();
        let wire = update
            .message(38)
            .unwrap()
            .unwrap()
            .remove(12)
            .unwrap()
            .replace_fields(3, &[F::uint(3, 1000).unwrap()])
            .unwrap()
            .replace_fields(4, &[F::uint(4, 8_000_000_000_000_000_000).unwrap()])
            .unwrap();
        let mut row = update.clone();
        let position = update.string(8).unwrap().unwrap().trim_end_matches(":temp");
        for f in [
            F::uint(2, 1000).unwrap(),
            F::bytes(6, snapshot.thread_id().as_bytes()).unwrap(),
            F::bytes(7, snapshot.document_id().as_bytes()).unwrap(),
            F::bytes(8, position.as_bytes()).unwrap(),
            F::bytes(21, position.as_bytes()).unwrap(),
            F::message(38, &wire).unwrap(),
        ] {
            row = row
                .merge_partial(&RawMessage::decode(f.raw()).unwrap())
                .unwrap();
        }
        let data = RawMessage::from_fields(&[F::message(1, &row).unwrap()]).unwrap();
        let ack = RawMessage::from_fields(&[F::uint(1, 1).unwrap(), F::message(2, &data).unwrap()])
            .unwrap();
        let result = validate_worker(edit, ack.raw().to_vec()).await.unwrap();
        assert_eq!(result.section(), &Section::from_raw(row).unwrap());
        for bytes in [vec![0xff], vec![], ack.raw().to_vec()] {
            // The valid receipt from another independently generated intent is
            // still uncertain, just like malformed or empty direct responses.
            assert!(matches!(
                validate_worker(prepare(&snapshot), bytes).await,
                Err(CanvasEditFailure::Uncertain(_))
            ));
        }
    }
}
