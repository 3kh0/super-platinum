//! Explicit refresh of one open hosted text original. Local order is not a server version.
use crate::{
    media::MediaRegistry,
    state::{Overlay, ShellState, ViewerVm},
};
use dioxus::prelude::*;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};
use super_platinum_core::{
    MediaAssetId, file_actions, file_preview,
    slack::{Error, api, models::File},
};
#[cfg(all(
    feature = "original-download-native-fixture",
    feature = "protected-native-fixture"
))]
pub(crate) mod check;
pub(crate) mod fixture;
mod source;
#[cfg(test)]
pub(crate) mod tests;

#[derive(Clone)]
pub(crate) struct Lease(Arc<Visit>);
struct Visit {
    asset: MediaAssetId,
    file: String,
    team: String,
    account: Option<String>,
    epoch: u64,
    live: AtomicBool,
    canceled: Arc<AtomicBool>,
    _asset: Option<crate::media::FreshOriginal>,
}
impl PartialEq for Lease {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}
impl Lease {
    pub(crate) fn current(&self, shell: &ShellState) -> bool {
        self.0.live.load(Ordering::Acquire)
            && shell.core.account_epoch == self.0.epoch
            && shell.core.active_account == self.0.account
            && shell.core.active_team.as_ref() == Some(&self.0.team)
            && shell.overlay == Some(Overlay::Viewer)
            && shell
                .file_refresh
                .active
                .as_ref()
                .is_some_and(|a| a.lease == *self)
            && shell.viewer.as_ref().is_some_and(|v| {
                v.id == self.0.asset && v.file.as_ref().is_some_and(|f| f.id == self.0.file)
            })
            && shell
                .core
                .workspaces
                .get(&self.0.team)
                .is_some_and(|w| !w.deleted_files.contains(&self.0.file))
    }
    pub(crate) fn key(&self) -> String {
        format!("refresh-{:p}", Arc::as_ptr(&self.0))
    }
    pub(crate) fn cancellation(&self) -> Arc<AtomicBool> {
        self.0.canceled.clone()
    }
    fn retire(&self) {
        self.0.live.store(false, Ordering::Release);
        self.0.canceled.store(true, Ordering::Release);
    }
}
#[derive(Default)]
pub(crate) struct State {
    active: Option<Active>,
    retry_at: Option<Instant>,
    retry_exhausted: bool,
}
struct Active {
    lease: Lease,
    revision: u64,
    attempt: u64,
    exhausted: bool,
    busy: bool,
    notice: &'static str,
    held_preview: Option<Arc<[u8]>>,
}
impl Drop for State {
    fn drop(&mut self) {
        if let Some(a) = &self.active {
            a.lease.retire();
        }
    }
}
impl State {
    pub(crate) fn close(&mut self) {
        if let Some(old) = self.active.take() {
            old.lease.retire();
        }
    }
}
pub(crate) fn close(shell: &mut ShellState) {
    shell.file_refresh.close();
}
pub(crate) fn opened(shell: &mut ShellState) {
    install(shell, None);
}
fn install(shell: &mut ShellState, asset: Option<crate::media::FreshOriginal>) {
    close(shell);
    let Some(viewer) = shell.viewer.as_ref() else {
        return;
    };
    let Some(file) = &viewer.file else {
        return;
    };
    let Some(team) = shell.core.active_team.clone() else {
        return;
    };
    if !source::text_mime(&viewer.mime)
        || !file.renameable
        || !shell
            .media
            .original_download_url(&viewer.id)
            .is_some_and(|u| source::hosted(&u))
    {
        return;
    }
    let lease = Lease(Arc::new(Visit {
        asset: viewer.id.clone(),
        file: file.id.clone(),
        team,
        account: shell.core.active_account.clone(),
        epoch: shell.core.account_epoch,
        live: AtomicBool::new(true),
        canceled: Arc::new(AtomicBool::new(false)),
        _asset: asset,
    }));
    shell.file_refresh.active = Some(Active {
        lease,
        revision: 0,
        attempt: 0,
        exhausted: false,
        busy: false,
        notice: "",
        held_preview: None,
    });
}
pub(crate) fn retained_preview(shell: &ShellState, id: &MediaAssetId) -> bool {
    shell.file_refresh.active.as_ref().is_some_and(|a| {
        a.lease.0.asset == *id && a.lease.current(shell) && a.held_preview.is_some()
    })
}
pub(crate) fn lease(shell: &ShellState, id: &MediaAssetId) -> Option<Lease> {
    let a = shell.file_refresh.active.as_ref()?;
    (a.lease.0.asset == *id && a.lease.current(shell)).then(|| a.lease.clone())
}
/// Validated remote or local metadata invalidates this observation, not old bytes.
pub(crate) fn changed(shell: &mut ShellState, team: &str, file: &str) {
    let Some(a) = shell.file_refresh.active.as_mut() else {
        return;
    };
    if a.lease.0.team != team || a.lease.0.file != file {
        return;
    }
    match a.revision.checked_add(1) {
        Some(n) => a.revision = n,
        None => a.exhausted = true,
    }
    a.notice = "This file may have changed. Refresh to fetch its contents again.";
}

