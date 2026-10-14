//! Native image decoding can fail even after a successful HTTP download.
use crate::media::MediaUri;
use dioxus::prelude::*;
use super_platinum_core::MediaAssetId;

use crate::state::ShellState;

#[component]
pub(crate) fn MediaImage(
    state: Signal<ShellState>,
    id: MediaAssetId,
    media_epoch: u64,
    #[props(default)] alt: String,
    #[props(default)] class: String,
    #[props(default)] style: String,
    #[props(default)] title: String,
    #[props(default = "eager".to_owned())] loading: String,
    #[props(default)] compact: bool,
    #[props(default)] fallback: Option<Element>,
) -> Element {
    // Invalidate this instance immediately on decoder failure. The registry's
    // normal dirty tick updates any other copies of the same image.
    let mut local_revision = use_signal(|| 0u64);
    let _ = local_revision();
    let media = state.peek().media.clone();
    let (revision, unavailable) = media.image_state(&id);
    if unavailable {
        if let Some(fallback) = fallback {
            return fallback;
        }
        let label = if alt.is_empty() {
            "Image unavailable".to_owned()
        } else {
            format!("{alt} — image unavailable")
        };
        return rsx! {
            span {
                class:if compact {format!("{class} media-unavailable compact")} else {format!("{class} media-unavailable")},
                style, role:"img", "aria-label":label.clone(), title:label,
                if !compact { "Image unavailable" }
            }
        };
    }
    let uri = id.webview_uri_at(media_epoch);
    rsx! {
        img {
            src:uri, alt, class, style, title, loading,
            onerror:move |_| {
                if let Some(revision) = revision {
                    if media.reject_image(&id, revision) || media.image_state(&id).1 {
                        local_revision += 1;
                    }
                }
            }
        }
    }
}
