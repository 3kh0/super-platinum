//! Original-node image export custody before the body is replaced by Comments.
//! Native evidence has no send authority; create still re-reads and journals.
use super::*;
use crate::{canvas::checklist, media::MediaUri};
use serde::Deserialize;
use serde_json::json;
use super_platinum_core::{
    MediaAssetId,
    canvas::edit::annotation::{
        AnnotationPresentation, ImageExport, SectionAnnotationChoice, image_seed,
    },
};
const SCRIPT: &str = include_str!("image.js");
#[derive(Clone, Copy)]
pub(in crate::canvas) struct Editors {
    pub rename: Signal<Option<crate::canvas::RenameDraft>>,
    pub text: Signal<Option<crate::canvas::text::Draft>>,
    pub review: Signal<Option<checklist::recovery::review::Draft>>,
}
impl Editors {
    fn idle(&self) -> bool {
        self.rename.peek().is_none() && self.text.peek().is_none() && self.review.peek().is_none()
    }
}
#[derive(Clone)]
pub(super) struct Selected {
    pub section: String,
    pub label: String,
    pub export: ImageExport,
}
#[derive(Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
struct Capture {
    token: String,
    section: String,
    visit: String,
    read: String,
    media: String,
    outcome: ImageExport,
}
#[derive(Clone)]
struct Binding {
    snapshot: Arc<CanvasCollabSnapshot>,
    section: String,
    label: String,
    visit: u64,
    read: u64,
    epoch: u64,
    media: Option<MediaAssetId>,
    uri: String,
    available: &'static str,
}
fn availability(state: &ShellState, id: Option<&MediaAssetId>) -> &'static str {
    match id {
        None => "unavailable",
        Some(id) if state.media.image_state(id).1 => "unavailable",
        Some(id) if state.media.is_ready(id) => "ready",
        Some(_) => "pending",
    }
}
impl Binding {
    fn current(&self, shell: &ShellState, reader: &crate::canvas::Reader) -> bool {
        shell.media_epoch == self.epoch
            && !reader.loading
            && availability(shell, self.media.as_ref()) == self.available
            && checklist::presentation(shell).is_some_and(|view| {
                view.visit == self.visit
                    && view.read == self.read
                    && !view.loading
                    && !view.stale
                    && view.operation.is_none()
                    && view.capacity_reason.is_none()
                    && view
                        .snapshot
                        .as_ref()
                        .is_some_and(|s| Arc::ptr_eq(s, &self.snapshot))
            })
    }
}

