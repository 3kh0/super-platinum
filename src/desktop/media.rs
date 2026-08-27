use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use dioxus::desktop::wry::http::{Request, Response, StatusCode};
use dioxus::desktop::{Config, WindowBuilder};
use super_platinum_core::slack::Transport;
use super_platinum_core::{
    MediaAssetId, MediaAssetKind, MediaCacheKind, MediaStore, detect_image_mime,
};

const CSP: &str = "default-src 'none'; img-src 'self' super-platinum-media: data:; media-src super-platinum-media:; style-src 'unsafe-inline'; script-src dioxus: 'unsafe-inline' 'unsafe-eval'; connect-src dioxus: ipc: ws: wss:; font-src 'self'; frame-src 'none'; object-src 'none'; base-uri 'none'; form-action 'none'";

/// A 64×64 translucent grey skeleton box.
///
/// Sources are registered during projection and fetched afterwards, so the
/// WebView always asks for some assets before their bytes exist. Answering that
/// with 404 paints the platform's broken-image icon — a hard visual error for
/// what is only a load in flight. Sized elements (avatars, emoji, icons) stretch
/// this to their own box; block and attachment images have no intrinsic size in
/// CSS, so 64 square is what they reserve until the real image arrives.
const PLACEHOLDER_PNG: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x40, 0x00, 0x00, 0x00, 0x40, 0x08, 0x06, 0x00, 0x00, 0x00, 0xaa, 0x69, 0x71,
    0xde, 0x00, 0x00, 0x00, 0x64, 0x49, 0x44, 0x41, 0x54, 0x78, 0xda, 0xed, 0xd0, 0x41, 0x11, 0x00,
    0x00, 0x08, 0x03, 0xa0, 0xa5, 0x31, 0xe7, 0xa2, 0x9b, 0xc3, 0x93, 0x07, 0x05, 0x48, 0xdb, 0xf9,
    0x2c, 0x02, 0x04, 0x08, 0x10, 0x20, 0x40, 0x80, 0x00, 0x01, 0x02, 0x04, 0x08, 0x10, 0x20, 0x40,
    0x80, 0x00, 0x01, 0x02, 0x04, 0x08, 0x10, 0x20, 0x40, 0x80, 0x00, 0x01, 0x02, 0x04, 0x08, 0x10,
    0x20, 0x40, 0x80, 0x00, 0x01, 0x02, 0x04, 0x08, 0x10, 0x20, 0x40, 0x80, 0x00, 0x01, 0x02, 0x04,
    0x08, 0x10, 0x20, 0x40, 0x80, 0x00, 0x01, 0x02, 0x04, 0x08, 0x10, 0x20, 0x40, 0x80, 0x00, 0x01,
    0x02, 0x04, 0x08, 0x10, 0x20, 0x40, 0xc0, 0x7d, 0x0b, 0x59, 0x1a, 0x61, 0x87, 0x15, 0x15, 0x35,
    0xe6, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
];

/// A fully transparent 1×1 PNG for emoji, which sit inline in a sentence: a grey
/// box mid-word reads as damage, whereas a gap reads as text still loading.
const TRANSPARENT_PNG: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4,
    0x89, 0x00, 0x00, 0x00, 0x0b, 0x49, 0x44, 0x41, 0x54, 0x78, 0xda, 0x63, 0x60, 0x00, 0x02, 0x00,
    0x00, 0x05, 0x00, 0x01, 0xe9, 0xfa, 0xdc, 0xd8, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44,
    0xae, 0x42, 0x60, 0x82,
];

/// Ceiling on the retry backoff, reached after eight consecutive failures.
const MAX_RETRY_BACKOFF: Duration = Duration::from_secs(256);

/// Hosts Slack serves its own assets from, which require the session cookie.
fn is_slack_hosted(url: &str) -> bool {
    url.contains("slack-edge.com") || url.contains("slack.com")
}

