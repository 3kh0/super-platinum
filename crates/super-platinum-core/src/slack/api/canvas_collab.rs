//! Canonical, read-only Canvas collaboration snapshots. HTML exports cannot
//! supply section sequences, permission state, or collaboration metadata.
use super::super::models::File;
use super::*;
use crate::canvas::wire::{self, EncodedField, LoadDataResponse, RawMessage, Section};
use serde_json::Value;
use std::collections::HashSet;

mod annotation;
pub use annotation::{canvas_annotation, execute_canvas_annotation};
mod edit;
mod replacement_edit;
mod structure_edit;
pub use replacement_edit::execute_canvas_replacement_edit;
mod end_edit;
mod split_edit;
pub use end_edit::execute_canvas_end_edit;
mod text_edit;
pub use split_edit::execute_canvas_split_edit;
pub use structure_edit::{
    PreparedCanvasStructureRequest, execute_canvas_structure_edit,
    execute_prepared_canvas_structure_edit, prepare_canvas_structure_request,
};
pub use text_edit::{
    PreparedCanvasTextRequest, execute_canvas_text_edit, execute_prepared_canvas_text_edit,
    prepare_canvas_text_request,
};
mod encoding;
pub use edit::execute_canvas_checkbox_edit;
#[cfg(test)]
mod tests;

pub(crate) use encoding::{decode as decode_canvas_base64, encode as encode_canvas_base64};

fn invalid() -> Error {
    Error::Api("invalid_canvas_collab_snapshot".into())
}
fn wire_error(_: wire::Error) -> Error {
    Error::Api("invalid_canvas_collab_protobuf".into())
}
fn identity(value: Option<&str>) -> Result<&str, Error> {
    value
        .filter(|v| {
            !v.is_empty()
                && v.len() <= 256
                && v.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        })
        .ok_or_else(invalid)
}

/// Private initialization metadata; intentionally does not implement Debug.
#[derive(Clone)]
pub struct CanvasControllerInit {
    options: RawMessage,
    initializer: Value,
    quip_user_id: String,
    workspace_id: String,
}
impl CanvasControllerInit {
    pub fn parse(response: Value, workspace: &WorkspaceSession) -> Result<Self, Error> {
        if response.get("ok").and_then(Value::as_bool) != Some(true) {
            return Err(invalid());
        }
        let encoded = response
            .get("init_options")
            .and_then(Value::as_str)
            .ok_or_else(invalid)?;
        let options = RawMessage::decode(&decode_canvas_base64(encoded)?).map_err(wire_error)?;
        let user = identity(response.get("quip_user_id").and_then(Value::as_str))?;
        if options.string(1).map_err(wire_error)? != Some(user)
            || options.string(33).map_err(wire_error)? != Some(workspace.user_id.as_str())
            || options.string(34).map_err(wire_error)? != Some(workspace.team_id.as_str())
        {
            return Err(Error::Api("canvas_collab_account_mismatch".into()));
        }
        Ok(Self {
            options,
            initializer: response
                .get("js_initializer")
                .cloned()
                .unwrap_or(Value::Null),
            quip_user_id: user.to_owned(),
            workspace_id: workspace.team_id.clone(),
        })
    }
    pub fn options(&self) -> &RawMessage {
        &self.options
    }
    pub fn initializer(&self) -> &Value {
        &self.initializer
    }
    pub fn quip_user_id(&self) -> &str {
        &self.quip_user_id
    }
    pub fn workspace_id(&self) -> &str {
        &self.workspace_id
    }
}

