//! Browser-facing URLs, separate from renderer-neutral opaque media identities.
use super_platinum_core::MediaAssetId;

pub(crate) trait MediaUri {
    fn webview_uri(&self) -> String;
    fn webview_uri_at(&self, generation: u64) -> String;
}

impl MediaUri for MediaAssetId {
    fn webview_uri(&self) -> String {
        presentation_uri(&self.uri(), cfg!(target_os = "windows"))
    }

    fn webview_uri_at(&self, generation: u64) -> String {
        presentation_uri(&self.uri_at(generation), cfg!(target_os = "windows"))
    }
}

fn presentation_uri(canonical: &str, windows: bool) -> String {
    if windows {
        // Pinned Wry 0.53.5 intercepts this synthetic HTTP origin and restores
        // the canonical scheme before calling MediaRegistry::respond. Only its
        // initial navigation is automatically rewritten; DOM URLs are not.
        let asset = canonical
            .strip_prefix("super-platinum-media://")
            .expect("core media URI has its canonical scheme");
        format!("http://super-platinum-media.{asset}")
    } else {
        canonical.to_owned()
    }
}

pub(super) fn content_security_policy() -> String {
    csp_for(cfg!(target_os = "windows"))
}

fn csp_for(windows: bool) -> String {
    // Exact synthetic media origins only: never grant arbitrary HTTP images or
    // connections. Huddle script/worker/signalling allowances remain separate.
    let media = if windows {
        "http://super-platinum-media.avatar http://super-platinum-media.emoji http://super-platinum-media.attachment http://super-platinum-media.background"
    } else {
        "super-platinum-media:"
    };
    format!(
        "default-src 'none'; img-src 'self' {media} data: blob:; media-src 'self' {media} blob: mediastream:; style-src 'unsafe-inline'; script-src 'self' dioxus: 'unsafe-inline' 'unsafe-eval'; worker-src 'self' dioxus: blob:; child-src blob:; connect-src 'self' dioxus: ipc: ws: wss: https://*.chime.aws; font-src 'self' blob:; frame-src 'none'; object-src 'none'; base-uri 'none'; form-action 'none'"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use super_platinum_core::MediaAssetKind;

    #[test]
    fn every_kind_has_a_browser_url_and_canonical_callback_identity() {
        for kind in [
            MediaAssetKind::Avatar,
            MediaAssetKind::Emoji,
            MediaAssetKind::Attachment,
            MediaAssetKind::Background,
        ] {
            let id = MediaAssetId::new(kind);
            let windows = presentation_uri(&id.uri(), true);
            assert_eq!(
                windows,
                format!(
                    "http://super-platinum-media.{}/{}",
                    kind.as_str(),
                    id.opaque()
                )
            );
            assert_eq!(presentation_uri(&id.uri(), false), id.uri());
            for generation in [0, 7, u64::MAX] {
                assert_eq!(
                    presentation_uri(&id.uri_at(generation), true),
                    format!("{windows}?v={generation}")
                );
                assert_eq!(
                    presentation_uri(&id.uri_at(generation), false),
                    id.uri_at(generation)
                );
                let uri = id.webview_uri_at(generation);
                assert_eq!(uri, format!("{}?v={generation}", id.webview_uri()));
                // This is Wry's revert_uri_work_around transformation. Registry
                // callbacks retain the exact core ID, including generation URLs.
                let canonical = format!("{windows}?v={generation}")
                    .replace("http://super-platinum-media.", "super-platinum-media://");
                assert_eq!(canonical, id.uri_at(generation));
                assert_eq!(canonical.parse::<MediaAssetId>().unwrap(), id);
                let registry = super::super::MediaRegistry::default();
                registry.insert(id.clone(), "image/png", b"fixture bytes".as_slice());
                let callback = dioxus::desktop::wry::http::Request::builder()
                    .uri(canonical)
                    .body(Vec::new())
                    .unwrap();
                let response = registry.respond(callback);
                assert_eq!(response.status(), 200);
                assert_eq!(response.body().as_ref(), b"fixture bytes");
            }
        }
    }

    #[test]
    fn windows_csp_grants_only_exact_media_origins_to_images_and_playback() {
        let csp = csp_for(true);
        for directive in ["img-src", "media-src"] {
            let sources = csp
                .split(';')
                .find(|part| part.trim().starts_with(directive))
                .unwrap();
            let sources: Vec<_> = sources.split_whitespace().collect();
            for kind in ["avatar", "emoji", "attachment", "background"] {
                assert!(sources.contains(&format!("http://super-platinum-media.{kind}").as_str()));
            }
            assert!(!sources.contains(&"http:"));
            assert!(!sources.contains(&"http://*"));
            assert!(!sources.contains(&"super-platinum-media:"));
        }
        let connect = csp
            .split(';')
            .find(|part| part.trim().starts_with("connect-src"))
            .unwrap();
        assert!(!connect.contains("super-platinum-media"));
        let native = csp_for(false);
        assert!(native.contains("img-src 'self' super-platinum-media: data: blob:"));
        assert!(!native.contains("http://super-platinum-media."));
    }
}