#[derive(Clone, Default)]
pub struct MediaRegistry {
    assets: Arc<RwLock<HashMap<MediaAssetId, MediaAsset>>>,
    sources: Arc<RwLock<HashMap<MediaAssetId, MediaSource>>>,
    source_ids: Arc<RwLock<HashMap<(MediaAssetKind, String), MediaAssetId>>>,
    backoff: Arc<RwLock<HashMap<MediaAssetId, Backoff>>>,
    load_lock: Arc<tokio::sync::Mutex<()>>,
    /// Set whenever bytes land. The UI tick turns this into one media generation
    /// bump, which is what actually makes painted `img` elements re-request.
    dirty: Arc<AtomicBool>,
    store: Option<Arc<MediaStore>>,
    /// 0 means unlimited (count-cap only). Settings push the user-facing byte
    /// limit here so persist can prune without reading shell state.
    cache_limit: Arc<AtomicU64>,
}

/// What to do about a source that failed to download.
#[derive(Clone, Copy)]
enum Backoff {
    /// The host answered, and the answer will not change: the URL is gone. The
    /// sweep polls every second, so retrying this is a permanent stream of
    /// requests that can never succeed.
    Abandoned,
    /// A timeout, a refused connection, or a server error — all of which pass.
    /// Retried on a doubling delay so a boot while offline does not turn into a
    /// request per second per image.
    Retry { failures: u32, at: Instant },
}

#[derive(Clone)]
struct MediaAsset {
    mime: String,
    bytes: Arc<[u8]>,
    /// Bytes worth painting that are not the ones asked for — the slot's
    /// previous picture, recovered from disk. They stay pending so the current
    /// image replaces them once it downloads.
    provisional: bool,
}

#[derive(Clone)]
struct MediaSource {
    url: String,
    mime: String,
    authenticated: bool,
    /// Registered but not queued. Full-resolution originals — a 60 MB video, a
    /// photo behind a thumbnail — exist as assets so the viewer can open them,
    /// but nothing downloads them until somebody asks.
    deferred: bool,
    /// Stable identity for the on-disk cache; see `MediaStore`. Only set for the
    /// small, endlessly reused images (avatars, emoji) worth persisting.
    slot: Option<String>,
}

impl MediaRegistry {
    /// The registry the app runs on: remembers avatars and emoji across restarts.
    pub fn with_persistence() -> Self {
        let store = match MediaStore::open_default() {
            Ok(store) => Some(Arc::new(store)),
            Err(error) => {
                eprintln!("super-platinum: media cache unavailable, staying in memory: {error}");
                None
            }
        };
        Self {
            store,
            ..Self::default()
        }
    }

    pub fn store(&self) -> Option<Arc<MediaStore>> {
        self.store.clone()
    }

    pub fn set_cache_limit(&self, bytes: Option<u64>) {
        self.cache_limit
            .store(bytes.unwrap_or(0), Ordering::Release);
    }

    fn current_limit(&self) -> Option<u64> {
        match self.cache_limit.load(Ordering::Acquire) {
            0 => None,
            n => Some(n),
        }
    }

    /// Drops painted bytes for slots matching `kind` so the WebView refetches
    /// after a Storage clear. Sources stay registered.
    pub fn evict_kind(&self, kind: MediaCacheKind) {
        let sources = self.sources.read().expect("media registry poisoned");
        let ids: Vec<_> = sources
            .iter()
            .filter_map(|(id, source)| {
                let slot = source.slot.as_deref()?;
                (MediaCacheKind::from_slot(slot) == kind).then(|| id.clone())
            })
            .collect();
        drop(sources);
        if ids.is_empty() {
            return;
        }
        let mut assets = self.assets.write().expect("media registry poisoned");
        for id in ids {
            assets.remove(&id);
        }
        self.dirty.store(true, Ordering::Release);
    }

    pub async fn prune_now(&self) {
        let Some(store) = self.store.clone() else {
            return;
        };
        let limit = self.current_limit();
        let _ = tokio::task::spawn_blocking(move || store.prune_to(limit)).await;
    }

    pub fn insert(&self, id: MediaAssetId, mime: impl Into<String>, bytes: impl Into<Arc<[u8]>>) {
        self.insert_asset(id, mime.into(), bytes.into(), false);
    }

