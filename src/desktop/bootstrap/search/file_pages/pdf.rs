//! First-owned PDF staging/decoder admission. No File/ViewerVm/actions.
use super::*;
use super_platinum_core::{
    MediaAssetId, MediaAssetKind,
    file_preview::isolated_pdf::{MAX_PAGES, Projection},
    search::file_pages::Activation,
    slack::api::file_pdf::{self as pdf_api, Request},
};
#[cfg(test)]
pub(crate) mod test_io;
#[derive(Clone)]
pub(crate) struct Source {
    frame: Arc<Frame>,
    activation: Activation,
    index: usize,
    id: String,
    socket: u64,
    observed: Option<Arc<()>>,
}
impl Source {
    pub(crate) fn capture(s: &ShellState, index: usize) -> Option<Self> {
        let scene = scene(s)?;
        let activation = scene.book.active()?.clone();
        let hit = activation.page.hits().get(index)?;
        if scene.book.pending().is_some() || !visible(s, hit) || !pdf_api::valid_id(hit.id()) {
            return None;
        }
        let source = Self {
            frame: scene.frame.clone(),
            activation: activation.clone(),
            index,
            id: hit.id().into(),
            socket: s
                .core
                .workspaces
                .get(scene.frame.owner.team())?
                .rt_generation,
            observed: scene.pdf.as_ref().map(|p| p.nonce.clone()),
        };
        source.current(s).then_some(source)
    }
    fn current(&self, s: &ShellState) -> bool {
        s.overlay == Some(Overlay::Search)
            && owned(s, &self.frame).is_some_and(|v| {
                v.book.pending().is_none() && v.book.activation_current(&self.activation)
            })
            && s.core
                .workspaces
                .get(self.frame.owner.team())
                .is_some_and(|w| w.rt_generation == self.socket)
            && self
                .activation
                .page
                .hits()
                .get(self.index)
                .is_some_and(|h| h.id() == self.id && visible(s, h))
    }
    fn control_current(&self, s: &ShellState) -> bool {
        self.current(s)
            && match (
                &self.observed,
                owned(s, &self.frame).and_then(|v| v.pdf.as_ref()),
            ) {
                (None, None) => true,
                (Some(old), Some(p)) => Arc::ptr_eq(old, &p.nonce),
                _ => false,
            }
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Loading,
    Unavailable,
    Failed,
    Staged,
    Ready,
}
pub(super) struct Attempt {
    source: Source,
    nonce: Arc<()>,
    route_nonce: String,
    id: MediaAssetId,
    phase: Phase,
    bytes: Option<Arc<[u8]>>,
    pages: Option<usize>,
    native_decode: bool,
}
#[derive(Clone)]
pub(crate) enum Display {
    Loading,
    Unavailable,
    Failed,
    Document {
        id: MediaAssetId,
        nonce: String,
        ready: bool,
    },
}
#[derive(Clone)]
struct Job {
    source: Source,
    nonce: Arc<()>,
    rate: Arc<Rate>,
}
impl Job {
    fn current(&self, s: &ShellState) -> bool {
        self.source.current(s)
            && owned(s, &self.source.frame)
                .and_then(|v| v.pdf.as_ref())
                .is_some_and(|p| Arc::ptr_eq(&p.nonce, &self.nonce) && p.phase == Phase::Loading)
    }
}
pub(crate) fn display(s: &ShellState, index: usize) -> Option<Display> {
    let p = scene(s)?.pdf.as_ref()?;
    if p.source.index != index || !p.source.current(s) {
        return None;
    }
    Some(match p.phase {
        Phase::Loading => Display::Loading,
        Phase::Unavailable => Display::Unavailable,
        Phase::Failed => Display::Failed,
        Phase::Staged | Phase::Ready => Display::Document {
            id: p.id.clone(),
            nonce: p.route_nonce.clone(),
            ready: p.phase == Phase::Ready,
        },
    })
}
pub(crate) fn cooling(s: &ShellState) -> bool {
    s.search_file_module
        .as_ref()
        .is_none_or(|m| m.rate.cooling())
}
pub(crate) fn fixture(s: &mut ShellState) {
    s.overlay = Some(Overlay::Search);
    s.set_search_kind(SearchKind::Files);
    s.set_search_query("PDF notes".into());
    s.search_file_pages_fixture = Some(false);
    s.search_file_pdf_fixture = true;
}
pub(crate) fn layout(s: &ShellState, index: usize) -> bool {
    index == 0
        && s.search_file_pdf_fixture
        && s.core.session.is_none()
        && s.core.transport.is_none()
        && s.core.active_account.is_none()
        && s.search_file_module.is_none()
}
fn begin(s: &mut ShellState, source: Source) -> Option<Job> {
    if !source.control_current(s) || cooling(s) {
        return None;
    }
    if owned(s, &source.frame)
        .and_then(|v| v.pdf.as_ref())
        .is_some_and(|p| p.phase == Phase::Loading)
    {
        return None;
    }
    let rate = s.search_file_module.as_ref()?.rate.clone();
    let nonce = Arc::new(());
    let id = MediaAssetId::new(MediaAssetKind::Attachment);
    let route_nonce = MediaAssetId::new(MediaAssetKind::Attachment)
        .opaque()
        .to_owned();
    owned_mut(s, &source.frame)?.pdf = Some(Attempt {
        source: source.clone(),
        nonce: nonce.clone(),
        route_nonce,
        id,
        phase: Phase::Loading,
        bytes: None,
        pages: None,
        native_decode: false,
    });
    Some(Job {
        source,
        nonce,
        rate,
    })
}
pub(crate) fn open(mut state: Signal<ShellState>, source: Source) {
    let job = state
        .try_write()
        .ok()
        .and_then(|mut s| begin(&mut s, source));
    if let Some(job) = job {
        dioxus::core::spawn_forever(run(state, job));
    }
}
pub(crate) fn close(mut state: Signal<ShellState>, source: Source) {
    let Ok(mut s) = state.try_write() else { return };
    if !source.control_current(&s) {
        return;
    }
    if let Some(scene) = owned_mut(&mut s, &source.frame) {
        scene.pdf = None;
    }
}
/// Decoder-only staging; never inserted into MediaRegistry or a ViewerVm.
pub(crate) fn bytes(s: &ShellState, nonce: &str, opaque: &str) -> Option<Arc<[u8]>> {
    let p = scene(s)?.pdf.as_ref()?;
    if p.route_nonce != nonce
        || p.id.opaque() != opaque
        || !p.source.current(s)
        || !matches!(p.phase, Phase::Staged | Phase::Ready)
    {
        return None;
    }
    p.bytes.clone()
}
/// Native/model provenance comes from the renderer's Rust IO discriminator.
pub(crate) fn decoded(
    mut state: Signal<ShellState>,
    source: Source,
    pages: usize,
    native: bool,
) -> bool {
    let Ok(mut s) = state.try_write() else {
        return false;
    };
    if !source.control_current(&s) || pages == 0 || pages > MAX_PAGES {
        return false;
    }
    let Some(p) = owned_mut(&mut s, &source.frame).and_then(|v| v.pdf.as_mut()) else {
        return false;
    };
    if p.phase != Phase::Staged || p.bytes.is_none() {
        return false;
    }
    p.phase = Phase::Ready;
    p.pages = Some(pages);
    p.native_decode = native;
    true
}
pub(crate) fn failed(mut state: Signal<ShellState>, source: Source) {
    let Ok(mut s) = state.try_write() else { return };
    if !source.control_current(&s) {
        return;
    }
    if let Some(p) = owned_mut(&mut s, &source.frame).and_then(|v| v.pdf.as_mut()) {
        p.phase = Phase::Failed;
        p.bytes = None;
        p.pages = None;
        p.native_decode = false;
    }
}
#[cfg(test)]
pub(crate) fn decoded_fact(s: &ShellState, index: usize) -> Option<(usize, bool)> {
    let p = scene(s)?.pdf.as_ref()?;
    if p.source.index != index || !p.source.current(s) || p.phase != Phase::Ready {
        return None;
    }
    Some((p.pages?, p.native_decode))
}
pub(super) fn poll(s: &mut ShellState) -> bool {
    let retire = s
        .search_file_module
        .as_ref()
        .and_then(|m| m.scene.as_ref())
        .and_then(|v| v.pdf.as_ref())
        .is_some_and(|p| !p.source.current(s));
    if retire {
        if let Some(scene) = s.search_file_module.as_mut().and_then(|m| m.scene.as_mut()) {
            scene.pdf = None;
        }
        return true;
    }
    false
}
async fn execute(
    state: Signal<ShellState>,
    job: &Job,
    request: Request,
) -> Result<serde_json::Value, Error> {
    let current = || state.try_peek().is_ok_and(|s| job.current(&s));
    if !current() {
        return Err(Error::Offline("PDF preview retired".into()));
    }
    #[cfg(test)]
    let result = test_io::get().execute(request).await;
    #[cfg(not(test))]
    let result = job
        .source
        .frame
        .parameters
        .transport()
        .execute_file_pdf_current(&request, current)
        .await;
    if let Ok(s) = state.try_peek() {
        job.rate.observe(&s, &result)
    }
    result
}
async fn load(state: Signal<ShellState>, job: &Job) -> Result<Option<Arc<[u8]>>, Error> {
    let f = &job.source.frame;
    let raw = execute(
        state,
        job,
        Request::metadata(
            f.parameters.client(),
            f.parameters.session(),
            &job.source.id,
        )?,
    )
    .await?;
    if !state.try_peek().is_ok_and(|s| job.current(&s)) {
        return Ok(None);
    }
    let Ok(projection) = Projection::from_fresh(&raw, &job.source.id) else {
        return Ok(None);
    };
    let ip = execute(
        state,
        job,
        Request::ip(
            f.parameters.client(),
            f.parameters.session(),
            &job.source.id,
        )?,
    )
    .await?;
    if !state.try_peek().is_ok_and(|s| job.current(&s)) || !pdf_api::ip_allowed(&ip) {
        return Ok(None);
    }
    let current = || state.try_peek().is_ok_and(|s| job.current(&s));
    #[cfg(test)]
    let received = test_io::get().bytes(projection.url()).await;
    #[cfg(not(test))]
    let received = f
        .parameters
        .transport()
        .get_file_pdf_current(&projection, current)
        .await;
    if let Err(error) = &received {
        if let Ok(s) = state.try_peek() {
            job.rate.observe(&s, &Err(error.clone()))
        }
    }
    let bytes = received?;
    if !current() {
        return Ok(None);
    }
    // This is a bounded container precheck, not a decoder/ready receipt. The
    // actual bundled PDF.js decode must still pass the separate mounted gate.
    if projection.validate_bytes(&bytes).is_err() {
        return Ok(None);
    }
    Ok(Some(Arc::from(bytes)))
}
async fn run(mut state: Signal<ShellState>, job: Job) {
    let result = load(state, &job).await;
    let Ok(mut s) = state.try_write() else { return };
    if !job.current(&s) {
        return;
    }
    let (phase, bytes) = match result {
        Ok(Some(bytes)) => (Phase::Staged, Some(bytes)),
        Ok(None) => (Phase::Unavailable, None),
        Err(error) => {
            s.report_failure(&error, "PDF preview needs a connection", || {
                "PDF preview couldn't load. Your file results are retained.".into()
            });
            (Phase::Failed, None)
        }
    };
    let p = owned_mut(&mut s, &job.source.frame)
        .and_then(|v| v.pdf.as_mut())
        .expect("checked original PDF attempt");
    p.phase = phase;
    p.bytes = bytes;
}
