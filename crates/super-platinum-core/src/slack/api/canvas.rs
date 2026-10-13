//! Desktop sessions use files.info + its authenticated HTML export. Public
//! canvases.getContent rejects xoxc sessions (verified with a private test canvas).
use super::super::models::File;
use super::*;
use crate::canvas::{CanvasDocument, content_url};

pub fn canvas_info(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    id: &str,
) -> PreparedRequest {
    client.rest_form(workspace, "files.info", vec![("file", id.into())])
}

pub async fn fetch_canvas_file(
    transport: &Transport,
    client: &SlackClient,
    workspace: &WorkspaceSession,
    id: &str,
) -> Result<File, Error> {
    let response = transport
        .execute(canvas_info(client, workspace, id))
        .await?;
    let file: File = decode(
        response
            .get("file")
            .cloned()
            .ok_or_else(|| Error::Api("canvas_file_missing".into()))?,
        "canvas file",
    )?;
    if file.id.as_deref() != Some(id)
        || !matches!(file.filetype.as_deref(), Some("quip" | "canvas"))
    {
        return Err(Error::Api("invalid_canvas_file".into()));
    }
    Ok(file)
}

/// Only title is sent: a rename cannot replace the document body or sharing.
pub fn canvas_rename(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    id: &str,
    title: &str,
) -> Result<PreparedRequest, Error> {
    if title.trim().is_empty() {
        return Err(Error::Api("canvas_title_empty".into()));
    }
    Ok(client.rest_form(
        workspace,
        "files.edit",
        vec![("file", id.into()), ("title", title.trim().into())],
    ))
}

pub fn validate_canvas_rename(file: &File, expected_title: &str) -> Result<(), Error> {
    if file
        .extra
        .get("editable")
        .and_then(serde_json::Value::as_bool)
        != Some(true)
    {
        return Err(Error::Api("canvas_not_editable".into()));
    }
    if crate::state::file_title(file) != expected_title {
        return Err(Error::Api("canvas_title_changed".into()));
    }
    Ok(())
}

pub async fn fetch_canvas(
    transport: &Transport,
    client: &SlackClient,
    workspace: &WorkspaceSession,
    id: &str,
) -> Result<(File, CanvasDocument), Error> {
    let file = fetch_canvas_file(transport, client, workspace, id).await?;
    let url = file
        .url_private
        .as_deref()
        .filter(|url| content_url(url))
        .ok_or_else(|| Error::Api("canvas_content_unavailable".into()))?;
    let bytes = transport.get_bytes(url, "Mozilla/5.0").await?;
    if bytes.len() > crate::canvas::MAX_CANVAS_BYTES {
        return Err(Error::Api("canvas_too_large".into()));
    }
    let html =
        String::from_utf8(bytes).map_err(|_| Error::Api("canvas_encoding_invalid".into()))?;
    // HTML parsing never runs on a renderer's event loop.
    let document = tokio::task::spawn_blocking(move || CanvasDocument::parse(&html))
        .await
        .map_err(|_| Error::Api("canvas_parse_failed".into()))?
        .map_err(|e| Error::Api(e.into()))?;
    Ok((file, document))
}

/// Creates a standalone canvas. No conversation or sharing parameters are sent.
/// This mutation has no idempotency key and must not be automatically retried.
pub fn canvas_create(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    title: &str,
    markdown: &str,
) -> Result<PreparedRequest, Error> {
    if title.trim().is_empty() {
        return Err(Error::Api("canvas_title_empty".into()));
    }
    if markdown.len() > crate::canvas::MAX_CANVAS_BYTES {
        return Err(Error::Api("canvas_too_large".into()));
    }
    Ok(client.rest_form(
        workspace,
        "files.createCanvas",
        vec![
            ("title", title.trim().into()),
            ("markdown", markdown.into()),
        ],
    ))
}

pub fn created_canvas_id(response: &serde_json::Value) -> Result<String, Error> {
    response
        .get("file_id")
        .and_then(serde_json::Value::as_str)
        .filter(|id| {
            id.starts_with('F') && id.len() > 1 && id.bytes().all(|b| b.is_ascii_alphanumeric())
        })
        .map(str::to_owned)
        .ok_or_else(|| Error::Api("canvas_created_id_missing".into()))
}