    fn insert_asset(&self, id: MediaAssetId, mime: String, bytes: Arc<[u8]>, provisional: bool) {
        self.assets
            .write()
            .expect("media registry poisoned")
            .insert(
                id,
                MediaAsset {
                    mime,
                    bytes,
                    provisional,
                },
            );
        self.dirty.store(true, Ordering::Release);
    }

    /// Registers a remote image, deciding for itself whether the Slack session
    /// cookie belongs on the request. Slack-hosted assets need it; Block Kit
    /// `image_url`s and unfurl previews point at arbitrary public hosts and must
    /// never see it.
    pub fn register_image(
        &self,
        kind: MediaAssetKind,
        url: &str,
        mime: impl Into<String>,
    ) -> MediaAssetId {
        self.register(kind, url, mime, is_slack_hosted(url))
    }

    /// Registers a profile picture under a stable identity — a Slack user id, or
    /// the bot/webhook icon key from `state::message_avatar`.
    ///
    /// The identity, not the URL, is what the on-disk cache is keyed by: Slack
    /// rotates the URL whenever somebody changes their picture, so identity
    /// keying is what lets the previous face paint immediately while the new one
    /// downloads in the background.
    pub fn register_avatar(&self, identity: &str, url: &str) -> MediaAssetId {
        self.register_slotted(
            MediaAssetKind::Avatar,
            format!("avatar/{identity}"),
            url,
            "image/jpeg",
        )
    }

    /// Registers an image the UI paints at icon size — a context-block avatar,
    /// an unfurl service icon — which has no identity beyond its URL. Cached so
    /// a channel of bot posts stops re-downloading the same faces every launch.
    pub fn register_icon(&self, kind: MediaAssetKind, url: &str) -> MediaAssetId {
        self.register_slotted(kind, format!("icon/{url}"), url, "image/png")
    }

    /// Registers a custom workspace emoji under its shortcode.
    pub fn register_emoji(&self, name: &str, url: &str) -> MediaAssetId {
        self.register_slotted(
            MediaAssetKind::Emoji,
            format!("emoji/{name}"),
            url,
            "image/png",
        )
    }

    fn register_slotted(
        &self,
        kind: MediaAssetKind,
        slot: String,
        url: &str,
        mime: &str,
    ) -> MediaAssetId {
        self.register_source(kind, url, mime.to_owned(), is_slack_hosted(url), Some(slot))
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
        self.register_source(kind, url, mime.into(), authenticated, None)
    }

    /// Registers bytes nobody has asked for yet: the original behind an inline
    /// thumbnail, or the movie behind a poster frame. `request` is what puts it
    /// in the download queue.
    pub fn register_deferred(
        &self,
        kind: MediaAssetKind,
        url: &str,
        mime: impl Into<String>,
    ) -> MediaAssetId {
        self.register_source_inner(kind, url, mime.into(), is_slack_hosted(url), None, true)
    }

    /// Queues a deferred asset. Called when the viewer opens one.
    pub fn request(&self, id: &MediaAssetId) {
        let mut sources = self.sources.write().expect("media registry poisoned");
        let Some(source) = sources.get_mut(id) else {
            return;
        };
        if !source.deferred {
            return;
        }
        source.deferred = false;
        drop(sources);
        // A deferred asset that failed earlier must not stay parked on its
        // backoff when the reader explicitly asks for it again.
        self.backoff
            .write()
            .expect("media registry poisoned")
            .remove(id);
        self.dirty.store(true, Ordering::Relaxed);
    }

    fn register_source(
        &self,
        kind: MediaAssetKind,
        url: &str,
        mime: String,
        authenticated: bool,
        slot: Option<String>,
    ) -> MediaAssetId {
        self.register_source_inner(kind, url, mime, authenticated, slot, false)
    }

