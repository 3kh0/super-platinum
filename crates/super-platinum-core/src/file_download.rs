//! Original downloads are staged beside the destination, never in the preview cache.
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use tokio::io::AsyncWriteExt;

pub const MAX_ORIGINAL_BYTES: u64 = 8 * 1024 * 1024 * 1024;
pub const WRITE_CHUNK: usize = 64 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum DownloadError {
    #[error("download canceled")]
    Canceled,
    #[error("invalid original file source")]
    InvalidSource,
    #[error("original exceeds the 8 GiB download limit")]
    TooLarge,
    #[error("another original download is using the available slots")]
    Busy,
    #[error("choose a new filename; the existing destination was preserved")]
    DestinationExists,
    #[error("the destination changed; its contents were preserved")]
    DestinationChanged,
    #[error("the file could not be saved")]
    Io,
    #[error("original file request failed")]
    Network(crate::slack::Error),
}

/// No Debug implementation: signed URLs stay in native memory.
#[derive(Clone, PartialEq, Eq)]
pub struct OriginalSource {
    pub(crate) url: url::Url,
    pub(crate) authenticated: bool,
    #[cfg(feature = "original-download-fixture")]
    loopback_fixture: bool,
}
impl OriginalSource {
    /// Disconnected native fixture only. No arbitrary host, credentials or redirects.
    #[cfg(feature = "original-download-fixture")]
    pub fn fixture_loopback(port: u16, nonce: &str) -> Result<Self, DownloadError> {
        if port == 0 || nonce.len() != 32 || !nonce.bytes().all(|c| c.is_ascii_hexdigit()) {
            return Err(DownloadError::InvalidSource);
        }
        Ok(Self {
            url: url::Url::parse(&format!("http://127.0.0.1:{port}/original/{nonce}"))
                .map_err(|_| DownloadError::InvalidSource)?,
            authenticated: false,
            loopback_fixture: true,
        })
    }
    pub(crate) fn accepts(&self, url: &url::Url) -> bool {
        #[cfg(feature = "original-download-fixture")]
        if self.loopback_fixture {
            return url == &self.url;
        }
        valid_url(url)
    }
    pub(crate) fn permits_redirect(&self) -> bool {
        #[cfg(feature = "original-download-fixture")]
        if self.loopback_fixture {
            return false;
        }
        true
    }

    pub fn parse(raw: &str) -> Result<Self, DownloadError> {
        let url = url::Url::parse(raw).map_err(|_| DownloadError::InvalidSource)?;
        if raw.len() > 16 * 1024 || !valid_url(&url) {
            return Err(DownloadError::InvalidSource);
        }
        let authenticated = url.host_str().is_some_and(|h| {
            h == "slack.com"
                || h.ends_with(".slack.com")
                || h == "slack-edge.com"
                || h.ends_with(".slack-edge.com")
        });
        Ok(Self {
            url,
            authenticated,
            #[cfg(feature = "original-download-fixture")]
            loopback_fixture: false,
        })
    }
}
pub(crate) fn valid_url(url: &url::Url) -> bool {
    let secure = url.scheme() == "https";
    #[cfg(test)]
    let secure = secure
        || (url.scheme() == "http" && matches!(url.host_str(), Some("127.0.0.1" | "localhost")));
    secure
        && url.host_str().is_some()
        && url.username().is_empty()
        && url.password().is_none()
        && url.fragment().is_none()
}

