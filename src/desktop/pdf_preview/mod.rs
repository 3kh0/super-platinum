mod isolated;
mod render;
pub use isolated::{Decoded, IsolatedPdf};
pub mod check;
pub mod fixture;
#[cfg(test)]
pub(crate) mod test_io;
// PDF rendering uses bundled PDF.js and opaque, current-viewer-only bytes.
use crate::state::{Overlay, ShellState};
use dioxus::prelude::*;
use std::borrow::Cow;
use super_platinum_core::MediaAssetId;

pub fn serve_asset(
    state: Signal<ShellState>,
    request: dioxus::desktop::AssetRequest,
    responder: dioxus::desktop::wry::RequestAsyncResponder,
) {
    let path = request.uri().path();
    let (status, mime, body): (u16, &str, Cow<'static, [u8]>) = if request.method() != "GET" {
        (405, "text/plain", Cow::Borrowed(b""))
    } else if path == "/pdf/search.mjs" {
        (
            200,
            "text/javascript",
            Cow::Borrowed(include_bytes!("search.mjs")),
        )
    } else if let Some(bytes) = crate::pdf_assets::asset(path) {
        let mime = if path.ends_with(".mjs") || path.ends_with(".js") {
            "text/javascript"
        } else if path.ends_with(".wasm") {
            "application/wasm"
        } else {
            "application/octet-stream"
        };
        (200, mime, Cow::Borrowed(bytes))
    } else if let Some(relative) = path.strip_prefix("/pdf/search-data/") {
        let bytes = search_bytes(&state.peek(), relative);
        if let Some(bytes) = bytes {
            (200, "application/pdf", Cow::Owned(bytes.to_vec()))
        } else {
            (404, "text/plain", Cow::Borrowed(b""))
        }
    } else if let Some(opaque) = path.strip_prefix("/pdf/data/") {
        let bytes = viewer_bytes(&state.peek(), opaque);
        if let Some(bytes) = bytes {
            (200, "application/pdf", Cow::Owned(bytes.to_vec()))
        } else {
            (404, "text/plain", Cow::Borrowed(b""))
        }
    } else {
        (404, "text/plain", Cow::Borrowed(b""))
    };
    responder.respond(
        dioxus::desktop::wry::http::Response::builder()
            .status(status)
            .header("Content-Type", mime)
            .header("Cache-Control", "no-store")
            .header("X-Content-Type-Options", "nosniff")
            .body(body)
            .expect("static PDF response"),
    );
}

pub(crate) fn search_bytes(shell: &ShellState, relative: &str) -> Option<std::sync::Arc<[u8]>> {
    let (nonce, opaque) = relative.split_once('/')?;
    if nonce.is_empty() || opaque.is_empty() || opaque.contains('/') {
        return None;
    }
    crate::bootstrap::search_file_pages::pdf::bytes(shell, nonce, opaque)
}
/// Inert named-fixture markup. No decoder, private route or readiness exists.
#[component]
pub(crate) fn IsolatedPdfLayout() -> Element {
    render::view("pdf-search-layout".into(), EventHandler::new(|_| {}))
}

#[component]
pub fn PdfPreview(state: Signal<ShellState>, id: MediaAssetId) -> Element {
    let _ = state;
    let dom_id = format!("pdf-{}", id.opaque());
    let config = serde_json::json!({"id":dom_id,"source":format!("/pdf/data/{}",id.opaque()),"debug":crate::fixture::is_fixture() && std::env::var_os("SUPER_PLATINUM_PDF_DEBUG").is_some()});
    let cleanup = dom_id.clone();
    use_drop(move || {
        document::eval(&format!(
            "window.superPlatinumPdf?.get({})?.destroy();",
            serde_json::to_string(&cleanup).unwrap()
        ));
    });
    render::view(
        dom_id,
        EventHandler::new(move |_| {
            let config = config.clone();
            spawn(async move {
                let mut eval = document::eval(&format!(
                    "{}\nawait runPdfPreview(await dioxus.recv(),globalThis,dioxus);",
                    include_str!("bridge.js")
                ));
                if eval.send(config).is_ok() {
                    let _ = eval.recv::<serde_json::Value>().await;
                }
            });
        }),
    )
}

fn viewer_bytes(shell: &ShellState, opaque: &str) -> Option<std::sync::Arc<[u8]>> {
    let id = format!("super-platinum-media://attachment/{opaque}")
        .parse::<MediaAssetId>()
        .ok()?;
    if shell.overlay != Some(Overlay::Viewer)
        || !shell
            .viewer
            .as_ref()
            .is_some_and(|v| v.id == id && v.mime == "application/pdf")
    {
        return None;
    }
    shell.media.ready_bytes(&id)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pdf_bytes_are_available_only_for_the_current_pdf_viewer() {
        let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
        fixture::apply(&mut shell, "pdf-preview");
        let id = shell.viewer.as_ref().unwrap().id.clone();
        assert!(
            viewer_bytes(&shell, id.opaque())
                .unwrap()
                .starts_with(b"%PDF-1.7")
        );
        assert!(viewer_bytes(&shell, "../../other").is_none());
        shell.overlay = None;
        assert!(viewer_bytes(&shell, id.opaque()).is_none());
        shell.overlay = Some(Overlay::Viewer);
        shell.viewer.as_mut().unwrap().mime = "text/plain".into();
        assert!(viewer_bytes(&shell, id.opaque()).is_none());
        fixture::apply(&mut shell, "pdf-preview");
        assert!(viewer_bytes(&shell, id.opaque()).is_none());
    }
    #[test]
    fn bundled_pdf_routes_do_not_expose_scripting_or_path_traversal() {
        assert!(crate::pdf_assets::asset("/pdf/pdf.mjs").is_some());
        assert!(crate::pdf_assets::asset("/pdf/pdf.worker.mjs").is_some());
        assert!(crate::pdf_assets::asset("/pdf/../LICENSE").is_none());
        assert!(crate::pdf_assets::asset("/pdf/pdf.sandbox.mjs").is_none());
        assert!(crate::pdf_assets::asset("/pdf/wasm/quickjs-eval.wasm").is_none());
    }
}
