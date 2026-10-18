//! Retained ordinary Canvas visibility. Presentation is never a write permit.
//! A presentation context is not an annotation/edit permit. Its captured values
//! must be rechecked against the same desktop owner before native acknowledgment.
use super::{Error, RawMessage, Section};
use serde::{Deserialize, Serialize};

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnnotationPresentation {
    hide_stubs: bool,
    reaction_bar: bool,
    exact_empty: bool,
    open_comment: Option<String>,
    active_reaction: Option<String>,
}

impl AnnotationPresentation {
    /// Actual retained ordinary caller: feature 173 is pushed unconditionally,
    /// reaction bar is enabled, and omitted showStubAnnotations means hide=true.
    /// Current comment/reaction identities are explicit owner-bound host state.
    pub fn ordinary(
        open_comment: Option<String>,
        active_reaction: Option<String>,
    ) -> Result<Self, Error> {
        let value = Self {
            hide_stubs: true,
            reaction_bar: true,
            exact_empty: true,
            open_comment,
            active_reaction,
        };
        value.validate()?;
        Ok(value)
    }

    pub(super) fn validate(&self) -> Result<(), Error> {
        if !self.hide_stubs || !self.reaction_bar || !self.exact_empty {
            return Err(Error::Unsupported);
        }
        if self
            .open_comment
            .iter()
            .chain(self.active_reaction.iter())
            .any(|id| id.is_empty() || id.len() > 256 || id.chars().any(char::is_control))
        {
            return Err(Error::Unsupported);
        }
        Ok(())
    }

    pub(super) fn exact_empty(&self) -> bool {
        self.exact_empty
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct Visibility {
    pub archived: bool,
    pub hidden: bool,
    pub openable: bool,
}

pub(super) fn inspect(
    control: &Section,
    metadata: Option<&RawMessage>,
    presentation: &AnnotationPresentation,
) -> Result<Visibility, Error> {
    presentation.validate()?;
    let highlight = control
        .raw()
        .message(12)?
        .ok_or(Error::Unsupported)?
        .message(7)?
        .ok_or(Error::Unsupported)?;
    if metadata.is_some_and(|m| m.string(1).ok().flatten() != Some(control.id())) {
        return Err(Error::Unsupported);
    }
    let archived = metadata
        .map(|m| m.boolean(8))
        .transpose()?
        .flatten()
        .unwrap_or(highlight.boolean(4)?.unwrap_or(false));
    let messages = metadata
        .map(|m| m.uint(2))
        .transpose()?
        .flatten()
        .unwrap_or(0);
    let reactions = metadata
        .map(|m| m.uint(6))
        .transpose()?
        .flatten()
        .unwrap_or(0);
    if messages > i32::MAX as u64 || reactions > i32::MAX as u64 {
        return Err(Error::Unsupported);
    }
    let hidden = archived
        || (presentation.hide_stubs
            && !(reactions != 0 && presentation.reaction_bar)
            && presentation.open_comment.as_deref() != Some(control.id())
            && presentation.active_reaction.as_deref() != Some(control.id())
            && messages == 0);
    Ok(Visibility {
        archived,
        hidden,
        // Retained hasOpenableCommentThread is !isArchived, not !isHidden.
        openable: !archived,
    })
}
