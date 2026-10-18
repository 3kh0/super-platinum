//! Read-only root File/Canvas title inputs from one canonical collaboration read.
//! Export binding, account/visit custody, acquisition and navigation belong to
//! the host. Nothing here admits an edit or resolves an HTML anchor by its URL.
use super::wire::{self, RawMessage, Section};
use crate::slack::api::CanvasCollabSnapshot;
use std::sync::Arc;

mod context;
mod data;
mod object;
mod presentation;
mod rich;
#[cfg(test)]
mod tests;
pub use context::MountedContext;
pub use data::MountedData;
pub use object::{Kind, Reference};
pub use presentation::{Anchor, Icon, Label, Presentation, project};
pub use rich::{Chunk, RichPlan};

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("unsupported Canvas title source")]
    Unsupported,
    #[error("Canvas title source changed")]
    Changed,
    #[error("invalid Canvas title protobuf")]
    Wire(#[from] wire::Error),
}

/// A title read witness, deliberately not serializable or an edit capability.
/// Missing cached data is distinct from a private/deleted cached object.
pub struct Captured {
    snapshot: Arc<CanvasCollabSnapshot>,
    section: Section,
    payload: RawMessage,
    reference: Option<Reference>,
    data: Option<MountedData>,
    context: MountedContext,
}

pub fn capture(snapshot: Arc<CanvasCollabSnapshot>, id: &str) -> Result<Captured, Error> {
    let mut matches = snapshot.sections.iter().filter(|s| s.id() == id);
    let section = matches.next().ok_or(Error::Changed)?.clone();
    if matches.next().is_some() || section.deleted()? == Some(true) {
        return Err(Error::Unsupported);
    }
    let payload = snapshot.load.payload()?.ok_or(Error::Unsupported)?;
    let reference = Reference::card(&section)?;
    let data = reference
        .as_ref()
        .map(|r| r.data(&payload))
        .transpose()?
        .flatten();
    let context = MountedContext::capture(&snapshot)?;
    Ok(Captured {
        snapshot,
        section,
        payload,
        reference,
        data,
        context,
    })
}

impl Captured {
    pub fn section(&self) -> &Section {
        &self.section
    }
    pub fn reference(&self) -> Option<&Reference> {
        self.reference.as_ref()
    }
    pub fn data(&self) -> Option<&MountedData> {
        self.data.as_ref()
    }
    pub fn context(&self) -> &MountedContext {
        &self.context
    }
    pub fn presentation(&self, model_loading: bool) -> Presentation {
        self.data
            .as_ref()
            .map_or(Presentation::Placeholder, |data| {
                project(data, model_loading)
            })
    }
    pub fn rich_plan(&self) -> Result<Option<RichPlan>, Error> {
        match &self.data {
            Some(MountedData::Document {
                rich_text_name: Some(raw),
                ..
            }) => RichPlan::parse(raw).map(Some),
            _ => Ok(None),
        }
    }
    /// Only presentation text is supplied by the renderer (not canonical data).
    /// The plan must still exactly match this capture. Dates use the renderer's
    /// source-compatible Intl/local-time projection; emoji DOM follows later.
    pub fn finish_rich(
        &self,
        plan: &RichPlan,
        parts: &[String],
        model_loading: bool,
    ) -> Result<Presentation, Error> {
        if self.rich_plan()?.as_ref() != Some(plan) || parts.len() != plan.chunks().len() {
            return Err(Error::Changed);
        }
        presentation::finish_rich(
            self.data.as_ref().ok_or(Error::Changed)?,
            &rich::truncate(&parts.concat()),
            model_loading,
        )
    }
    /// Full original read/context equality is intentionally conservative. Host
    /// account/session/transport/visit checks remain necessary in addition.
    pub fn current(&self, snapshot: &CanvasCollabSnapshot) -> Result<(), Error> {
        if snapshot.file_id() != self.snapshot.file_id()
            || snapshot.thread_id() != self.snapshot.thread_id()
            || snapshot.document_id() != self.snapshot.document_id()
            || snapshot.init.options() != self.snapshot.init.options()
            || snapshot.init.initializer() != self.snapshot.init.initializer()
            || snapshot.init.quip_user_id() != self.snapshot.init.quip_user_id()
            || snapshot.init.workspace_id() != self.snapshot.init.workspace_id()
            || snapshot.thread != self.snapshot.thread
            || snapshot.document != self.snapshot.document
            || snapshot.sections != self.snapshot.sections
            || snapshot.load.payload()?.as_ref() != Some(&self.payload)
        {
            return Err(Error::Changed);
        }
        Ok(())
    }
}