pub(in crate::canvas) fn render(
    state: Signal<ShellState>,
    reader: Signal<crate::canvas::Reader>,
    model: Signal<super::super::State>,
    opened: Signal<bool>,
    canonical: Option<&checklist::Presentation>,
    section: Option<&str>,
    url: Option<&str>,
    alt: &str,
    epoch: u64,
    editors: Editors,
) -> Element {
    let media = url.map(|url| {
        state.peek().media.register_image(
            super_platinum_core::MediaAssetKind::Attachment,
            url,
            "image/png",
        )
    });
    let image = match media.clone() {
        Some(id) => rsx! {crate::view::media_image::MediaImage {
            state,id,media_epoch:epoch,alt:alt.to_owned(),class:"canvas-image",loading:"lazy"
        }},
        None => rsx! {span {class:"canvas-image-unavailable","{alt}"}},
    };
    let binding = (|| {
        let view = canonical.filter(|v| {
            !v.loading && !v.stale && v.operation.is_none() && v.capacity_reason.is_none()
        })?;
        let snapshot = view.snapshot.clone()?;
        let section = section?;
        let seed = image_seed(&snapshot, section).ok()?;
        if match url {
            Some(url) => !seed.matches_rendered_url(url),
            None => seed.has_image_source(),
        } {
            return None;
        }
        let available = availability(&state.peek(), media.as_ref());
        let uri = media
            .as_ref()
            .map(|id| id.webview_uri_at(epoch))
            .unwrap_or_default();
        Some(Binding {
            snapshot,
            section: section.into(),
            label: seed.alt().into(),
            visit: view.visit,
            read: view.read,
            epoch,
            media,
            uri,
            available,
        })
    })();
    let Some(binding) = binding else {
        return image;
    };
    let disabled = !editors.idle()
        || *opened.read()
        || reader.read().loading
        || binding.available == "pending";
    rsx! {div {class:"canvas-image-comment",
        "data-canvas-image-section":binding.section.clone(),
        "data-canvas-image-visit":binding.visit.to_string(),
        "data-canvas-image-read":binding.read.to_string(),
        "data-canvas-image-media":binding.uri.clone(),
        "data-canvas-image-availability":binding.available,
        div {"data-canvas-image-body":"true",{image}}
        button {class:"canvas-more",disabled,onclick:move |_|choose(state,reader,model,opened,binding.clone(),editors),"Comment on image"}
    }}
}
fn choose(
    mut state: Signal<ShellState>,
    reader: Signal<crate::canvas::Reader>,
    model: Signal<super::super::State>,
    mut opened: Signal<bool>,
    binding: Binding,
    editors: Editors,
) {
    if !editors.idle()
        || *opened.peek()
        || binding.available == "pending"
        || !binding.current(&state.peek(), &reader.peek())
    {
        return;
    }
    let Some(owner) = Owner::capture(&state.peek()) else {
        return;
    };
    let token = uuid::Uuid::new_v4().to_string();
    let options = json!({"token":token,"section":binding.section,
        "visit":binding.visit.to_string(),"read":binding.read.to_string(),"media":binding.uri});
    dioxus::core::spawn_forever(async move {
        let mut retained: Option<Ticket> = None;
        let result: Result<Option<String>, ()> =
            tokio::time::timeout(std::time::Duration::from_secs(3), async {
                let mut eval = dioxus::document::eval(&format!(
                    "{SCRIPT}\nreturn await captureCanvasAnnotationImage({options},dioxus);"
                ));
                let captured = eval.recv::<Capture>().await.map_err(|_| ())?;
                if captured.token != token
                    || captured.section != binding.section
                    || captured.visit != binding.visit.to_string()
                    || captured.read != binding.read.to_string()
                    || captured.media != binding.uri
                    || (binding.available == "unavailable")
                        != (captured.outcome == ImageExport::NoImage)
                    || !editors.idle()
                    || *opened.peek()
                    || !owner.current(&state.peek())
                    || !binding.current(&state.peek(), &reader.peek())
                {
                    return Err(());
                }
                let snapshot = binding.snapshot.clone();
                let section = binding.section.clone();
                let export = captured.outcome;
                let choice = tokio::task::spawn_blocking(move || {
                    let presentation = AnnotationPresentation::ordinary(None, None)?;
                    AnnotationIntent::for_image(&snapshot, &section, &presentation, export)
                })
                .await
                .map_err(|_| ())?
                .map_err(|_| ())?;
                if !editors.idle()
                    || *opened.peek()
                    || !owner.current(&state.peek())
                    || !binding.current(&state.peek(), &reader.peek())
                {
                    return Err(());
                }
                let existing = match choice {
                    SectionAnnotationChoice::Existing(control) => Some(control),
                    SectionAnnotationChoice::New(_) => {
                        let ticket = bind(state).ok_or(())?;
                        {
                            let shell = state.peek();
                            let e = entry(&shell, &ticket.id).ok_or(())?;
                            if e.pending.is_some()
                                || e.acknowledged.is_some()
                                || e.verified.is_some()
                                || e.accepted.is_some()
                            {
                                return Err(());
                            }
                        }
                        if !start(state, &ticket) {
                            return Err(());
                        }
                        retained = Some(ticket.clone());
                        let mut shell = state.write();
                        if !ticket.current(&shell) {
                            return Err(());
                        }
                        let e = entry_mut(&mut shell, &ticket.id).ok_or(())?;
                        e.snapshot = Some(binding.snapshot.clone());
                        e.selected = binding.section.clone();
                        e.selected_range = None;
                        e.selected_image = Some(Selected {
                            section: binding.section.clone(),
                            label: binding.label.clone(),
                            export,
                        });
                        None
                    }
                };
                eval.send(json!({"token":token,"accepted":true}))
                    .map_err(|_| ())?;
                if eval.join::<Capture>().await.map_err(|_| ())? != captured {
                    return Err(());
                }
                Ok(existing)
            })
            .await
            .unwrap_or(Err(()));
        let current = editors.idle()
            && !*opened.peek()
            && owner.current(&state.peek())
            && binding.current(&state.peek(), &reader.peek())
            && retained.as_ref().is_none_or(|t| t.current(&state.peek()));
        if result.is_err() || !current {
            if let Some(t) = &retained {
                if let Some(e) = entry_mut(&mut state.write(), &t.id) {
                    e.selected_image = None;
                    e.selected.clear();
                }
                issue(
                    state,
                    t,
                    "The original image changed or could not be captured. Nothing was sent.",
                );
            }
            if current {
                state
                    .write()
                    .show_toast("The original image could not be captured. Nothing was sent.");
            }
            return;
        }
        if let Some(t) = &retained {
            if let Some(e) = entry_mut(&mut state.write(), &t.id) {
                e.busy = false;
            }
        }
        opened.set(true);
        if let Ok(Some(control)) = result {
            super::super::selection::open(state, model, owner, control);
        } else {
            super::super::open(state, model);
        }
    });
}
