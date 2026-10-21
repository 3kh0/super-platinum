//! Synthetic original read receipts, decoded by the ordinary Canvas parsers.
use super::*;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use super_platinum_core::{
    canvas::{
        CanvasDocument,
        wire::{EncodedField as F, LoadDataResponse, RawMessage},
    },
    slack::{
        api::{CanvasCollabSnapshot, CanvasControllerInit},
        models::File,
    },
};
const THREAD: &str = "navigation-thread";
const DOCUMENT: &str = "navigation-document";
const EXPORT: &str = "https://files.slack.com/files-tmb/FLISTFILE1/navigation.html";
const HTML: &str = "<div class='quip-canvas-content'><h1>Alex Canvas</h1><p>Native target Canvas content</p><p>The original List draft remains saved.</p></div>";
fn msg(fields: &[F]) -> RawMessage {
    RawMessage::from_fields(fields).unwrap()
}
fn s(tag: u32, value: &str) -> F {
    F::bytes(tag, value.as_bytes()).unwrap()
}
fn n(tag: u32, value: u64) -> F {
    F::uint(tag, value).unwrap()
}
fn m(tag: u32, value: &RawMessage) -> F {
    F::message(tag, value).unwrap()
}
fn responses(owner: &WorkspaceSession) -> Result<(Value, Value, Vec<u8>), Error> {
    let mut file = source_file(CANVAS)?;
    file["quip_thread_id"] = json!(THREAD);
    file["editable"] = json!(false);
    file["url_private"] = json!(EXPORT);
    let options = msg(&[
        s(1, "navigation-quip-user"),
        s(33, &owner.user_id),
        s(34, &owner.team_id),
    ]);
    let init = json!({"ok":true,"quip_user_id":"navigation-quip-user","init_options":STANDARD.encode(options.raw())});
    let thread = msg(&[s(1, THREAD), n(2, 101), s(10, DOCUMENT)]);
    let document = msg(&[s(1, DOCUMENT), n(2, 101), s(6, THREAD)]);
    // An explicit complete empty canonical section index supplies read-only
    // authority. The HTML export remains a separate rendering receipt.
    let payload = msg(&[m(3, &thread), m(6, &document)]);
    let index = msg(&[s(1, &format!("Section/{DOCUMENT}"))]);
    let response = msg(&[n(1, 1), m(2, &payload), m(3, &index)]);
    Ok((file, init, msg(&[m(2, &response)]).raw().to_vec()))
}
pub(crate) async fn read_pair(
    state: Signal<ShellState>,
    id: &str,
) -> Result<(CanvasCollabSnapshot, (File, CanvasDocument)), Error> {
    let (owner, ledger_model, file, init, bytes) = {
        let shell = state.peek();
        if id != CANVAS || !crate::canvas::surface::visible(&shell, CANVAS) {
            return Err(refused());
        }
        let model = model(&shell)?;
        let owner = session(&shell)?;
        let (file, init, bytes) = responses(&owner)?;
        let client = &shell.core.client;
        // This is the same read pair as loader::read_pair: canonical files.info,
        // controller-init/load-data, and export files.info/HTML. No HTTP client.
        for (request, response) in [
            (
                api::canvas_info(client, &owner, CANVAS),
                json!({"ok":true,"file":file}),
            ),
            (api::canvas_controller_init(client, &owner), init.clone()),
            (
                api::canvas_collab_load_request(client, &owner, THREAD)?,
                json!({"base64":STANDARD.encode(&bytes)}),
            ),
            (
                api::canvas_info(client, &owner, CANVAS),
                json!({"ok":true,"file":file}),
            ),
        ] {
            record(&shell, CANVAS, request, response)?;
        }
        let mut ledger = model.0.lock().map_err(|_| refused())?;
        let nav = ledger.navigation.as_mut().ok_or_else(refused)?;
        nav.requests
            .push(json!({"serial":nav.requests.len()+1,"target":CANVAS,
            "method":"GET","path":EXPORT,"args":{},"response":{"html":HTML}}));
        drop(ledger);
        (owner, model, file, init, bytes)
    };
    let file: File = serde_json::from_value(file).map_err(|_| refused())?;
    let init = CanvasControllerInit::parse(init, &owner)?;
    let load = LoadDataResponse::decode(&bytes).map_err(|_| refused())?;
    let canonical = CanvasCollabSnapshot::from_responses(file.clone(), init, load)?;
    let document = tokio::task::spawn_blocking(|| CanvasDocument::parse(HTML))
        .await
        .map_err(|_| refused())?
        .map_err(|_| refused())?;
    let shell = state.peek();
    let current = model(&shell)?;
    if !Arc::ptr_eq(&ledger_model.0, &current.0) || !crate::canvas::surface::visible(&shell, CANVAS)
    {
        return Err(refused());
    }
    Ok((canonical, (file, document)))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn target_canvas_original_receipts_decode_readonly_and_match_export() {
        let owner = WorkspaceSession {
            team_id: "T1".into(),
            user_id: completion::SELF.into(),
            ..super::super::super::session()
        };
        let (file, init, bytes) = responses(&owner).unwrap();
        let file: File = serde_json::from_value(file).unwrap();
        let snapshot = CanvasCollabSnapshot::from_responses(
            file.clone(),
            CanvasControllerInit::parse(init, &owner).unwrap(),
            LoadDataResponse::decode(&bytes).unwrap(),
        )
        .unwrap();
        assert_eq!(snapshot.file_id(), CANVAS);
        assert_eq!(snapshot.thread_id(), THREAD);
        assert_eq!(snapshot.document_id(), DOCUMENT);
        assert_eq!(file.extra["editable"], false);
        assert!(super_platinum_core::canvas::content_url(EXPORT));
        assert!(CanvasDocument::parse(HTML).is_ok());
        assert!(HTML.contains(CANVAS_CONTENT));
    }
}
