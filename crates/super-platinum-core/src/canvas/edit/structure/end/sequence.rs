//! One bounded active-last paragraph sequence; no composition of prepared edits.
use super::*;
mod acceptance;
pub(in crate::canvas::edit::structure) mod budget;
mod proof;
pub(super) use acceptance::{confirms_readback, validate_ack};
pub(in crate::canvas::edit::structure) use proof::validate;

#[derive(Clone)]
pub(in crate::canvas::edit::structure) struct Member {
    pub id: String,
    pub position: String,
    pub reused: bool,
    pub draft: TextDraft,
    pub updates: Vec<TextDraft>,
    pub wire: RawMessage,
}
/// Read-only ordered appended member. Native event attestation is renderer-owned.
pub struct EndAppend<'a> {
    id: &'a str,
    position: &'a str,
    reused: bool,
    draft: &'a TextDraft,
    updates: &'a [TextDraft],
}
impl<'a> EndAppend<'a> {
    pub fn id(&self) -> &'a str {
        self.id
    }
    pub fn position(&self) -> &'a str {
        self.position
    }
    pub fn reused(&self) -> bool {
        self.reused
    }
    pub fn draft(&self) -> &'a TextDraft {
        self.draft
    }
    pub fn updates(&self) -> &'a [TextDraft] {
        self.updates
    }
}
impl EndWitness {
    /// Each stage starts with one end Enter and then its ordered CONTENT reports.
    /// Later stages act on the preceding last row at its collapsed end, including empty.
    pub fn sequence(
        before: &TextDraft,
        offset_utf16: usize,
        reports_by_append: &[Vec<TextDraft>],
    ) -> Result<Self, Error> {
        validate_stages(reports_by_append)?;
        let mut result = Self::new(before, offset_utf16, before, &TextDraft::default())?;
        result.sequence_updates = Some(reports_by_append.to_vec());
        Ok(result)
    }
    pub fn is_sequence(&self) -> bool {
        self.sequence_updates.is_some()
    }
    pub fn append_count(&self) -> usize {
        self.sequence_updates
            .as_ref()
            .map_or(if self.repeat { 2 } else { 1 }, Vec::len)
    }
    pub fn sequence_updates(&self) -> Option<&[Vec<TextDraft>]> {
        self.sequence_updates.as_deref()
    }
}
pub(super) fn validate_witness(w: &EndWitness) -> Result<(), Error> {
    if w.repeat || w.empty_enter || !w.updates.is_empty() || !w.second_updates.is_empty() {
        return Err(Error::Unsupported);
    }
    EndWitness::new(&w.before, w.offset, &w.before, &TextDraft::default())?;
    validate_stages(w.sequence_updates().ok_or(Error::Unsupported)?)
}
pub(in crate::canvas::edit::structure) fn validate_stages(
    stages: &[Vec<TextDraft>],
) -> Result<(), Error> {
    validate_borrowed_stages(stages.iter().map(Vec::as_slice))
}
fn validate_borrowed_stages<'a>(
    stages: impl ExactSizeIterator<Item = &'a [TextDraft]> + Clone,
) -> Result<(), Error> {
    let appends = stages.len();
    let mut count = 0usize;
    for updates in stages.clone() {
        count = count.checked_add(updates.len()).ok_or(Error::Unsupported)?;
        budget::counts(appends, count, 0)?;
    }
    budget::counts(appends, count, 0)?;
    let mut bytes = 0usize;
    for updates in stages {
        typing::validate_updates(updates)?;
        for d in updates {
            bytes = bytes
                .checked_add(d.rtml()?.len())
                .ok_or(Error::Unsupported)?;
            budget::counts(appends, count, bytes)?;
        }
    }
    Ok(())
}