struct Ticket {
    lease: Lease,
    scope: source::Scope,
    registry: MediaRegistry,
    revision: u64,
    attempt: u64,
}
impl Ticket {
    fn begin(shell: &mut ShellState, lease: Lease) -> Option<Self> {
        if !lease.current(shell) {
            return None;
        }
        let scope = source::Scope::capture(shell)?;
        // Hold the old visible text through a failed attempt even if unrelated
        // media pressure evicts its shared cache entry while the read awaits.
        let held = shell
            .file_refresh
            .active
            .as_ref()?
            .held_preview
            .clone()
            .or_else(|| shell.media.ready_bytes(&lease.0.asset))?;
        let retry_at = shell.file_refresh.retry_at;
        let retry_exhausted = shell.file_refresh.retry_exhausted;
        let a = shell.file_refresh.active.as_mut()?;
        if a.busy || a.exhausted || retry_exhausted {
            return None;
        }
        if retry_at.is_some_and(|at| Instant::now() < at) {
            a.notice = "The server asked us to wait. Try refreshing shortly.";
            return None;
        }
        let Some(attempt) = a.attempt.checked_add(1) else {
            a.exhausted = true;
            return None;
        };
        a.held_preview = Some(held);
        a.attempt = attempt;
        a.busy = true;
        a.notice = "Fetching contents. The open preview is unchanged.";
        Some(Self {
            lease,
            scope,
            registry: shell.media.clone(),
            revision: a.revision,
            attempt,
        })
    }
    fn owns(&self, shell: &ShellState) -> bool {
        self.lease.current(shell)
            && self.scope.current(shell)
            && self.registry.same_registry(&shell.media)
            && shell
                .file_refresh
                .active
                .as_ref()
                .is_some_and(|a| a.attempt == self.attempt && a.busy)
    }
    fn current(&self, shell: &ShellState) -> bool {
        self.owns(shell)
            && shell
                .file_refresh
                .active
                .as_ref()
                .is_some_and(|a| !a.exhausted && a.revision == self.revision)
    }
    fn fail(&self, shell: &mut ShellState, message: &'static str, retry: Option<Duration>) {
        if !self.lease.current(shell)
            || !shell
                .file_refresh
                .active
                .as_ref()
                .is_some_and(|a| a.attempt == self.attempt && a.busy)
        {
            return;
        }
        let a = shell.file_refresh.active.as_mut().unwrap();
        a.busy = false;
        a.notice = if a.revision != self.revision {
            "This file changed during refresh. Refresh again."
        } else {
            message
        };
        if let Some(delay) = retry {
            match Instant::now().checked_add(delay) {
                Some(at) => shell.file_refresh.retry_at = Some(at),
                None => shell.file_refresh.retry_exhausted = true,
            }
        }
    }
    fn publish(
        &self,
        shell: &mut ShellState,
        file: &File,
        source: &source::Source,
        bytes: Arc<[u8]>,
    ) -> bool {
        if !self.current(shell) {
            return false;
        }
        let asset = shell
            .media
            .insert_fresh_original(&source.url, &source.mime, bytes);
        let id = asset.id().clone();
        let old = shell.viewer.as_ref().expect("current viewer");
        // Display fields have independent authority; a content read may omit them.
        // This never fills missing URL/MIME/source fields from the older snapshot.
        let name = if file
            .title
            .as_deref()
            .filter(|v| !v.trim().is_empty())
            .or(file.name.as_deref().filter(|v| !v.trim().is_empty()))
            .is_some()
        {
            super_platinum_core::state::file_title(file)
        } else {
            old.name.clone()
        };
        let download_name = if file.name.as_deref().is_some_and(|v| !v.trim().is_empty()) {
            super_platinum_core::state::file_download_name(file)
        } else {
            old.download_name.clone()
        };
        // Synchronously trip the exact old DownloadButton's commit latch before replacing its key.
        self.lease.retire();
        shell.viewer = Some(ViewerVm {
            file: file_actions::identity(file),
            id,
            name,
            download_name,
            mime: source.mime.clone(),
        });
        install(shell, Some(asset));
        if let Some(a) = shell.file_refresh.active.as_mut() {
            a.notice = "Contents refreshed.";
        }
        true
    }
    async fn run(self, mut state: Signal<ShellState>) {
        let result = self.read_metadata(state).await;
        if !self.current(&state.peek()) {
            self.fail(
                &mut state.write(),
                "Refresh no longer belongs to this preview.",
                None,
            );
            return;
        }
        let file = match result {
            Ok(file)
                if file.id.as_deref() == Some(&self.lease.0.file)
                    && file_actions::deleted(&file) =>
            {
                state
                    .write()
                    .core
                    .remove_file(&self.lease.0.team, &self.lease.0.file);
                state.write().refresh_from_core();
                return;
            }
            Err(Error::Api(code)) if code == "file_deleted" => {
                state
                    .write()
                    .core
                    .remove_file(&self.lease.0.team, &self.lease.0.file);
                state.write().refresh_from_core();
                return;
            }
            Ok(file) => file,
            Err(error) => {
                self.error(&mut state.write(), &error);
                return;
            }
        };
        let Some(source) = source::Source::parse(&file, &self.lease.0.file) else {
            self.fail(
                &mut state.write(),
                "Fresh text contents are unavailable. The open preview is unchanged.",
                None,
            );
            return;
        };
        let bytes = match self.read_body(state, &source.url).await {
            Ok(bytes) => bytes,
            Err(error) => {
                self.error(&mut state.write(), &error);
                return;
            }
        };
        if !self.current(&state.peek()) {
            self.fail(
                &mut state.write(),
                "Refresh no longer belongs to this preview.",
                None,
            );
            return;
        }
        // The existing bounded decoder inspects only its capped prefix; do that off the renderer.
        let decoded = tokio::task::spawn_blocking(move || {
            file_preview::decode(&bytes).map(|_| Arc::<[u8]>::from(bytes))
        })
        .await;
        match decoded {
            Ok(Ok(bytes)) => {
                if !self.publish(&mut state.write(), &file, &source, bytes) {
                    self.fail(
                        &mut state.write(),
                        "Refresh no longer belongs to this preview.",
                        None,
                    );
                }
            }
            _ => self.fail(
                &mut state.write(),
                "Fresh contents could not be read as text. The open preview is unchanged.",
                None,
            ),
        }
    }
    async fn read_metadata(&self, state: Signal<ShellState>) -> Result<File, Error> {
        #[cfg(all(
            feature = "original-download-native-fixture",
            feature = "protected-native-fixture"
        ))]
        if check::enabled() {
            return check::metadata(self, &state.peek());
        }
        #[cfg(all(
            feature = "original-download-native-fixture",
            feature = "protected-native-fixture"
        ))]
        if crate::download::fixture::cached::enabled() {
            if !self.current(&state.peek())
                || self.lease.0.file != crate::download::fixture::cached::FILE
            {
                return Err(Error::Api("cached_save_fixture_refused".into()));
            }
            return crate::download::fixture::cached::metadata(&state.peek());
        }
        let _ = state;
        api::fetch_file(
            &self.scope.context.transport,
            &self.scope.context.client,
            &self.scope.context.session,
            &self.lease.0.file,
        )
        .await
    }
    async fn read_body(&self, state: Signal<ShellState>, url: &str) -> Result<Vec<u8>, Error> {
        #[cfg(all(
            feature = "original-download-native-fixture",
            feature = "protected-native-fixture"
        ))]
        if check::enabled() {
            return check::body(self, state, url).await;
        }
        #[cfg(all(
            feature = "original-download-native-fixture",
            feature = "protected-native-fixture"
        ))]
        if crate::download::fixture::cached::enabled() {
            if !self.current(&state.peek())
                || self.lease.0.file != crate::download::fixture::cached::FILE
            {
                return Err(Error::Api("cached_save_fixture_refused".into()));
            }
            return crate::download::fixture::cached::body(state, url).await;
        }
        let _ = state;
        let user_agent = super_platinum_core::slack::xparams::Identity::from_capture().user_agent;
        self.scope
            .context
            .transport
            .get_bytes(url, &user_agent)
            .await
    }
    fn error(&self, shell: &mut ShellState, error: &Error) {
        let retry = match error {
            Error::RateLimited { retry_after_secs } => {
                Some(Duration::from_secs(retry_after_secs.unwrap_or(30).max(1)))
            }
            _ => None,
        };
        self.fail(
            shell,
            if retry.is_some() {
                "The server asked us to wait. Try refreshing shortly."
            } else {
                "Could not refresh contents. The open preview is unchanged. Try again."
            },
            retry,
        );
    }
}

#[component]
pub(crate) fn Controls(mut state: Signal<ShellState>, lease: Lease) -> Element {
    let (busy, notice, disabled) = {
        let shell = state.read();
        if !lease.current(&shell) {
            return rsx! {};
        }
        let a = shell.file_refresh.active.as_ref().unwrap();
        (
            a.busy,
            a.notice,
            a.exhausted
                || shell.file_refresh.retry_exhausted
                || !source::available(&shell)
                || (!retained_preview(&shell, &lease.0.asset)
                    && shell.media.ready_bytes(&lease.0.asset).is_none()),
        )
    };
    rsx! {div {class:"file-refresh", "data-refresh-busy":"{busy}",
        button {r#type:"button",id:"file-preview-refresh",disabled:busy||disabled,
            onclick:move |_| {let ticket=Ticket::begin(&mut state.write(),lease.clone());if let Some(ticket)=ticket {spawn(async move {ticket.run(state).await;});}},
            "Refresh contents"
        }
        p {role:"status","{notice}"}
    }}
}
