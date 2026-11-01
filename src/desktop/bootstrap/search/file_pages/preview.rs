//! Occurrence-owned image-only expansion. No File/ViewerVm/action conversion.
use super::*;
use super_platinum_core::{
    MediaAssetId, MediaAssetKind,
    file_preview::isolated_image::Projection,
    search::file_pages::Activation,
    slack::api::file_image::{self, Request},
};
#[cfg(test)]
pub(crate) mod test_io;
#[cfg(test)]
mod tests;

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
        if scene.book.pending().is_some() || !visible(s, hit) || !file_image::valid_id(hit.id()) {
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
            observed: scene.preview.as_ref().map(|v| v.nonce.clone()),
        };
        source.current(s).then_some(source)
    }
    fn current(&self, s: &ShellState) -> bool {
        owned(s, &self.frame).is_some_and(|scene| scene.book.activation_current(&self.activation))
            && s.core
                .workspaces
                .get(self.frame.owner.team())
                .is_some_and(|w| w.rt_generation == self.socket)
            && self
                .activation
                .page
                .hits()
                .get(self.index)
                .is_some_and(|hit| hit.id() == self.id && visible(s, hit))
    }
    fn control_current(&self, s: &ShellState) -> bool {
        self.current(s)
            && match (
                &self.observed,
                owned(s, &self.frame).and_then(|v| v.preview.as_ref()),
            ) {
                (None, None) => true,
                (Some(old), Some(v)) => Arc::ptr_eq(old, &v.nonce),
                _ => false,
            }
    }
}
#[derive(Clone)]
pub(crate) enum Display {
    Loading,
    Unavailable,
    Failed,
    Image(MediaAssetId),
}
pub(super) struct Attempt {
    source: Source,
    nonce: Arc<()>,
    display: Display,
    dom_key: String,
}
/// Only an explicit Close of an actually committed, ready Image can seal this
/// private receipt. It is not a File/canonical-store or cached-IP capability.
pub(super) struct ClosedImage {
    source: Source,
    id: MediaAssetId,
}
impl ClosedImage {
    fn matches(&self, s: &ShellState, source: &Source) -> bool {
        self.source.current(s)
            && source.current(s)
            && Arc::ptr_eq(&self.source.frame, &source.frame)
            && self.source.index == source.index
            && self.source.id == source.id
    }
}
enum Content {
    Fresh(Vec<u8>),
    Reused(MediaAssetId),
}
#[derive(Clone)]
struct Job {
    source: Source,
    nonce: Arc<()>,
    rate: Arc<Rate>,
    closed: Option<Arc<ClosedImage>>,
}
impl Job {
    fn current(&self, s: &ShellState) -> bool {
        self.source.current(s)
            && owned(s, &self.source.frame)
                .and_then(|v| v.preview.as_ref())
                .is_some_and(|v| Arc::ptr_eq(&v.nonce, &self.nonce))
            && self.closed.as_ref().is_none_or(|closed| {
                closed.matches(s, &self.source)
                    && owned(s, &self.source.frame)
                        .and_then(|scene| scene.closed_image.as_ref())
                        .is_some_and(|current| Arc::ptr_eq(closed, current))
            })
    }
}
pub(crate) fn display(s: &ShellState, index: usize) -> Option<Display> {
    let attempt = scene(s)?.preview.as_ref()?;
    (attempt.source.index == index && attempt.source.current(s)).then(|| attempt.display.clone())
}
/// A presentation lifetime only. Never an image, request or action capability.
/// Reopen gets a new key even when it reuses this occurrence's own PNG bytes.
pub(crate) fn dom_key(s: &ShellState, index: usize) -> Option<String> {
    let attempt = scene(s)?.preview.as_ref()?;
    (attempt.source.index == index && attempt.source.current(s)).then(|| attempt.dom_key.clone())
}
pub(crate) fn cooling(s: &ShellState) -> bool {
    s.search_file_module
        .as_ref()
        .is_none_or(|m| m.rate.cooling())
}
pub(crate) fn fixture(s: &mut ShellState) {
    s.overlay = Some(Overlay::Search);
    s.set_search_kind(SearchKind::Files);
    s.set_search_query("design notes".into());
    s.search_file_pages_fixture = Some(false);
    s.search_pager.page = 1;
    let id = MediaAssetId::new(MediaAssetKind::Attachment);
    s.media.insert(
        id.clone(),
        "image/png",
        include_bytes!("../../../../../assets/icons/icon-512.png").as_slice(),
    );
    s.search_file_image_fixture = Some(id);
}
pub(crate) fn close(mut state: Signal<ShellState>, source: Source) {
    let mut s = state.write();
    if !source.control_current(&s) {
        return;
    }
    let Some(attempt) = owned(&s, &source.frame)
        .and_then(|scene| scene.preview.as_ref())
        .filter(|attempt| attempt.source.index == source.index && attempt.source.id == source.id)
    else {
        return;
    };
    let closed = match &attempt.display {
        Display::Image(id) if s.media.is_ready(id) => Some(Arc::new(ClosedImage {
            source: attempt.source.clone(),
            id: id.clone(),
        })),
        _ => None,
    };
    if let Some(scene) = owned_mut(&mut s, &source.frame) {
        scene.closed_image = closed;
        scene.preview = None;
    }
}
fn begin(s: &mut ShellState, source: Source) -> Option<Job> {
    if !source.control_current(s) || cooling(s) {
        return None;
    }
    let scene = owned(s, &source.frame)?;
    if scene
        .preview
        .as_ref()
        .is_some_and(|v| v.source.index == source.index && matches!(v.display, Display::Loading))
    {
        return None;
    }
    let closed = scene
        .closed_image
        .as_ref()
        .filter(|closed| closed.matches(s, &source) && s.media.is_ready(&closed.id))
        .cloned();
    let job = Job {
        source: source.clone(),
        nonce: Arc::new(()),
        rate: s.search_file_module.as_ref()?.rate.clone(),
        closed,
    };
    let scene = owned_mut(s, &job.source.frame)?;
    // A different occurrence (even the same file id), retired lease or evicted
    // raster must take the original fresh-metadata route, never an id lookup.
    scene.closed_image = job.closed.clone();
    scene.preview = Some(Attempt {
        source,
        nonce: job.nonce.clone(),
        display: Display::Loading,
        dom_key: format!("image-zoom-{}", uuid::Uuid::new_v4()),
    });
    Some(job)
}
pub(crate) fn open(mut state: Signal<ShellState>, source: Source) {
    let job = begin(&mut state.write(), source);
    if let Some(job) = job {
        spawn(run(state, job));
    }
}
async fn execute(
    state: Signal<ShellState>,
    job: &Job,
    request: Request,
) -> Result<serde_json::Value, Error> {
    let current = || state.try_peek().is_ok_and(|s| job.current(&s));
    if !current() {
        return Err(Error::Offline("Image preview retired".into()));
    }
    #[cfg(test)]
    let result = test_io::get().execute(request).await;
    #[cfg(not(test))]
    let result = job
        .source
        .frame
        .parameters
        .transport()
        .execute_file_image_current(&request, current)
        .await;
    if let Ok(s) = state.try_peek() {
        job.rate.observe(&s, &result);
    }
    result
}
async fn load(state: Signal<ShellState>, job: &Job) -> Result<Option<Content>, Error> {
    let f = &job.source.frame;
    let projection = if job.closed.is_none() {
        let request = Request::metadata(
            f.parameters.client(),
            f.parameters.session(),
            &job.source.id,
        )?;
        let raw = execute(state, job, request).await?;
        if !state.try_peek().is_ok_and(|s| job.current(&s)) {
            return Ok(None);
        }
        let Ok(projection) = Projection::from_fresh(&raw, &job.source.id) else {
            return Ok(None);
        };
        Some(projection)
    } else {
        None
    };
    let request = Request::ip(
        f.parameters.client(),
        f.parameters.session(),
        &job.source.id,
    )?;
    let raw = execute(state, job, request).await?;
    if !state.try_peek().is_ok_and(|s| job.current(&s)) || !file_image::ip_allowed(&raw) {
        return Ok(None);
    }
    // The previously committed PNG is already owned by this exact occurrence.
    // The new attempt's IP success is never borrowed from the closed preview.
    if let Some(closed) = &job.closed {
        return Ok(Some(Content::Reused(closed.id.clone())));
    }
    let projection = projection.expect("fresh route owns its checked projection");
    let current = || state.try_peek().is_ok_and(|s| job.current(&s));
    #[cfg(test)]
    let bytes = test_io::get().bytes(projection.url()).await;
    #[cfg(not(test))]
    let bytes = {
        let ua = super_platinum_core::slack::xparams::Identity::from_capture().user_agent;
        tokio::select! {
            biased;
            _=async{loop{if !current(){break;}tokio::time::sleep(Duration::from_millis(100)).await;}}=>return Ok(None),
            bytes=f.parameters.transport().get_bytes(projection.url(),&ua)=>bytes,
        }
    };
    if let Err(error) = &bytes {
        if let Ok(s) = state.try_peek() {
            job.rate.observe(&s, &Err(error.clone()));
        }
    }
    let bytes = bytes?;
    if !current() {
        return Ok(None);
    }
    // Decode/re-encode is CPU/memory work, never a render/event-path operation.
    let image = tokio::task::spawn_blocking(move || projection.rasterize(&bytes))
        .await
        .ok()
        .and_then(Result::ok);
    Ok(image.map(Content::Fresh))
}
async fn run(mut state: Signal<ShellState>, job: Job) {
    let result = load(state, &job).await;
    let Ok(mut s) = state.try_write() else {
        return;
    };
    if !job.current(&s) {
        return;
    }
    let display = match result {
        Ok(Some(Content::Fresh(bytes))) => {
            let id = MediaAssetId::new(MediaAssetKind::Attachment);
            s.media.insert(id.clone(), "image/png", bytes);
            Display::Image(id)
        }
        Ok(Some(Content::Reused(id))) => Display::Image(id),
        Ok(None) => Display::Unavailable,
        Err(error) => {
            s.report_failure(&error, "Image preview needs a connection", || {
                "Could not load this preview. Your file results are retained.".into()
            });
            Display::Failed
        }
    };
    if let Some(scene) = owned_mut(&mut s, &job.source.frame) {
        if matches!(display, Display::Unavailable) {
            scene.closed_image = None;
        }
        if let Some(attempt) = &mut scene.preview {
            attempt.display = display;
        }
    }
}
