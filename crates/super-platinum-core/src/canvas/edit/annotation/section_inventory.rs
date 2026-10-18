//! Complete original section-control inventory. No operation ID,
//! request, mutation or deserialized send authority is created by this module.
use super::visibility::{self, AnnotationPresentation, Visibility};
use super::{CanvasCollabSnapshot, Error, RawMessage, Section};
use std::collections::{BTreeMap, BTreeSet};

pub(super) struct SectionControls {
    anchor: Section,
    scope: (String, String, String),
    presentation: AnnotationPresentation,
    // Caller supplies the original renderer's observed registry order. It must
    // enumerate exactly the canonical set, never a filtered visible subset.
    entries: Vec<Entry>,
}
struct Entry {
    control: Section,
    metadata: Option<RawMessage>,
    visibility: Visibility,
    positioned: bool,
}

impl SectionControls {
    pub(super) fn capture(
        snapshot: &CanvasCollabSnapshot,
        anchor_id: &str,
        observed_order: &[String],
        presentation: AnnotationPresentation,
    ) -> Result<Self, Error> {
        let scope = scope(snapshot)?;
        let anchor = snapshot.section(anchor_id).ok_or(Error::Changed)?;
        if anchor.thread_id()? != Some(snapshot.thread_id())
            || anchor.document_id()? != Some(snapshot.document_id())
        {
            return Err(Error::Changed);
        }
        Self::from_parts(
            &snapshot.sections,
            &snapshot.document,
            anchor,
            scope,
            observed_order,
            presentation,
        )
    }

    pub(super) fn from_parts(
        sections: &[Section],
        document: &RawMessage,
        anchor: &Section,
        scope: (String, String, String),
        observed_order: &[String],
        presentation: AnnotationPresentation,
    ) -> Result<Self, Error> {
        presentation.validate()?;
        if document.string(1)? != anchor.document_id()?
            || document.string(6)? != anchor.thread_id()?
            || anchor.thread_id()?.is_none()
            || anchor.document_id()?.is_none()
        {
            return Err(Error::Changed);
        }
        let anchor_id = anchor.id();
        let metadata = metadata(document)?;
        let mut expected = BTreeMap::new();
        for control in sections {
            if control.deleted()? == Some(true) || control.section_type()? != Some(9) {
                continue;
            }
            let Some(content) = control.raw().message(12)? else {
                continue;
            };
            let Some(highlight) = content.message(7)? else {
                continue;
            };
            let ids = super::strings(&highlight, 3)?;
            if !ids.iter().any(|id| id == anchor_id) {
                continue;
            }
            if ids.is_empty()
                || ids.len() > 4096
                || ids.iter().collect::<BTreeSet<_>>().len() != ids.len()
                || control.id() == anchor_id
                || control.document_id()? != anchor.document_id()?
                || control.thread_id()? != anchor.thread_id()?
                || control.raw().uint(11)? != Some(1)
                || control
                    .sequence()?
                    .is_none_or(|v| v == 0 || v > i64::MAX as u64)
                || expected
                    .insert(control.id().to_owned(), (control, highlight))
                    .is_some()
            {
                return Err(Error::Unsupported);
            }
        }
        if expected.len() > 4096 || observed_order.len() != expected.len() {
            return Err(Error::Changed);
        }
        let mut entries = Vec::with_capacity(expected.len());
        let mut bytes = 0usize;
        for id in observed_order {
            let (control, highlight) = expected.remove(id).ok_or(Error::Changed)?;
            let meta = metadata.get(id);
            bytes = bytes
                .checked_add(control.raw().raw().len())
                .and_then(|v| v.checked_add(meta.map_or(0, |m| m.raw().len())))
                .filter(|v| *v <= 512 * 1024)
                .ok_or(Error::Unsupported)?;
            let visibility = visibility::inspect(control, meta, &presentation)?;
            let positioned = positioned(&highlight)?;
            entries.push(Entry {
                control: control.clone(),
                metadata: meta.cloned(),
                visibility,
                positioned,
            });
        }
        if !expected.is_empty() {
            return Err(Error::Changed);
        }
        Ok(Self {
            anchor: anchor.clone(),
            scope,
            presentation,
            entries,
        })
    }