    fn register_source_inner(
        &self,
        kind: MediaAssetKind,
        url: &str,
        mime: String,
        authenticated: bool,
        slot: Option<String>,
        deferred: bool,
    ) -> MediaAssetId {
        let key = (kind, url.to_owned());
        let existing = self
            .source_ids
            .read()
            .expect("media registry poisoned")
            .get(&key)
            .cloned();
        if let Some(id) = existing {
            // The same URL asked for eagerly wins over a deferred registration.
            if !deferred {
                self.request(&id);
            }
            return id;
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
                    mime,
                    authenticated,
                    deferred,
                    slot,
                },
            );
        id
    }

    /// Whether this asset has bytes to paint right now.
    ///
    /// Callers with a real fallback — initials, an event glyph — should render
    /// that instead of an `img` whose bytes have not landed. The placeholder the
    /// protocol serves is only for images with nothing better to show.
    pub fn is_ready(&self, id: &MediaAssetId) -> bool {
        self.assets
            .read()
            .expect("media registry poisoned")
            .contains_key(id)
    }

    /// Whether any registered source still needs bytes. Sources are also
    /// registered during render (hover cards, activity rows), which no Slack
    /// call follows, so the UI tick polls this to kick a load.
    pub fn has_pending(&self) -> bool {
        let now = Instant::now();
        let assets = self.assets.read().expect("media registry poisoned");
        let backoff = self.backoff.read().expect("media registry poisoned");
        self.sources
            .read()
            .expect("media registry poisoned")
            .iter()
            .any(|(id, source)| {
                !source.deferred
                    && is_due(backoff.get(id), now)
                    && assets.get(id).is_none_or(|asset| asset.provisional)
            })
    }

    pub fn is_loading(&self) -> bool {
        self.load_lock.try_lock().is_err()
    }

    /// Consumes the "new bytes landed" flag. The caller owns the media
    /// generation bump that makes the WebView re-request painted images.
    pub fn take_dirty(&self) -> bool {
        self.dirty.swap(false, Ordering::AcqRel)
    }

    /// Populates every registered source that has no current bytes yet, from the
    /// on-disk cache where possible and the network otherwise.
    pub async fn load_pending(&self, transport: Arc<Transport>) {
        // Serialize refreshes so a later projection can safely queue more
        // sources while an earlier batch is still downloading.
        let _guard = self.load_lock.lock().await;
        let pending = self.pending_sources();
        if pending.is_empty() {
            return;
        }
        // Disk first: an unchanged URL needs no request at all, and a hit under
        // the same identity at an older URL paints the previous picture
        // immediately instead of leaving a gap until the download finishes.
        let pending = self.hydrate_from_store(pending).await;
        if pending.is_empty() {
            return;
        }
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
        let mut persist = Vec::new();
        while let Some(result) = tasks.join_next().await {
            let Ok((id, source, result)) = result else {
                continue;
            };
            match result {
                Ok(bytes) => {
                    let mime = detect_image_mime(&bytes).unwrap_or(&source.mime);
                    if let Some(slot) = source.slot.clone() {
                        persist.push((slot, source.url.clone(), bytes.clone()));
                    }
                    self.backoff
                        .write()
                        .expect("media registry poisoned")
                        .remove(&id);
                    self.insert_asset(id, mime.to_owned(), bytes.into(), false);
                }
                Err(error) => {
                    // Host plus path, never the query: a signed media URL
                    // carries its credential there. Without the path a failure
                    // cannot be told apart from the next one on the same CDN.
                    let target = url::Url::parse(&source.url)
                        .ok()
                        .map(|url| format!("{}{}", url.host_str().unwrap_or_default(), url.path()))
                        .unwrap_or_else(|| "invalid-url".into());
                    eprintln!(
                        "super-platinum: {} media fetch from {target} failed: {error}",
                        id.kind().as_str()
                    );
                    self.record_failure(&id, is_permanent(&error));
                }
            }
        }
        self.persist(persist).await;
    }

    /// Sources with no current bytes, cheapest and most noticeable first.
    fn pending_sources(&self) -> Vec<(MediaAssetId, MediaSource)> {
        let now = Instant::now();
        let assets = self.assets.read().expect("media registry poisoned");
        let backoff = self.backoff.read().expect("media registry poisoned");
        let mut pending = self
            .sources
            .read()
            .expect("media registry poisoned")
            .iter()
            .filter(|(id, source)| {
                !source.deferred
                    && is_due(backoff.get(*id), now)
                    && assets.get(*id).is_none_or(|asset| asset.provisional)
            })
            .map(|(id, source)| (id.clone(), source.clone()))
            .collect::<Vec<_>>();
        // Avatars and emoji are small and are what the reader notices first;
        // let them land ahead of multi-megabyte file attachments.
        pending.sort_by_key(|(id, _)| match id.kind() {
            MediaAssetKind::Avatar | MediaAssetKind::Emoji => 0,
            MediaAssetKind::Background => 1,
            MediaAssetKind::Attachment => 2,
        });
        pending
    }

    /// Fills in what the on-disk cache already holds and returns the sources
    /// that still need the network.
    async fn hydrate_from_store(
        &self,
        pending: Vec<(MediaAssetId, MediaSource)>,
    ) -> Vec<(MediaAssetId, MediaSource)> {
        let Some(store) = self.store.clone() else {
            return pending;
        };
        let lookups = pending
            .iter()
            .filter_map(|(id, source)| {
                let slot = source.slot.clone()?;
                Some((id.clone(), slot, source.url.clone()))
            })
            .collect::<Vec<_>>();
        if lookups.is_empty() {
            return pending;
        }
        let hits = tokio::task::spawn_blocking(move || {
            lookups
                .into_iter()
                .filter_map(|(id, slot, url)| store.load(&slot, &url).map(|image| (id, image)))
                .collect::<Vec<_>>()
        })
        .await
        .unwrap_or_default();
        let mut satisfied = HashSet::new();
        for (id, image) in hits {
            if image.current {
                satisfied.insert(id.clone());
            }
            self.insert_asset(id, image.mime, image.bytes.into(), !image.current);
        }
        pending
            .into_iter()
            .filter(|(id, _)| !satisfied.contains(id))
            .collect()
    }

    async fn persist(&self, entries: Vec<(String, String, Vec<u8>)>) {
        let Some(store) = self.store.clone() else {
            return;
        };
        if entries.is_empty() {
            return;
        }
        let limit = self.current_limit();
        let _ = tokio::task::spawn_blocking(move || {
            for (slot, url, bytes) in entries {
                if let Err(error) = store.store(&slot, &url, &bytes) {
                    eprintln!("super-platinum: could not cache {slot}: {error}");
                }
            }
            store.prune_to(limit);
        })
        .await;
    }

    fn record_failure(&self, id: &MediaAssetId, permanent: bool) {
        let mut backoff = self.backoff.write().expect("media registry poisoned");
        if permanent {
            backoff.insert(id.clone(), Backoff::Abandoned);
            return;
        }
        let failures = match backoff.get(id) {
            Some(Backoff::Retry { failures, .. }) => failures.saturating_add(1),
            _ => 1,
        };
        let delay = Duration::from_secs(1 << failures.min(8)).min(MAX_RETRY_BACKOFF);
        backoff.insert(
            id.clone(),
            Backoff::Retry {
                failures,
                at: Instant::now() + delay,
            },
        );
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
                // Provisional bytes are replaced in place once the current
                // picture downloads, so they must not be held by the WebView.
                if asset.provisional {
                    "no-store"
                } else {
                    "private, max-age=3600"
                },
            ),
            // Not an error: the asset is still downloading. A placeholder keeps
            // the layout intact where a 404 would paint a broken-image icon.
            None => response(
                StatusCode::OK,
                "image/png",
                Cow::Borrowed(placeholder_for(id.kind())),
                "no-store",
            ),
        }
    }
}

