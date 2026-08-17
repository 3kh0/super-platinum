use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use dioxus::desktop::wry::http::{Request, Response, StatusCode};
use dioxus::desktop::{Config, WindowBuilder};
use super_platinum_core::slack::Transport;
use super_platinum_core::{MediaAssetId, MediaAssetKind};

const CSP: &str = "default-src 'none'; img-src 'self' super-platinum-media: data:; media-src super-platinum-media:; style-src 'unsafe-inline'; script-src dioxus: 'unsafe-inline' 'unsafe-eval'; connect-src dioxus: ipc: ws: wss:; font-src 'self'; frame-src 'none'; object-src 'none'; base-uri 'none'; form-action 'none'";

#[derive(Clone, Default)]
pub struct MediaRegistry {
    assets: Arc<RwLock<HashMap<MediaAssetId, MediaAsset>>>,
    sources: Arc<RwLock<HashMap<MediaAssetId, MediaSource>>>,
    source_ids: Arc<RwLock<HashMap<(MediaAssetKind, String), MediaAssetId>>>,
    load_lock: Arc<tokio::sync::Mutex<()>>,
}

#[derive(Clone)]
struct MediaAsset {
    mime: String,
    bytes: Arc<[u8]>,
}

#[derive(Clone)]
struct MediaSource {
    url: String,
    mime: String,
    authenticated: bool,
}

impl MediaRegistry {
    pub fn insert(&self, id: MediaAssetId, mime: impl Into<String>, bytes: impl Into<Arc<[u8]>>) {
        self.assets
            .write()
            .expect("media registry poisoned")
            .insert(
                id,
                MediaAsset {
                    mime: mime.into(),
                    bytes: bytes.into(),
                },
            );
    }

    /// Registers a native-only source and returns an opaque URL safe to expose
    /// to the WebView. The Slack URL is retained exclusively in Rust memory.
    pub fn register(
        &self,
        kind: MediaAssetKind,
        url: &str,
        mime: impl Into<String>,
        authenticated: bool,
    ) -> MediaAssetId {
        let key = (kind, url.to_owned());
        if let Some(id) = self
            .source_ids
            .read()
            .expect("media registry poisoned")
            .get(&key)
        {
            return id.clone();
        }
        let id = MediaAssetId::new(kind);
        self.source_ids
            .write()
            .expect("media registry poisoned")
            .insert(key, id.clone());
        self.sources
            .write()
            .expect("media registry poisoned")
            .insert(
                id.clone(),
                MediaSource {
                    url: url.to_owned(),
                    mime: mime.into(),
                    authenticated,
                },
            );
        id
    }

    pub async fn load_pending(&self, transport: Arc<Transport>) {
        // Serialize refreshes so a later projection can safely queue more
        // sources while an earlier batch is still downloading.
        let _guard = self.load_lock.lock().await;
        let pending = self
            .sources
            .read()
            .expect("media registry poisoned")
            .iter()
            .filter(|(id, _)| {
                !self
                    .assets
                    .read()
                    .expect("media registry poisoned")
                    .contains_key(*id)
            })
            .map(|(id, source)| (id.clone(), source.clone()))
            .collect::<Vec<_>>();
        let user_agent = super_platinum_core::slack::xparams::Identity::from_capture().user_agent;
        let permits = Arc::new(tokio::sync::Semaphore::new(12));
        let mut tasks = tokio::task::JoinSet::new();
        for (id, source) in pending {
            let transport = transport.clone();
            let user_agent = user_agent.clone();
            let permits = permits.clone();
            tasks.spawn(async move {
                let _permit = permits.acquire_owned().await.expect("semaphore is open");
                let result = if source.authenticated {
                    transport.get_bytes(&source.url, &user_agent).await
                } else if source.mime == "image/gif" {
                    transport
                        .get_public_gif_bytes(&source.url, &user_agent)
                        .await
                } else {
                    transport.get_public_bytes(&source.url, &user_agent).await
                };
                (id, source, result)
            });
        }
        while let Some(result) = tasks.join_next().await {
            let Ok((id, source, result)) = result else {
                continue;
            };
            match result {
                Ok(bytes) => {
                    let mime = detected_image_mime(&bytes).unwrap_or(&source.mime);
                    self.insert(id, mime, bytes);
                }
                Err(error) => {
                    let host = url::Url::parse(&source.url)
                        .ok()
                        .and_then(|url| url.host_str().map(str::to_owned))
                        .unwrap_or_else(|| "invalid-url".into());
                    eprintln!(
                        "super-platinum: {} media fetch from {host} failed: {error}",
                        id.kind().as_str()
                    );
                }
            }
        }
    }