    /// Exact retained getOrCreateHighlightForSection loop: the last visible
    /// unpositioned control wins. This is an existing-root read route only.
    pub(super) fn existing(&self) -> Option<&Section> {
        self.entries
            .iter()
            .rev()
            .find(|entry| !entry.visibility.hidden && !entry.positioned)
            .map(|entry| &entry.control)
    }

    /// A candidate clear set, not a send permit. Building ControlUpdate still
    /// requires original-node placement after full association removal. Visible
    /// positioned controls and every unknown/unrelated section stay untouched.
    pub(super) fn hidden(&self) -> impl Iterator<Item = (&Section, Option<&RawMessage>)> {
        self.entries
            .iter()
            .filter(|entry| entry.visibility.hidden)
            .map(|entry| (&entry.control, entry.metadata.as_ref()))
    }

    pub(super) fn revalidate(
        &self,
        fresh: &CanvasCollabSnapshot,
        observed_order: &[String],
        presentation: &AnnotationPresentation,
    ) -> Result<(), Error> {
        if scope(fresh)? != self.scope
            || presentation != &self.presentation
            || fresh.section(self.anchor.id()) != Some(&self.anchor)
        {
            return Err(Error::Changed);
        }
        let current = Self::capture(
            fresh,
            self.anchor.id(),
            observed_order,
            presentation.clone(),
        )?;
        if current.entries.len() != self.entries.len()
            || current.entries.iter().zip(&self.entries).any(|(now, old)| {
                now.control != old.control
                    || now.metadata != old.metadata
                    || now.visibility != old.visibility
                    || now.positioned != old.positioned
            })
        {
            return Err(Error::Changed);
        }
        Ok(())
    }
}

pub(super) fn positioned(highlight: &RawMessage) -> Result<bool, Error> {
    Ok(match highlight.message(11)? {
        None => false,
        Some(position) => {
            // hasPosition uses presence of both double fields, not their
            // numeric truthiness: an exact (0,0) position still counts.
            for tag in [1, 2] {
                let fields = position
                    .fields()
                    .iter()
                    .filter(|f| f.tag() == tag)
                    .collect::<Vec<_>>();
                if fields.len() > 1 || fields.iter().any(|field| field.wire_type() != 1) {
                    return Err(Error::Unsupported);
                }
            }
            position.has(1) && position.has(2)
        }
    })
}

fn scope(snapshot: &CanvasCollabSnapshot) -> Result<(String, String, String), Error> {
    let workspace = snapshot.init.workspace_id();
    let user = snapshot.init.options().string(33)?.ok_or(Error::Changed)?;
    let file = snapshot.file_id();
    if [workspace, user, file]
        .iter()
        .any(|value| value.is_empty() || value.len() > 256 || value.chars().any(char::is_control))
    {
        return Err(Error::Changed);
    }
    Ok((workspace.to_owned(), user.to_owned(), file.to_owned()))
}

pub(super) fn metadata(document: &RawMessage) -> Result<BTreeMap<String, RawMessage>, Error> {
    let mut values = BTreeMap::new();
    if let Some(container) = document.message(16)? {
        if container.fields().iter().any(|field| field.tag() != 1) {
            return Err(Error::Unsupported);
        }
        for raw in container.messages(1)? {
            let id = raw
                .string(1)?
                .filter(|id| !id.is_empty() && id.len() <= 256 && !id.chars().any(char::is_control))
                .ok_or(Error::Unsupported)?;
            if values.len() >= crate::canvas::wire::MAX_SECTIONS
                || values.insert(id.to_owned(), raw.clone()).is_some()
            {
                return Err(Error::Unsupported);
            }
        }
    }
    Ok(values)
}

#[cfg(test)]
mod tests {
    use super::super::F;
    use super::*;