fn is_due(backoff: Option<&Backoff>, now: Instant) -> bool {
    match backoff {
        None => true,
        Some(Backoff::Abandoned) => false,
        Some(Backoff::Retry { at, .. }) => *at <= now,
    }
}

/// A dead URL will never start working; a stalled host, a refused connection, or
/// a server error all pass. Only the first is worth giving up on, and one
/// answer from the host is enough to know.
fn is_permanent(error: &super_platinum_core::slack::Error) -> bool {
    matches!(
        error,
        super_platinum_core::slack::Error::HttpStatus { status, .. }
            if (400..500).contains(status) && *status != 408 && *status != 429
    )
}

/// Emoji flow inline with text and get a transparent gap; everything else gets a
/// skeleton box the size of the image that is coming.
fn placeholder_for(kind: MediaAssetKind) -> &'static [u8] {
    match kind {
        MediaAssetKind::Emoji => TRANSPARENT_PNG,
        _ => PLACEHOLDER_PNG,
    }
}

/// The floor the real client enforces on macOS, measured against Slack 4.51
/// over CDP: any smaller and its own layout starts colliding. `styles/
/// responsive.css` is written to hold together down to exactly this size.
const WINDOW_MIN_INNER_SIZE: dioxus::desktop::tao::dpi::LogicalSize<f64> =
    dioxus::desktop::tao::dpi::LogicalSize::new(668.0, 400.0);

