//! Opt-in synthetic inputs. Original-stream fixtures never install ShellState credentials;
//! the separate cached fixture explicitly owns a synthetic Session/disconnected Transport.
use super::*;
use std::{
    path::PathBuf,
    sync::{Mutex, OnceLock, atomic::AtomicUsize},
};
use super_platinum_core::{MediaAssetKind, slack::Transport};
#[cfg(feature = "protected-native-fixture")]
pub(crate) mod cached;
pub(crate) mod check;
pub(crate) mod dialog;
pub(crate) mod workflow;
pub const BYTES: u64 = 65 * 1024 * 1024 + 7;
pub const NAME: &str = "original-fixture.bin";
static RUN: OnceLock<Arc<Run>> = OnceLock::new();

pub(crate) struct Run {
    pub root: PathBuf,
    pub nonce: String,
    pub alias: String,
    pub id: MediaAssetId,
    url: String,
    source: OriginalSource,
    transport: Arc<Transport>,
    media: crate::media::MediaRegistry,
    epoch: u64,
    issued: AtomicUsize,
    outcomes: Mutex<Vec<(usize, &'static str)>>,
}
#[derive(Clone)]
pub(super) struct Input {
    run: Arc<Run>,
    attempt: Arc<AtomicUsize>,
}
pub(crate) fn enabled() -> bool {
    (matches!(
        std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
        Ok("original-download" | "original-download-narrow")
    ) || dialog::enabled()
        || workflow::enabled())
        && std::env::var("SUPER_PLATINUM_ORIGINAL_DOWNLOAD_CHECK").as_deref() == Ok("1")
}
pub(super) fn capture(shell: &ShellState, id: &MediaAssetId) -> Option<Input> {
    if !enabled() {
        return None;
    }
    capture_from(RUN.get()?, shell, id)
}
fn capture_from(run: &Arc<Run>, shell: &ShellState, id: &MediaAssetId) -> Option<Input> {
    admitted(run, shell, id).then(|| Input {
        run: run.clone(),
        attempt: Arc::new(AtomicUsize::new(0)),
    })
}
pub(super) fn available(shell: &ShellState, id: &MediaAssetId) -> bool {
    enabled() && RUN.get().is_some_and(|run| admitted(run, shell, id))
}
fn admitted(run: &Run, shell: &ShellState, id: &MediaAssetId) -> bool {
    shell.core.session.is_none()
        && shell.core.transport.is_none()
        && shell.core.active_account.is_none()
        && shell.core.active_team.as_deref() == Some("T1")
        && shell.core.account_epoch == run.epoch
        && &run.id == id
        && shell.media.same_registry(&run.media)
        && shell.media.original_download_url(id).as_deref() == Some(run.url.as_str())
}
impl Input {
    pub fn destination(&self) -> Option<PathBuf> {
        self.attempt
            .compare_exchange(0, usize::MAX, Ordering::AcqRel, Ordering::Acquire)
            .ok()?;
        let prior = self
            .run
            .issued
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                (n < 3).then_some(n + 1)
            })
            .ok()?;
        self.attempt.store(prior + 1, Ordering::Release);
        Some(
            self.run
                .root
                .join("downloads")
                .join(format!("attempt-{}.bin", prior + 1)),
        )
    }
    pub fn network(&self) -> (OriginalSource, Arc<Transport>) {
        (self.run.source.clone(), self.run.transport.clone())
    }
    pub fn completed(&self, result: &Result<(), DownloadError>) {
        if self.native_workflow() {
            self.workflow_completed(result);
            return;
        }
        let outcome = match result {
            Ok(()) => "saved",
            Err(DownloadError::Canceled) => "canceled",
            Err(_) => "error",
        };
        if let Ok(mut rows) = self.run.outcomes.lock() {
            if rows.len() < 3 {
                rows.push((self.attempt.load(Ordering::Acquire), outcome));
            }
        }
    }
}
impl Run {
    pub fn outcomes(&self) -> Result<Vec<(usize, &'static str)>, String> {
        self.outcomes
            .lock()
            .map(|rows| rows.clone())
            .map_err(|_| "outcome lock".into())
    }
    pub fn issued(&self) -> usize {
        self.issued.load(Ordering::Acquire)
    }
}
pub(crate) async fn install(mut state: Signal<ShellState>) -> Result<Arc<Run>, String> {
    if !enabled() {
        return Err("exact feature/alias/check required".into());
    }
    let root = PathBuf::from(
        std::env::var("SUPER_PLATINUM_ORIGINAL_DOWNLOAD_DIR").map_err(|_| "fixture directory")?,
    );
    let nonce =
        std::env::var("SUPER_PLATINUM_ORIGINAL_DOWNLOAD_NONCE").map_err(|_| "fixture nonce")?;
    let port: u16 = std::env::var("SUPER_PLATINUM_ORIGINAL_DOWNLOAD_PORT")
        .map_err(|_| "fixture port")?
        .parse()
        .map_err(|_| "fixture port")?;
    let source = OriginalSource::fixture_loopback(port, &nonce).map_err(|_| "fixture source")?;
    let folder = root.clone();
    tokio::task::spawn_blocking(move || -> Result<(), String> {
        if !folder.is_absolute() {
            return Err("absolute fixture directory required".into());
        }
        for path in folder.ancestors() {
            if std::fs::symlink_metadata(path)
                .map_err(|_| "fixture path")?
                .file_type()
                .is_symlink()
            {
                return Err("linked fixture directory".into());
            }
        }
        let files = folder.join("downloads");
        if std::fs::symlink_metadata(&files)
            .map_err(|_| "download directory")?
            .file_type()
            .is_symlink()
            || std::fs::read_dir(files)
                .map_err(|_| "download directory")?
                .next()
                .is_some()
        {
            return Err("fresh unlinked empty download directory required".into());
        }
        Ok(())
    })
    .await
    .map_err(|_| "fixture directory worker")??;
    let transport = Arc::new(Transport::new("").map_err(|_| "fixture transport")?);
    let mut shell = state.write();
    if shell.core.session.is_some()
        || shell.core.transport.is_some()
        || shell.core.active_account.is_some()
        || shell.core.active_team.as_deref() != Some("T1")
    {
        return Err("disconnected fixture required".into());
    }
    let url = format!("http://127.0.0.1:{port}/original/{nonce}");
    let id =
        shell
            .media
            .register_deferred(MediaAssetKind::Attachment, &url, "application/octet-stream");
    shell.media.register_file_download(
        &id,
        &File {
            url_private: Some(url.clone()),
            ..Default::default()
        },
    );
    let run = Arc::new(Run {
        root,
        nonce,
        alias: std::env::var("SUPER_PLATINUM_FIXTURE").unwrap_or_default(),
        id: id.clone(),
        url,
        source,
        transport,
        media: shell.media.clone(),
        epoch: shell.core.account_epoch,
        issued: AtomicUsize::new(0),
        outcomes: Mutex::new(Vec::new()),
    });
    RUN.set(run.clone())
        .map_err(|_| "fixture already initialized")?;
    shell.viewer = Some(crate::state::ViewerVm {
        file: None,
        id,
        name: NAME.into(),
        download_name: NAME.into(),
        mime: "application/octet-stream".into(),
    });
    shell.overlay = Some(crate::state::Overlay::Viewer);
    Ok(run)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn case() -> (ShellState, Arc<Run>) {
        let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
        shell.core.active_account = None;
        let nonce = "0123456789abcdef0123456789abcdef".to_owned();
        let url = format!("http://127.0.0.1:41234/original/{nonce}");
        let id = shell.media.register_deferred(
            MediaAssetKind::Attachment,
            &url,
            "application/octet-stream",
        );
        shell.media.register_file_download(
            &id,
            &File {
                url_private: Some(url.clone()),
                ..Default::default()
            },
        );
        let run = Arc::new(Run {
            root: PathBuf::from("unused-fixture-path"),
            nonce: nonce.clone(),
            alias: "original-download".into(),
            id,
            url,
            source: OriginalSource::fixture_loopback(41234, &nonce).unwrap(),
            transport: Arc::new(Transport::new("").unwrap()),
            media: shell.media.clone(),
            epoch: shell.core.account_epoch,
            issued: AtomicUsize::new(0),
            outcomes: Mutex::new(Vec::new()),
        });
        (shell, run)
    }
    #[test]
    fn synthetic_admission_requires_exact_disconnected_asset_and_owner() {
        for n in 0..5 {
            let (mut shell, run) = case();
            assert!(capture_from(&run, &shell, &run.id).is_some());
            match n {
                0 => shell.core.account_epoch += 1,
                1 => shell.core.active_account = Some("unrelated".into()),
                2 => shell.core.transport = Some(Arc::new(Transport::new("").unwrap())),
                3 => shell.media = crate::media::MediaRegistry::default(),
                _ => shell.media.register_file_download(
                    &run.id,
                    &File {
                        url_private: Some("https://files.slack.com/other".into()),
                        ..Default::default()
                    },
                ),
            }
            assert!(capture_from(&run, &shell, &run.id).is_none());
        }
        let (shell, run) = case();
        assert!(
            capture_from(&run, &shell, &MediaAssetId::new(MediaAssetKind::Attachment)).is_none()
        );
        assert!(shell.core.session.is_none() && shell.core.transport.is_none());
    }
    #[test]
    fn destinations_are_three_bounded_one_use_attempts_with_exact_results() {
        let (shell, run) = case();
        for n in 1..=3 {
            let input = capture_from(&run, &shell, &run.id).unwrap();
            assert_eq!(
                input.destination().unwrap(),
                run.root.join("downloads").join(format!("attempt-{n}.bin"))
            );
            assert!(input.clone().destination().is_none());
            input.completed(&Err(DownloadError::Canceled));
        }
        assert!(
            capture_from(&run, &shell, &run.id)
                .unwrap()
                .destination()
                .is_none()
        );
        assert_eq!(run.issued(), 3);
        assert_eq!(
            run.outcomes().unwrap(),
            vec![(1, "canceled"), (2, "canceled"), (3, "canceled")]
        );
    }
}