    fn respond(&self, request: Request<Vec<u8>>) -> Response<Cow<'static, [u8]>> {
        if request.method() != "GET" {
            return response(
                StatusCode::METHOD_NOT_ALLOWED,
                "text/plain",
                Cow::Borrowed(b"method not allowed"),
                "no-store",
            );
        }
        let Ok(id) = request.uri().to_string().parse::<MediaAssetId>() else {
            return response(
                StatusCode::BAD_REQUEST,
                "text/plain",
                Cow::Borrowed(b"invalid media id"),
                "no-store",
            );
        };
        let asset = self
            .assets
            .read()
            .expect("media registry poisoned")
            .get(&id)
            .cloned();
        match asset {
            Some(asset) => response(
                StatusCode::OK,
                &asset.mime,
                Cow::Owned(asset.bytes.to_vec()),
                "private, max-age=3600",
            ),
            None => response(
                StatusCode::NOT_FOUND,
                "text/plain",
                Cow::Borrowed(b"media not found"),
                // Sources are registered during projection and populated
                // asynchronously. Never let WebKit cache that transient miss.
                "no-store",
            ),
        }
    }
}

pub fn desktop_config(media: MediaRegistry) -> Config {
    Config::new()
        .with_window(WindowBuilder::new().with_title("Super Platinum"))
        .with_custom_head(format!(
            r#"<meta http-equiv="Content-Security-Policy" content="{CSP}">"#
        ))
        .with_custom_protocol("super-platinum-media", move |_webview, request| {
            media.respond(request)
        })
        .with_navigation_handler(|url| {
            url.starts_with("dioxus://")
                || url.starts_with("super-platinum-media://")
                || url == "about:blank"
        })
}

fn response(
    status: StatusCode,
    mime: &str,
    body: Cow<'static, [u8]>,
    cache_control: &str,
) -> Response<Cow<'static, [u8]>> {
    Response::builder()
        .status(status)
        .header("Content-Type", mime)
        .header("Cache-Control", cache_control)
        .header("X-Content-Type-Options", "nosniff")
        .body(body)
        .expect("static media response is valid")
}

fn detected_image_mime(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        Some("image/jpeg")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some("image/gif")
    } else if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        Some("image/webp")
    } else {
        None
    }
}

pub fn open_external(url: &str) -> Result<(), String> {
    let parsed = validated_external_url(url)?;
    #[cfg(target_os = "macos")]
    let mut command = std::process::Command::new("open");
    #[cfg(target_os = "linux")]
    let mut command = std::process::Command::new("xdg-open");
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = std::process::Command::new("cmd");
        command.args(["/C", "start", ""]);
        command
    };
    command
        .arg(parsed.as_str())
        .spawn()
        .map(|_| ())
        .map_err(|error| error.to_string())
}

fn validated_external_url(url: &str) -> Result<url::Url, String> {
    let parsed = url::Url::parse(url).map_err(|error| error.to_string())?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
        return Err("only absolute HTTP(S) links may be opened".into());
    }
    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_external_urls_without_spawning() {
        assert!(validated_external_url("javascript:alert(1)").is_err());
        assert!(validated_external_url("file:///etc/passwd").is_err());
        assert!(validated_external_url("https://").is_err());
        assert!(validated_external_url("https://example.com/path").is_ok());
    }

    #[test]
    fn registered_sources_expose_only_opaque_ids() {
        let registry = MediaRegistry::default();
        let secret_url = "https://files.slack.com/files-pri/T/F/image.png?token=secret";
        let id = registry.register(MediaAssetKind::Attachment, secret_url, "image/png", true);
        assert!(id.uri().starts_with("super-platinum-media://attachment/"));
        assert!(!id.uri().contains("slack.com"));
        assert!(!id.uri().contains("secret"));
        assert_eq!(registry.sources.read().unwrap().len(), 1);
    }

    #[test]
    fn pending_media_misses_are_not_cached_by_the_webview() {
        let registry = MediaRegistry::default();
        let id = registry.register(
            MediaAssetKind::Avatar,
            "https://example.test/avatar.png",
            "image/png",
            false,
        );
        let request = Request::builder()
            .uri(id.uri())
            .body(Vec::new())
            .expect("media request");
        let response = registry.respond(request);

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        assert_eq!(response.headers()["Cache-Control"], "no-store");
    }

    #[test]
    fn downloaded_image_signature_overrides_a_stale_mime_hint() {
        assert_eq!(
            detected_image_mime(b"\x89PNG\r\n\x1a\nrest"),
            Some("image/png")
        );
        assert_eq!(
            detected_image_mime(&[0xff, 0xd8, 0xff, 0xe0]),
            Some("image/jpeg")
        );
        assert_eq!(
            detected_image_mime(b"RIFF\0\0\0\0WEBPrest"),
            Some("image/webp")
        );
    }
}
