pub mod check;
pub mod fixture;
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
    rsx! {
        div {id:"{dom_id}",class:"pdf-preview",onmounted:move |_|{
            let config=config.clone();
            spawn(async move {
                let mut eval=document::eval(include_str!("bridge.js"));
                if eval.send(config).is_ok(){let _=eval.recv::<serde_json::Value>().await;}
            });
        },
            div {class:"pdf-toolbar","aria-label":"PDF controls",
                button {r#type:"button","data-pdf":"previous",disabled:true,"Previous"}
                label {"Page",input {r#type:"number",min:"1",value:"1",disabled:true,"aria-label":"PDF page","data-pdf":"page"}}
                span {"data-pdf":"count","of …"}
                button {r#type:"button","data-pdf":"next",disabled:true,"Next"}
                label {"Zoom",select {"data-pdf":"zoom",disabled:true,"aria-label":"PDF zoom",
                    option {value:"1","Fit width"}option {value:"1.5","150%"}option {value:"2","200%"}option {value:"3","300%"}
                }}
                button {r#type:"button","data-pdf":"rotate",disabled:true,"Rotate"}
            }
            div {class:"pdf-search",role:"search","aria-label":"Find in PDF",
                input {r#type:"text",role:"searchbox",placeholder:"Find in PDF",disabled:true,"data-pdf":"find","aria-label":"Find in PDF",autocomplete:"off",spellcheck:false}
                span {role:"status","data-pdf":"find-status"}
                button {r#type:"button",class:"pdf-find-previous",disabled:true,"data-pdf":"find-previous","aria-label":"Previous PDF match",title:"Previous match (Shift+Enter)",{crate::icons::icon(crate::icons::Icon::ChevronRight,"icon")}}
                button {r#type:"button",disabled:true,"data-pdf":"find-next","aria-label":"Next PDF match",title:"Next match (Enter)",{crate::icons::icon(crate::icons::Icon::ChevronRight,"icon")}}
                button {r#type:"button",disabled:true,"data-pdf":"find-clear","aria-label":"Clear PDF search",title:"Clear search",{crate::icons::icon(crate::icons::Icon::Close,"icon")}}
            }
            p {class:"pdf-status",role:"status","data-pdf":"status","Loading PDF…"}
            form {class:"pdf-password",hidden:true,"data-pdf":"password-form",
                label {"Password",input {r#type:"password",autocomplete:"off","data-pdf":"password","aria-label":"PDF password"}}
                button {r#type:"submit","Unlock"}
            }
            div {class:"pdf-scroll",tabindex:"0","aria-label":"PDF document",
                div {class:"pdf-page",canvas {"aria-label":"PDF page image"}div {class:"textLayer"}}
            }
        }
    }
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