#[derive(Clone, PartialEq, Eq)]
struct Existing {
    length: u64,
    modified: std::time::SystemTime,
    created: Option<std::time::SystemTime>,
}
fn existing(path: &Path) -> Result<Option<Existing>, DownloadError> {
    match std::fs::symlink_metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Ok(m) if m.is_file() && !m.file_type().is_symlink() => Ok(Some(Existing {
            length: m.len(),
            modified: m.modified().map_err(|_| DownloadError::Io)?,
            created: m.created().ok(),
        })),
        _ => Err(DownloadError::Io),
    }
}
/// Streaming uses NewFile only. Cached bytes retain dialog-confirmed overwrite.
#[derive(Clone, Copy)]
pub enum SaveMode {
    NewFile,
    ConfirmedCachedOverwrite,
}
pub struct StagedFile {
    file: Option<tokio::fs::File>,
    path: Option<PathBuf>,
    destination: PathBuf,
    expected: Option<Existing>,
    mode: SaveMode,
}
impl StagedFile {
    pub async fn create(destination: PathBuf, mode: SaveMode) -> Result<Self, DownloadError> {
        tokio::task::spawn_blocking(move || {
            let name = destination.file_name().ok_or(DownloadError::Io)?;
            let parent = destination
                .parent()
                .ok_or(DownloadError::Io)?
                .canonicalize()
                .map_err(|_| DownloadError::Io)?;
            let destination = parent.join(name);
            let expected = existing(&destination)?;
            if expected.is_some() && matches!(mode, SaveMode::NewFile) {
                return Err(DownloadError::DestinationExists);
            }
            let path = parent.join(format!(
                ".super-platinum-download-{}.part",
                uuid::Uuid::new_v4()
            ));
            let mut options = std::fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let file = options.open(&path).map_err(|_| DownloadError::Io)?;
            Ok(Self {
                file: Some(tokio::fs::File::from_std(file)),
                path: Some(path),
                destination,
                expected,
                mode,
            })
        })
        .await
        .map_err(|_| DownloadError::Io)?
    }
    pub fn writer(&mut self) -> &mut tokio::fs::File {
        self.file.as_mut().expect("uncommitted stage")
    }
    pub async fn cached(
        &mut self,
        bytes: &[u8],
        current: impl Fn() -> bool,
    ) -> Result<(), DownloadError> {
        for chunk in bytes.chunks(WRITE_CHUNK) {
            if !current() {
                return Err(DownloadError::Canceled);
            }
            self.writer()
                .write_all(chunk)
                .await
                .map_err(|_| DownloadError::Io)?;
        }
        Ok(())
    }
    /// Final renderer ownership check precedes admission of the blocking commit.
    /// The cancellation latch is checked again on the filesystem worker.
    pub async fn commit(
        mut self,
        current: impl Fn() -> bool,
        canceled: Arc<AtomicBool>,
    ) -> Result<(), DownloadError> {
        self.writer().flush().await.map_err(|_| DownloadError::Io)?;
        self.writer()
            .sync_all()
            .await
            .map_err(|_| DownloadError::Io)?;
        if !current() {
            return Err(DownloadError::Canceled);
        }
        drop(self.file.take());
        tokio::task::spawn_blocking(move || {
            if canceled.load(Ordering::Acquire) {
                return Err(DownloadError::Canceled);
            }
            if existing(&self.destination)? != self.expected {
                return Err(DownloadError::DestinationChanged);
            }
            if canceled.load(Ordering::Acquire) {
                return Err(DownloadError::Canceled);
            }
            let path = self.path.as_ref().ok_or(DownloadError::Io)?;
            if matches!(self.mode, SaveMode::NewFile) || self.expected.is_none() {
                // Atomic no-clobber publication. Never fall back to visible partial copying.
                std::fs::hard_link(path, &self.destination).map_err(|e| {
                    if e.kind() == std::io::ErrorKind::AlreadyExists {
                        DownloadError::DestinationExists
                    } else {
                        DownloadError::Io
                    }
                })?;
                if std::fs::remove_file(path).is_ok() {
                    self.path = None;
                }
                return Ok(());
            } else {
                std::fs::rename(path, &self.destination).map_err(|_| DownloadError::Io)?;
            }
            self.path = None;
            Ok(())
        })
        .await
        .map_err(|_| DownloadError::Io)?
    }
}
impl Drop for StagedFile {
    fn drop(&mut self) {
        let Some(path) = self.path.take() else {
            return;
        };
        let file = self.file.take();
        let cleanup = move || {
            drop(file);
            let _ = std::fs::remove_file(path);
        };
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            runtime.spawn_blocking(cleanup);
        } else {
            let _ = std::thread::Builder::new()
                .name("download-cleanup".into())
                .spawn(cleanup);
        }
    }
}

#[cfg(test)]
mod tests;

#[cfg(all(test, feature = "original-download-fixture"))]
mod fixture_tests {
    use super::*;
    #[test]
    fn fixture_capability_is_exact_and_cannot_redirect_or_authenticate() {
        let source =
            OriginalSource::fixture_loopback(41234, "0123456789abcdef0123456789abcdef").unwrap();
        assert!(!source.authenticated && !source.permits_redirect());
        assert!(source.accepts(&source.url));
        for raw in [
            "http://localhost:41234/original/0123456789abcdef0123456789abcdef",
            "http://127.0.0.1:41235/original/0123456789abcdef0123456789abcdef",
            "http://127.0.0.1:41234/other",
            "https://files.slack.com/original",
        ] {
            assert!(!source.accepts(&url::Url::parse(raw).unwrap()));
        }
    }
    #[test]
    fn fixture_constructor_rejects_unbounded_identity_and_preserves_normal_source() {
        assert!(OriginalSource::fixture_loopback(0, "0123456789abcdef0123456789abcdef").is_err());
        for nonce in ["", "../secret", "0123456789abcdef0123456789abcdeg"] {
            assert!(OriginalSource::fixture_loopback(41234, nonce).is_err());
        }
        let normal = OriginalSource::parse("https://files.slack.com/original").unwrap();
        assert!(normal.authenticated && normal.permits_redirect());
        assert!(normal.accepts(&url::Url::parse("https://cdn.example.test/file").unwrap()));
    }
    #[tokio::test]
    async fn actual_fixture_redirect_is_not_followed_and_sends_no_cookie() {
        use std::time::Duration;
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let target = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let source = OriginalSource::fixture_loopback(
            origin.local_addr().unwrap().port(),
            "0123456789abcdef0123456789abcdef",
        )
        .unwrap();
        let location = format!("http://{}/escape", target.local_addr().unwrap());
        let response = tokio::spawn(async move {
            let (mut socket, _) = origin.accept().await.unwrap();
            let mut request = Vec::new();
            while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                let mut chunk = [0u8; 1024];
                let n = socket.read(&mut chunk).await.unwrap();
                assert!(n > 0 && request.len() + n <= 8192);
                request.extend_from_slice(&chunk[..n]);
            }
            socket.write_all(format!("HTTP/1.1 302 Found\r\nLocation: {location}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").as_bytes()).await.unwrap();
            String::from_utf8(request).unwrap().to_ascii_lowercase()
        });
        let root = std::env::temp_dir().join(format!("original-fixture-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let path = root.join("file.bin");
        let mut staged = StagedFile::create(path.clone(), SaveMode::NewFile)
            .await
            .unwrap();
        let transport = crate::slack::Transport::new("must-not-be-sent").unwrap();
        assert!(matches!(
            transport
                .download_original(&source, "fixture", staged.writer(), || true)
                .await,
            Err(DownloadError::InvalidSource)
        ));
        let request = response.await.unwrap();
        assert!(!request.contains("cookie:") && !request.contains("authorization:"));
        assert!(
            tokio::time::timeout(Duration::from_millis(100), target.accept())
                .await
                .is_err()
        );
        drop(staged);
        for _ in 0..100 {
            if std::fs::read_dir(&root).unwrap().next().is_none() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(!path.exists());
        std::fs::remove_dir(&root).unwrap();
    }
}