/// A validated full bulk-load result, not permission to perform an edit.
/// Unknown canonical fields and initialization metadata remain available to a
/// separate source-backed intent validator. No Debug can expose private data.
#[derive(Clone)]
pub struct CanvasCollabSnapshot {
    pub file: File,
    pub init: CanvasControllerInit,
    pub thread: RawMessage,
    pub document: RawMessage,
    pub sections: Vec<Section>,
    pub load: LoadDataResponse,
    pub(crate) replacement_provenance:
        Option<std::sync::Arc<crate::canvas::edit::structure::ReplacementProvenance>>,
    file_id: String,
    thread_id: String,
    document_id: String,
}
impl CanvasCollabSnapshot {
    pub fn from_responses(
        file: File,
        init: CanvasControllerInit,
        load: LoadDataResponse,
    ) -> Result<Self, Error> {
        let file_id = identity(file.id.as_deref())?.to_owned();
        if !matches!(file.filetype.as_deref(), Some("quip" | "canvas")) {
            return Err(invalid());
        }
        let thread_id =
            identity(file.extra.get("quip_thread_id").and_then(Value::as_str))?.to_owned();
        if load.error().map_err(wire_error)?.is_some()
            || load.success().map_err(wire_error)? != Some(true)
        {
            return Err(Error::Api("canvas_collab_load_rejected".into()));
        }
        let response = load.response().ok_or_else(invalid)?;
        if response.has(4)
            || response.has(6)
            || response.boolean(9).map_err(wire_error)? == Some(true)
        {
            return Err(Error::Api("canvas_collab_load_incomplete".into()));
        }
        let payload = load.payload().map_err(wire_error)?.ok_or_else(invalid)?;
        let thread = one_matching(payload.messages(3).map_err(wire_error)?, &thread_id)?;
        if thread.boolean(3).map_err(wire_error)? == Some(true) {
            return Err(invalid());
        }
        let document_id = identity(thread.string(10).map_err(wire_error)?)?.to_owned();
        let document = one_matching(payload.messages(6).map_err(wire_error)?, &document_id)?;
        if document.string(6).map_err(wire_error)? != Some(thread_id.as_str())
            || document.boolean(3).map_err(wire_error)? == Some(true)
            || thread.uint(2).map_err(wire_error)?.is_none()
            || document.uint(2).map_err(wire_error)?.is_none()
        {
            return Err(invalid());
        }
        let sections = load.sections().map_err(wire_error)?;
        for section in &sections {
            if section.document_id().map_err(wire_error)? != Some(document_id.as_str())
                || section.thread_id().map_err(wire_error)? != Some(thread_id.as_str())
                || section.sequence().map_err(wire_error)?.is_none()
            {
                return Err(invalid());
            }
        }
        // A full canonical Section index must account for every non-deleted
        // section. Do not treat pagination/followup or missing objects as a
        // writable empty document. The captured complete index is chunked=true;
        // that flag alone does not signify an incomplete response.
        let index_name = format!("Section/{document_id}");
        let mut matching = Vec::new();
        for index in response.messages(3).map_err(wire_error)? {
            if index.string(1).map_err(wire_error)? == Some(index_name.as_str()) {
                matching.push(index);
            }
        }
        if matching.len() != 1 {
            return Err(Error::Api("canvas_collab_load_incomplete".into()));
        }
        let mut indexed = HashSet::new();
        for entry in matching[0].messages(2).map_err(wire_error)? {
            let id = entry
                .string(1)
                .map_err(wire_error)?
                .filter(|id| !id.is_empty() && id.len() <= 256 && !id.chars().any(char::is_control))
                .ok_or_else(invalid)?
                .to_owned();
            if !indexed.insert(id) {
                return Err(invalid());
            }
        }
        let mut present = HashSet::new();
        for section in &sections {
            if section.deleted().map_err(wire_error)? != Some(true) {
                present.insert(section.id().to_owned());
            }
        }
        if indexed != present {
            return Err(Error::Api("canvas_collab_load_incomplete".into()));
        }
        Ok(Self {
            file,
            init,
            thread,
            document,
            sections,
            load,
            replacement_provenance: None,
            file_id,
            thread_id,
            document_id,
        })
    }
    pub fn file_id(&self) -> &str {
        &self.file_id
    }
    pub fn thread_id(&self) -> &str {
        &self.thread_id
    }
    pub fn document_id(&self) -> &str {
        &self.document_id
    }
    pub fn section(&self, id: &str) -> Option<&Section> {
        self.sections.iter().find(|section| section.id() == id)
    }
}

