//! Pure canonical eligibility. No shell state, renderer nodes, media or signals.
use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
};
use super_platinum_core::{
    canvas::{CanvasDocument, CanvasKind, CanvasNode, edit::text::TextIntent},
    slack::api::CanvasCollabSnapshot,
};
pub(in crate::canvas) struct Prepared {
    pub(super) items: BTreeMap<String, CanonicalItem>,
}
pub(super) struct CanonicalItem {
    pub intent: Option<Arc<TextIntent>>,
    pub annotation: Option<super_platinum_core::canvas::edit::annotation::AnnotationParagraph>,
    pub structure: Option<super_platinum_core::canvas::edit::structure::ParagraphActions>,
    pub replacement: bool,
    pub blocks: Vec<serde_json::Value>,
}
impl Prepared {
    pub(in crate::canvas) fn project(
        document: &CanvasDocument,
        snapshot: &CanvasCollabSnapshot,
    ) -> Self {
        let mut ids = HashMap::<&str, usize>::new();
        let mut targets = Vec::new();
        let mut nodes: Vec<_> = document.nodes.iter().collect();
        while let Some(node) = nodes.pop() {
            let CanvasNode::Element {
                kind,
                section,
                children,
            } = node
            else {
                continue;
            };
            if let Some(id) = section.as_deref() {
                *ids.entry(id).or_default() += 1;
                let level = match kind {
                    CanvasKind::Paragraph => Some(None),
                    CanvasKind::Heading(level @ 1..=3) => Some(Some(*level)),
                    _ => None,
                };
                if let Some(level) = level {
                    let carriers = children.iter().filter(|child| matches!(child,
                        CanvasNode::Element { kind: CanvasKind::Inline, section: Some(child_id), .. } if child_id == id
                    )).count();
                    targets.push((id, carriers, level));
                }
            }
            nodes.extend(children);
        }
        let now = crate::canvas::checklist::save::now_usec().unwrap_or(u64::MAX);
        let targets: Vec<_> = targets
            .into_iter()
            .filter_map(|(id, carriers, level)| {
                (carriers <= 1
                    && ids.get(id).copied() == Some(1 + carriers)
                    && snapshot.section(id).is_some_and(|section| {
                        section
                            .style()
                            .is_ok_and(|style| style.unwrap_or(0) == u64::from(level.unwrap_or(0)))
                    }))
                .then_some((id, level))
            })
            .collect();
        let ids: Vec<_> = targets.iter().map(|(id, _)| *id).collect();
        let intents = TextIntent::capture_batch(snapshot, &ids, now);
        let actions =
            super_platinum_core::canvas::edit::structure::paragraph_actions(snapshot, &ids, now);
        let mut annotations: BTreeMap<_, _> =
            super_platinum_core::canvas::edit::annotation::AnnotationPresentation::ordinary(
                None, None,
            )
            .and_then(|context| {
                super_platinum_core::canvas::edit::annotation::range_seeds_with_presentation(
                    snapshot, &context,
                )
            })
            .unwrap_or_default()
            .into_iter()
            .collect();
        let mut items = BTreeMap::new();
        for (((id, level), intent), structure) in targets.into_iter().zip(intents).zip(actions) {
            let intent = intent.ok();
            let structure = structure.filter(|_| level.is_none());
            let annotation = annotations.remove(id);
            let seed = intent
                .as_ref()
                .map(TextIntent::seed)
                .or_else(|| structure.as_ref().map(|action| &action.seed))
                .or(annotation.as_ref().map(|a| &a.draft));
            let Some(seed) = seed else {
                continue;
            };
            let blocks = seed.rich_blocks();
            items.insert(
                id.into(),
                CanonicalItem {
                    intent: intent.map(Arc::new),
                    annotation,
                    replacement: structure.as_ref().is_some_and(|a| a.replace_main_canvas),
                    structure,
                    blocks,
                },
            );
        }
        Self { items }
    }
}