    fn add(snapshot: &mut CanvasCollabSnapshot, id: &str, count: u64, positioned: bool) {
        let anchor = snapshot.section("item").unwrap();
        let mut highlight = vec![
            F::bytes(1, b"creator").unwrap(),
            F::bytes(3, b"item").unwrap(),
            F::bytes(3, b"not-loaded").unwrap(),
        ];
        if positioned {
            // Both doubles are exactly zero. Presence, not truthiness, selects
            // the source hasPosition branch.
            let position =
                RawMessage::decode(&[9, 0, 0, 0, 0, 0, 0, 0, 0, 17, 0, 0, 0, 0, 0, 0, 0, 0])
                    .unwrap();
            highlight.push(F::message(11, &position).unwrap());
        }
        let highlight = RawMessage::from_fields(&highlight).unwrap();
        let content = RawMessage::from_fields(&[F::message(7, &highlight).unwrap()]).unwrap();
        let control = Section::from_raw(
            RawMessage::from_fields(&[
                F::bytes(1, id.as_bytes()).unwrap(),
                F::uint(2, 7).unwrap(),
                F::bytes(6, anchor.thread_id().unwrap().unwrap().as_bytes()).unwrap(),
                F::bytes(7, anchor.document_id().unwrap().unwrap().as_bytes()).unwrap(),
                F::uint(9, 9).unwrap(),
                F::uint(11, 1).unwrap(),
                F::message(12, &content).unwrap(),
            ])
            .unwrap(),
        )
        .unwrap();
        snapshot.sections.push(control);
        let previous = snapshot
            .document
            .message(16)
            .unwrap()
            .unwrap_or(RawMessage::decode(&[]).unwrap());
        let meta = RawMessage::from_fields(&[
            F::bytes(1, id.as_bytes()).unwrap(),
            F::uint(2, count).unwrap(),
        ])
        .unwrap();
        let mut all = previous
            .messages(1)
            .unwrap()
            .iter()
            .map(|raw| F::message(1, raw).unwrap())
            .collect::<Vec<_>>();
        all.push(F::message(1, &meta).unwrap());
        snapshot.document = snapshot
            .document
            .replace_fields(
                16,
                &[F::message(16, &RawMessage::from_fields(&all).unwrap()).unwrap()],
            )
            .unwrap();
    }

    #[test]
    fn exact_complete_order_reuses_last_visible_unpositioned_control() {
        let mut snapshot = super::super::tests::fixture();
        for (id, count, positioned) in [
            ("a", 1, false),
            ("hidden", 0, false),
            ("positioned", 1, true),
            ("b", 2, false),
        ] {
            add(&mut snapshot, id, count, positioned);
        }
        let order = ["a", "hidden", "positioned", "b"]
            .map(String::from)
            .to_vec();
        let inventory = SectionControls::capture(
            &snapshot,
            "item",
            &order,
            AnnotationPresentation::ordinary(None, None).unwrap(),
        )
        .unwrap();
        assert_eq!(inventory.existing().unwrap().id(), "b");
        assert_eq!(
            inventory.hidden().map(|(s, _)| s.id()).collect::<Vec<_>>(),
            vec!["hidden"]
        );
        let reversed = order.iter().rev().cloned().collect::<Vec<_>>();
        let other = SectionControls::capture(
            &snapshot,
            "item",
            &reversed,
            AnnotationPresentation::ordinary(None, None).unwrap(),
        )
        .unwrap();
        assert_eq!(other.existing().unwrap().id(), "a");
        for malformed in [
            order[..3].to_vec(),
            vec!["a".into(); 4],
            vec![
                "a".into(),
                "hidden".into(),
                "positioned".into(),
                "foreign".into(),
            ],
        ] {
            assert!(
                SectionControls::capture(
                    &snapshot,
                    "item",
                    &malformed,
                    AnnotationPresentation::ordinary(None, None).unwrap()
                )
                .is_err()
            );
        }
    }

    #[test]
    fn current_open_stub_is_not_hidden_and_context_revalidation_is_exact() {
        let mut snapshot = super::super::tests::fixture();
        add(&mut snapshot, "stub", 0, false);
        let order = vec!["stub".into()];
        let plain = AnnotationPresentation::ordinary(None, None).unwrap();
        let open = AnnotationPresentation::ordinary(Some("stub".into()), None).unwrap();
        let inventory = SectionControls::capture(&snapshot, "item", &order, open.clone()).unwrap();
        assert_eq!(inventory.existing().unwrap().id(), "stub");
        assert_eq!(inventory.hidden().count(), 0);
        inventory.revalidate(&snapshot, &order, &open).unwrap();
        assert!(inventory.revalidate(&snapshot, &order, &plain).is_err());
        add(&mut snapshot, "late", 0, false);
        assert!(inventory.revalidate(&snapshot, &order, &open).is_err());
        assert!(
            inventory
                .revalidate(&snapshot, &["stub".into(), "late".into()], &open)
                .is_err()
        );
    }
}