// All new members stay in the same original interval. Cache its successor once;
// do not rescan and allocate the full original inventory for every new row.
fn successor<'a>(intent: &'a EndIntent) -> Result<Option<&'a str>, Error> {
    let base = position::base(&intent.position)?;
    for id in &intent.inventory.ordered {
        let p = intent.inventory.sections[id]
            .raw()
            .string(8)?
            .ok_or(Error::Unsupported)?;
        if p > base && id != &intent.child {
            return Ok(Some(p));
        }
    }
    Ok(None)
}
impl EndIntent {
    pub fn is_sequence(&self) -> bool {
        self.sequence_members.is_some()
    }
    pub fn capture_sequence(
        snapshot: &CanvasCollabSnapshot,
        target: &str,
        append_count: usize,
        now: u64,
    ) -> Result<Self, Error> {
        budget::counts(append_count, 0, 0)?;
        // Reuse the admitted initialization cohort, not a prepared repeat operation.
        let mut intent = Self::capture_mode(snapshot, target, now, false, false, true)?;
        intent.second = None;
        budget::inventory(
            intent.inventory.sections.len(),
            append_count,
            intent.reused().is_some(),
        )?;
        let next_position = successor(&intent)?;
        let mut ids = std::collections::BTreeSet::from([intent.child.clone()]);
        let mut members = vec![repeat::Second {
            id: intent.child.clone(),
            position: intent.position.clone(),
        }];
        while members.len() < append_count {
            let last = members.last().ok_or(Error::Unsupported)?;
            let suffix = uuid::Uuid::new_v4().simple().to_string();
            let id = position::section_id(snapshot, &suffix[..25])?;
            if intent.inventory.sections.contains_key(&id) || !ids.insert(id.clone()) {
                return Err(Error::Changed);
            }
            members.push(repeat::Second {
                id,
                position: position::between(Some(position::base(&last.position)?), next_position)?,
            });
        }
        intent.sequence_members = Some(members);
        revalidate_plan(&intent)?;
        Ok(intent)
    }
}
pub(super) fn revalidate_plan(intent: &EndIntent) -> Result<(), Error> {
    repeat::admit(&intent.original, &intent.seed)?;
    let members = intent.sequence_members.as_ref().ok_or(Error::Unsupported)?;
    budget::inventory(
        intent.inventory.sections.len(),
        members.len(),
        intent.reused().is_some(),
    )?;
    if members[0].id != intent.child || members[0].position != intent.position {
        return Err(Error::Changed);
    }
    let mut ids = std::collections::BTreeSet::new();
    let next = successor(intent)?;
    for (i, m) in members.iter().enumerate() {
        if m.id == intent.original.id() || !ids.insert(&m.id) {
            return Err(Error::Changed);
        }
        if i > 0
            && (intent.inventory.sections.contains_key(&m.id)
                || position::between(Some(position::base(&members[i - 1].position)?), next)?
                    != m.position)
        {
            return Err(Error::Changed);
        }
    }
    Ok(())
}
impl PreparedEndEdit {
    pub fn is_sequence(&self) -> bool {
        self.intent.is_sequence()
    }
    pub fn appended_members(&self) -> impl ExactSizeIterator<Item = EndAppend<'_>> {
        let mut members = Vec::new();
        if self.is_sequence() {
            members.extend(self.sequence_rows.iter().map(|m| EndAppend {
                id: &m.id,
                position: &m.position,
                reused: m.reused,
                draft: &m.draft,
                updates: &m.updates,
            }));
        } else {
            members.push(EndAppend {
                id: self.child_section(),
                position: &self.intent.position,
                reused: self.reuses_child(),
                draft: self.child_draft(),
                updates: self.child_updates(),
            });
            if let (Some(second), Some(draft)) = (&self.intent.second, self.second_child_draft()) {
                members.push(EndAppend {
                    id: &second.id,
                    position: &second.position,
                    reused: false,
                    draft,
                    updates: self.second_child_updates(),
                });
            }
        }
        members.into_iter()
    }
}
impl AcceptedEnd {
    pub fn is_sequence(&self) -> bool {
        self.intent.is_sequence()
    }
    pub fn appended_sections(&self) -> impl ExactSizeIterator<Item = &Section> {
        if self.is_sequence() {
            self.sequence_sections.iter().collect::<Vec<_>>()
        } else {
            std::iter::once(&self.child)
                .chain(self.second.iter())
                .collect::<Vec<_>>()
        }
        .into_iter()
    }
}
pub(super) fn build(
    intent: Arc<EndIntent>,
    witness: EndWitness,
    now: u64,
) -> Result<PreparedEndEdit, Error> {
    validate_witness(&witness)?;
    let seed = match intent.original.crdt()? {
        Some(w) => w,
        None => text::crdt::bootstrap(&intent.original, &intent.seed, &intent.session, now)?,
    };
    let parent_wire = validation::project_parent_session(&seed)?;
    let captured_bytes = intent
        .inventory
        .sections
        .values()
        .try_fold(0usize, |n, s| {
            n.checked_add(s.raw().raw().len()).ok_or(Error::Unsupported)
        })?;
    let mut packet = budget::Packet::new(captured_bytes)?;
    packet.push(F::message(
        1,
        &prepared::metadata(&intent.original, &intent.session, &parent_wire)?,
    )?)?;
    let title_field = F::message(14, intent.title.rich_text())?;
    let mut expiry = seed.uint(4)?;
    let mut members = Vec::new();
    for (i, (planned, reports)) in intent
        .sequence_members
        .as_ref()
        .ok_or(Error::Unsupported)?
        .iter()
        .zip(witness.sequence_updates().ok_or(Error::Unsupported)?)
        .enumerate()
    {
        let reused = if i == 0 { intent.reused() } else { None };
        let seed = if let Some(source) = reused {
            let seed = match source.crdt()? {
                Some(w) => w,
                None => text::crdt::bootstrap(source, &TextDraft::default(), &intent.session, now)?,
            };
            expiry = [expiry, seed.uint(4)?].into_iter().flatten().min();
            validation::project_parent_session(&seed)?
        } else {
            crdt::insert(&planned.id, &TextDraft::default(), &intent.session, now)?
        };
        let wire = if reports.is_empty() {
            seed
        } else {
            typing::build(&seed, reused.is_some(), reports, &intent.session, now)?
        };
        let member = Member {
            id: planned.id.clone(),
            position: planned.position.clone(),
            reused: reused.is_some(),
            draft: reports.last().cloned().unwrap_or_default(),
            updates: reports.clone(),
            wire,
        };
        packet.push(member_field(&member, intent.reused(), &intent.session)?)?;
        members.push(member);
    }
    packet.push(title_field)?;
    let data = packet.finish()?;
    validate(
        &intent.original,
        intent.reused(),
        &intent.session,
        &intent.seed,
        &members,
        &data,
    )?;
    let first = members.first().ok_or(Error::Unsupported)?;
    Ok(PreparedEndEdit {
        child_wire: first.wire.clone(),
        child_draft: first.draft.clone(),
        second_wire: None,
        second_draft: None,
        sequence_rows: members,
        intent,
        witness,
        data,
        parent_wire,
        expiry,
    })
}
pub(super) fn project(
    original: &Section,
    reused: Option<&Section>,
    session: &str,
    parent_wire: &RawMessage,
    members: &[Member],
    title: &RawMessage,
) -> Result<RawMessage, Error> {
    let mut packet = budget::Packet::new(0)?;
    packet.push(F::message(
        1,
        &prepared::metadata(original, session, parent_wire)?,
    )?)?;
    for m in members {
        packet.push(member_field(m, reused, session)?)?;
    }
    packet.push(F::message(14, title)?)?;
    packet.finish()
}
fn member_field(m: &Member, reused: Option<&Section>, session: &str) -> Result<F, Error> {
    let row = if m.reused {
        let source = reused
            .filter(|s| s.id() == m.id)
            .ok_or(Error::Unsupported)?;
        let mut row = prepared::metadata(source, session, &m.wire)?;
        if !m.updates.is_empty() {
            row = row
                .replace_fields(9, &[F::uint(9, 0)?])?
                .replace_fields(12, &[F::message(12, &text::content(source, &m.draft)?)?])?;
        }
        row
    } else {
        prepared::new_row(&m.id, &m.position, session, &m.wire, &m.draft)?
    };
    Ok(F::message(1, &row)?)
}