const WINDOW_DEFAULT_INNER_SIZE: dioxus::desktop::tao::dpi::LogicalSize<f64> =
    dioxus::desktop::tao::dpi::LogicalSize::new(1280.0, 800.0);

pub fn desktop_config(media: MediaRegistry) -> Config {
    Config::new()
        .with_window(
            WindowBuilder::new()
                .with_title("Super Platinum")
                .with_inner_size(WINDOW_DEFAULT_INNER_SIZE)
                .with_min_inner_size(WINDOW_MIN_INNER_SIZE),
        )
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

    fn get(registry: &MediaRegistry, uri: &str) -> Response<Cow<'static, [u8]>> {
        let request = Request::builder()
            .uri(uri)
            .body(Vec::new())
            .expect("media request");
        registry.respond(request)
    }

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
    fn pending_media_paints_a_placeholder_instead_of_a_broken_image() {
        // A 404 here is what the platform draws its broken-image icon for, and
        // the source is merely still downloading.
        let registry = MediaRegistry::default();
        let avatar = registry.register(
            MediaAssetKind::Avatar,
            "https://example.test/avatar.png",
            "image/png",
            false,
        );
        let response = get(&registry, &avatar.uri());
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()["Content-Type"], "image/png");
        assert_eq!(response.body().as_ref(), PLACEHOLDER_PNG);
        // Never let the WebView hold that transient answer.
        assert_eq!(response.headers()["Cache-Control"], "no-store");
        assert!(!registry.is_ready(&avatar));

        // Inline emoji get a gap rather than a grey box mid-sentence.
        let emoji = registry.register_emoji("party", "https://example.test/party.png");
        assert_eq!(
            get(&registry, &emoji.uri()).body().as_ref(),
            TRANSPARENT_PNG
        );
    }

    #[test]
    fn generation_stamped_uris_resolve_to_the_same_asset() {
        // `src` is what the WebView diffs, so a pending image only retries when
        // the generation changes the URL. The protocol must still resolve it.
        let registry = MediaRegistry::default();
        let id = registry.register_image(
            MediaAssetKind::Attachment,
            "http://cdn.example.test/weather/64x64/night/302.png",
            "image/png",
        );
        registry.insert(id.clone(), "image/png", b"png-bytes".as_slice());
        for uri in [id.uri(), id.uri_at(0), id.uri_at(7)] {
            let response = get(&registry, &uri);
            assert_eq!(response.status(), StatusCode::OK, "{uri}");
            assert_eq!(response.body().as_ref(), b"png-bytes", "{uri}");
        }
    }

    #[test]
    fn a_deferred_source_waits_until_the_viewer_asks_for_it() {
        let registry = MediaRegistry::default();
        let id = registry.register_deferred(
            MediaAssetKind::Attachment,
            "https://files.slack.com/files-pri/T1-F1/clip.mp4",
            "video/mp4",
        );

        // Nothing downloads a 60 MB movie to draw a message.
        assert!(!registry.has_pending());
        assert!(registry.pending_sources().is_empty());

        registry.request(&id);
        assert!(registry.has_pending());
        assert_eq!(registry.pending_sources().len(), 1);
    }

    #[test]
    fn asking_for_a_url_eagerly_promotes_the_deferred_registration() {
        let registry = MediaRegistry::default();
        let url = "https://files.slack.com/files-tmb/T1-F1/photo_800.jpg";
        let deferred = registry.register_deferred(MediaAssetKind::Attachment, url, "image/jpeg");
        assert!(!registry.has_pending());

        let eager = registry.register_image(MediaAssetKind::Attachment, url, "image/jpeg");
        assert_eq!(deferred, eager, "the same URL keeps one asset id");
        assert!(registry.has_pending());
    }

    #[test]
    fn only_slack_hosted_images_carry_the_session_cookie() {
        assert!(is_slack_hosted("https://ca.slack-edge.com/T1-U1-abc-48"));
        assert!(is_slack_hosted("https://files.slack.com/files-tmb/x.png"));
        // Block Kit and unfurl images point at arbitrary third-party hosts.
        assert!(!is_slack_hosted("http://cdn.weatherapi.com/weather/64.png"));
        assert!(!is_slack_hosted("https://cachet.dunkirk.sh/users/U1/r"));
    }

    #[test]
    fn landed_bytes_raise_the_dirty_flag_exactly_once() {
        // The flag is what the UI tick turns into a media generation bump; a
        // stuck flag would re-stamp every image URL on every tick.
        let registry = MediaRegistry::default();
        assert!(!registry.take_dirty());
        let id = registry.register_avatar("U1", "https://ca.slack-edge.com/T1-U1-abc-48");
        assert!(!registry.take_dirty(), "registering is not painting");
        registry.insert(id, "image/png", b"bytes".as_slice());
        assert!(registry.take_dirty());
        assert!(!registry.take_dirty());
    }

    #[test]
    fn provisional_bytes_paint_but_stay_pending() {
        // The previous picture for this identity, recovered from disk while the
        // new URL downloads: it must render, must not be cached by the WebView,
        // and must not stop the fetch.
        let registry = MediaRegistry::default();
        let id = registry.register_avatar("U1", "https://ca.slack-edge.com/T1-U1-new-48");
        registry.insert_asset(
            id.clone(),
            "image/png".into(),
            b"old".as_slice().into(),
            true,
        );

        assert!(registry.is_ready(&id), "the old face still paints");
        assert!(registry.has_pending(), "the new face is still wanted");
        let response = get(&registry, &id.uri());
        assert_eq!(response.body().as_ref(), b"old");
        assert_eq!(response.headers()["Cache-Control"], "no-store");

        registry.insert(id.clone(), "image/png", b"new".as_slice());
        assert!(!registry.has_pending());
        assert_eq!(
            get(&registry, &id.uri()).headers()["Cache-Control"],
            "private, max-age=3600"
        );
    }

    #[test]
    fn a_dead_url_is_abandoned_and_a_stalled_host_is_retried_on_a_backoff() {
        let registry = MediaRegistry::default();
        let gone = registry.register_avatar("U1", "https://ca.slack-edge.com/T1-U1-gone-48");
        let stalled = registry.register_avatar("U2", "https://ca.slack-edge.com/T1-U2-abc-48");

        registry.record_failure(&gone, true);
        registry.record_failure(&stalled, false);
        assert!(
            !registry.has_pending(),
            "neither source is due, so the sweep must stay quiet"
        );

        let backoff = registry.backoff.read().unwrap();
        assert!(matches!(backoff[&gone], Backoff::Abandoned));
        let Backoff::Retry { failures, at } = backoff[&stalled] else {
            panic!("a timeout is transient, not fatal");
        };
        assert_eq!(failures, 1);
        assert!(at > Instant::now());
        drop(backoff);

        // Once the delay is up the source is tried again — an app that booted
        // offline must not be stuck showing initials for the whole session.
        registry.backoff.write().unwrap().insert(
            stalled.clone(),
            Backoff::Retry {
                failures: 1,
                at: Instant::now() - Duration::from_secs(1),
            },
        );
        assert!(registry.has_pending());
        assert_eq!(registry.pending_sources().len(), 1, "only the due source");

        // Each further failure doubles the wait rather than adding a fixed step.
        registry.record_failure(&stalled, false);
        let Backoff::Retry { failures, .. } = registry.backoff.read().unwrap()[&stalled] else {
            panic!("still transient");
        };
        assert_eq!(failures, 2);
    }

    #[test]
    fn only_client_errors_count_as_permanent() {
        use super_platinum_core::slack::Error;
        assert!(is_permanent(&Error::HttpStatus {
            status: 404,
            retry_after_secs: None
        }));
        assert!(!is_permanent(&Error::HttpStatus {
            status: 503,
            retry_after_secs: None
        }));
        // Timeouts and rate limits pass; giving up on them loses the image for
        // the rest of the session.
        assert!(!is_permanent(&Error::HttpStatus {
            status: 429,
            retry_after_secs: Some(5)
        }));
        assert!(!is_permanent(&Error::Transport("send: timeout".into())));
    }

    #[test]
    fn evict_kind_drops_painted_bytes_but_keeps_the_source() {
        let registry = MediaRegistry::default();
        let id = registry.register_avatar("U1", "https://ca.slack-edge.com/T1-U1-48");
        registry.insert(id.clone(), "image/png", b"face".as_slice());
        assert!(registry.is_ready(&id));
        registry.evict_kind(MediaCacheKind::Avatars);
        assert!(!registry.is_ready(&id), "cleared pictures must refetch");
        assert!(
            registry.sources.read().unwrap().contains_key(&id),
            "the source stays so load_pending can refill it"
        );
        assert!(registry.take_dirty());
    }

    #[test]
    fn identity_slots_survive_the_avatar_url_rotating() {
        let registry = MediaRegistry::default();
        let old = registry.register_avatar("U1", "https://ca.slack-edge.com/T1-U1-old-48");
        let new = registry.register_avatar("U1", "https://ca.slack-edge.com/T1-U1-new-48");
        assert_ne!(old, new, "different bytes need different asset ids");
        let sources = registry.sources.read().unwrap();
        assert_eq!(sources[&old].slot.as_deref(), Some("avatar/U1"));
        assert_eq!(sources[&new].slot.as_deref(), Some("avatar/U1"));
        assert!(sources[&new].authenticated, "Slack hosts need the cookie");
    }

    #[test]
    fn cached_pictures_paint_from_disk_and_a_rotated_url_stays_pending() {
        let root = std::env::temp_dir().join(format!(
            "super-platinum-registry-{}",
            super_platinum_core::MediaAssetId::new(MediaAssetKind::Avatar).opaque()
        ));
        let store = MediaStore::open(&root).expect("store");
        store
            .store(
                "avatar/U1",
                "https://ca.slack-edge.com/T1-U1-old-48",
                b"face",
            )
            .expect("seed");
        store
            .store(
                "emoji/party",
                "https://emoji.slack-edge.com/party.png",
                b"gif",
            )
            .expect("seed");
        let registry = MediaRegistry {
            store: Some(Arc::new(store)),
            ..MediaRegistry::default()
        };

        // U1 changed their picture: same identity, new URL.
        let rotated = registry.register_avatar("U1", "https://ca.slack-edge.com/T1-U1-new-48");
        let unchanged = registry.register_emoji("party", "https://emoji.slack-edge.com/party.png");
        let cold = registry.register_avatar("U2", "https://ca.slack-edge.com/T1-U2-abc-48");

        let still_wanted = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime")
            .block_on(registry.hydrate_from_store(registry.pending_sources()));

        assert!(
            registry.is_ready(&rotated),
            "the previous picture paints while the new one downloads"
        );
        assert!(registry.is_ready(&unchanged));
        assert!(!registry.is_ready(&cold), "nothing cached for U2");

        let wanted = still_wanted
            .iter()
            .map(|(id, _)| id.clone())
            .collect::<HashSet<_>>();
        assert!(
            wanted.contains(&rotated),
            "the new picture is still fetched"
        );
        assert!(wanted.contains(&cold));
        assert!(
            !wanted.contains(&unchanged),
            "an unchanged URL needs no request at all"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn downloaded_image_signature_overrides_a_stale_mime_hint() {
        assert_eq!(
            detect_image_mime(b"\x89PNG\r\n\x1a\nrest"),
            Some("image/png")
        );
        assert_eq!(
            detect_image_mime(&[0xff, 0xd8, 0xff, 0xe0]),
            Some("image/jpeg")
        );
        assert_eq!(
            detect_image_mime(b"RIFF\0\0\0\0WEBPrest"),
            Some("image/webp")
        );
    }
}
