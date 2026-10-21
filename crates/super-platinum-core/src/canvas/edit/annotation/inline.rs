//! Inline range preparation. The range/tree primitives are kept separate from
//! ordinary TextDraft admission, which still refuses annotation markup.
use super::super::structure::position::modern_id;
use super::super::text::{TextDraft, TextRun};
use super::Error;
use super::{CanvasCollabSnapshot, F, PreparedAnnotation, RawMessage};
use serde::{Deserialize, Serialize};

mod archived;
pub(super) use archived::Removal;
mod compact;
mod existing;
mod preserved;
pub(super) use existing::Existing;
pub(super) use preserved::{Inspection, Observation as PriorObservation, Preserved};
mod history;
mod whole;
pub(super) use whole::Whole;
mod range;
mod record;
pub(super) use record::{parent as check_record_parent, payload as checked_record_payload};

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Selection {
    pub start: usize,
    pub end: usize,
}

pub(super) struct Prepared {
    pub expires: Option<u64>,
}

pub(super) fn capture(
    original: &Existing,
    selection: Selection,
    control: &str,
    removal: Option<&Removal>,
) -> Result<(), Error> {
    range::capture_removing(original, selection.start, selection.end, control, removal).map(|_| ())
}
pub(super) fn original(section: &super::Section) -> Result<Existing, Error> {
    existing::parse(section)
}

pub(super) fn prepare(
    prepared: &mut PreparedAnnotation,
    fresh: &CanvasCollabSnapshot,
    selection: Selection,
    now: u64,
) -> Result<Prepared, Error> {
    let original = existing::parse(&prepared.intent.section)?;
    let model = range::capture_removing(
        &original,
        selection.start,
        selection.end,
        prepared.control_id(),
        prepared.intent.removal.as_ref(),
    )?;
    let history = history::prepare(
        &prepared.intent.section,
        &original,
        &model,
        prepared.session_id(),
        now,
    )?;
    let control = prepared.data.message(1)?.ok_or(Error::Unsupported)?;
    let highlight = control
        .message(12)?
        .ok_or(Error::Unsupported)?
        .message(7)?
        .ok_or(Error::Unsupported)?
        .remove(3)?;
    let control = control.replace_fields(
        12,
        &[F::message(
            12,
            &RawMessage::from_fields(&[F::message(7, &highlight)?])?,
        )?],
    )?;
    let content = content(&prepared.intent.section, &history.rtml)?;
    let mut fields = vec![
        F::bytes(1, prepared.anchor_section().as_bytes())?,
        F::uint(
            2,
            prepared
                .intent
                .section
                .sequence()?
                .ok_or(Error::Unsupported)?,
        )?,
        F::uint(9, 0)?,
        F::message(12, &content)?,
        F::message(13, &RawMessage::decode(&[])?)?,
    ];
    if let Some(attributes) =
        super::super::text::heading_content_attributes(&prepared.intent.section)?
    {
        fields.push(F::message(16, &attributes)?);
    }
    fields.extend([
        F::uint(19, 1)?,
        F::uint(20, 1)?,
        F::bytes(33, format!("{}-0", prepared.session_id()).as_bytes())?,
        F::message(38, &history.wire)?,
    ]);
    let parent = RawMessage::from_fields(&fields)?;
    prepared.intent.snippet = super::snippet::capture_inline(
        &prepared.intent.section,
        prepared.control_id(),
        fresh.init.quip_user_id(),
        prepared.user_id(),
        &history.rtml,
    )?;
    prepared.data = prepared
        .data
        .replace_fields(1, &[F::message(1, &control)?, F::message(1, &parent)?])?;
    Ok(Prepared {
        expires: history.expires,
    })
}

fn content(section: &super::Section, rtml: &str) -> Result<RawMessage, Error> {
    let content = section.raw().message(12)?.ok_or(Error::Unsupported)?;
    let text = content.message(1)?.ok_or(Error::Unsupported)?;
    Ok(content.replace_fields(
        1,
        &[F::message(
            1,
            &text.replace_fields(1, &[F::bytes(1, rtml.as_bytes())?])?,
        )?],
    )?)
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod workflow_tests;

pub(super) fn captured_rtml(
    original: &Existing,
    selection: Selection,
    control: &str,
    removal: Option<&Removal>,
) -> Result<String, Error> {
    range::capture_removing(original, selection.start, selection.end, control, removal)
        .map(|m| m.rtml)
}

#[cfg(test)]
mod underline_tests;