fn one_matching(messages: Vec<RawMessage>, id: &str) -> Result<RawMessage, Error> {
    let mut found = None;
    for message in messages {
        if message.string(1).map_err(wire_error)? == Some(id) {
            if found.is_some() {
                return Err(invalid());
            }
            found = Some(message);
        }
    }
    found.ok_or_else(invalid)
}

pub fn canvas_controller_init(
    client: &SlackClient,
    workspace: &WorkspaceSession,
) -> PreparedRequest {
    client.rest_form(
        workspace,
        "quip.getControllerInitData",
        vec![("_x_reason", "canvas-controller-init".into())],
    )
}

/// Q 194xxx explicitly supports load-data without a user-action tracer. This
/// sends the captured object + THREAD bulk load and omits UI tracing fields.
pub fn canvas_collab_load_request(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    thread_id: &str,
) -> Result<PreparedRequest, Error> {
    identity(Some(thread_id))?;
    let bulk = RawMessage::from_fields(&[
        EncodedField::uint(1, 1).map_err(wire_error)?, // BulkLoad.Type.THREAD
        EncodedField::bytes(2, thread_id.as_bytes()).map_err(wire_error)?,
    ])
    .map_err(wire_error)?;
    let request = RawMessage::from_fields(&[
        EncodedField::bytes(1, thread_id.as_bytes()).map_err(wire_error)?,
        EncodedField::message(3, &bulk).map_err(wire_error)?,
    ])
    .map_err(wire_error)?;
    let mut prepared = client.rest_form(
        workspace,
        "",
        vec![("request_binary", encode_canvas_base64(request.raw()))],
    );
    let mut url = url::Url::parse(&prepared.url).map_err(|_| invalid())?;
    if url.scheme() != "https"
        || !url
            .host_str()
            .is_some_and(|host| host.ends_with(".slack.com"))
        || url.port_or_known_default() != Some(443)
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(invalid());
    }
    let version = url
        .query_pairs()
        .find(|(key, _)| key == "_x_version_ts")
        .map(|(_, value)| value.into_owned())
        .ok_or_else(invalid)?;
    url.set_path("/canvas/-/load-data");
    url.set_query(None);
    url.query_pairs_mut().append_pair("_x_version_ts", &version);
    prepared.url = url.into();
    Ok(prepared)
}

pub async fn fetch_canvas_collab(
    transport: &Transport,
    client: &SlackClient,
    workspace: &WorkspaceSession,
    file_id: &str,
) -> Result<CanvasCollabSnapshot, Error> {
    identity(Some(file_id))?;
    let file = fetch_canvas_file(transport, client, workspace, file_id).await?;
    let thread_id = identity(file.extra.get("quip_thread_id").and_then(Value::as_str))?;
    let bytes = transport
        .execute_canvas_collab_read(canvas_controller_init(client, workspace))
        .await?;
    let response: Value = serde_json::from_slice(&bytes)
        .map_err(|_| Error::Api("invalid_canvas_controller_init".into()))?;
    let init = CanvasControllerInit::parse(response, workspace)?;
    let bytes = transport
        .execute_canvas_collab_read(canvas_collab_load_request(client, workspace, thread_id)?)
        .await?;
    let load = LoadDataResponse::decode(&bytes).map_err(wire_error)?;
    CanvasCollabSnapshot::from_responses(file, init, load)
}

#[cfg(test)]
pub(crate) use tests::canvas_collab_test_snapshot;

mod start_typing_edit;
pub use start_typing_edit::execute_canvas_start_typing_edit;
