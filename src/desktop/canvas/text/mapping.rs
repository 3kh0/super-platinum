//! Cached, exact paragraph anchors. Canonical text is never taken from exports.
use crate::{
    blocks::{BlockCtx, block_nodes},
    model::RichNode,
    state::ShellState,
};
use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
};
use super_platinum_core::{
    canvas::{CanvasDocument, CanvasKind, CanvasNode, edit::text::TextIntent},
    slack::api::CanvasCollabSnapshot,
};

#[derive(Default)]
pub(in crate::canvas) struct Mapping {
    pub owner: Option<(u64, u64)>,
    pub items: BTreeMap<String, Item>,
}
pub(in crate::canvas) struct Item {
    pub intent: Option<Arc<TextIntent>>,
    pub annotation: Option<super_platinum_core::canvas::edit::annotation::AnnotationParagraph>,
    pub structure: Option<super_platinum_core::canvas::edit::structure::ParagraphActions>,
    pub replacement: bool,
    pub blocks: String,
    pub preview: Vec<RichNode>,
}
impl Mapping {
    pub fn project(
        shell: &ShellState,
        document: &CanvasDocument,
        snapshot: Option<&CanvasCollabSnapshot>,
    ) -> Self {
        let Some(snapshot) = snapshot else {
            return Self::default();
        };
        let Some(workspace) = shell
            .core
            .active_team
            .as_ref()
            .and_then(|id| shell.core.workspaces.get(id))
        else {
            return Self::default();
        };
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
                if matches!(kind, CanvasKind::Paragraph) {
                    let carriers = children.iter().filter(|child| matches!(child,
                        CanvasNode::Element { kind: CanvasKind::Inline, section: Some(child_id), .. } if child_id == id
                    )).count();
                    targets.push((id, carriers));
                }
            }
            nodes.extend(children);
        }
        let now = super::checklist::save::now_usec().unwrap_or(u64::MAX);
        let targets: Vec<_> = targets
            .into_iter()
            .filter_map(|(id, carriers)| {
                (carriers <= 1 && ids.get(id).copied() == Some(1 + carriers)).then_some(id)
            })
            .collect();
        let intents = TextIntent::capture_batch(snapshot, &targets, now);
        let actions = super_platinum_core::canvas::edit::structure::paragraph_actions(
            snapshot, &targets, now,
        );
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
        for ((id, intent), structure) in targets.into_iter().zip(intents).zip(actions) {
            let intent = intent.ok();
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
            let preview = blocks
                .iter()
                .flat_map(|block| block_nodes(BlockCtx::new(workspace, &shell.media), block))
                .collect();
            items.insert(
                id.into(),
                Item {
                    intent: intent.map(Arc::new),
                    annotation,
                    replacement: super::checklist::presentation(shell).is_some_and(|view| {
                        !view.loading
                            && !view.stale
                            && view
                                .snapshot
                                .as_ref()
                                .is_some_and(|s| s.file_id() == snapshot.file_id())
                    })
                        && super_platinum_core::canvas::edit::structure::can_replace_main_canvas(
                            snapshot, id, now,
                        ),
                    structure,
                    blocks: serde_json::to_string(&blocks).unwrap(),
                    preview,
                },
            );
        }
        Self { owner: None, items }
    }
}

#[cfg(test)]
mod annotation_tests {
    use super::*;
    #[test]
    fn comment_only_mapping_keeps_blank_lines_without_granting_body_edits() {
        use super_platinum_core::canvas::wire::EncodedField as F;
        let shell = ShellState::fixture(crate::media::MediaRegistry::default());
        let mut snapshot =
            (*super::super::fixture::snapshot("<b>A&amp;\u{1f600}B</b><br><br>Last", 1000)).clone();
        snapshot
            .file
            .extra
            .insert("access".into(), serde_json::json!("comment"));
        snapshot.thread = snapshot
            .thread
            .replace_fields(23, &[F::uint(23, 5).unwrap()])
            .unwrap();
        let export = CanvasDocument::parse(
            "<div class='quip-canvas-content'><p id='paragraph'>lagging export</p></div>",
        )
        .unwrap();
        // A text-edit fixture has no canonical placement. Annotation mapping
        // must refuse that incomplete anchor rather than infer it from export.
        assert!(
            Mapping::project(&shell, &export, Some(&snapshot))
                .items
                .is_empty()
        );
        let paragraph = snapshot
            .sections
            .iter_mut()
            .find(|section| section.id() == "paragraph")
            .unwrap();
        *paragraph = super_platinum_core::canvas::wire::Section::from_raw(
            paragraph
                .raw()
                .replace_fields(8, &[F::bytes(8, b"aac").unwrap()])
                .unwrap(),
        )
        .unwrap();
        let map = Mapping::project(&shell, &export, Some(&snapshot));
        let item = map.items.get("paragraph").unwrap();
        assert!(item.intent.is_none());
        assert!(item.structure.is_none());
        let seed = item.annotation.as_ref().unwrap();
        assert_eq!(
            seed.runs
                .iter()
                .map(|r| r.text.as_str())
                .collect::<String>(),
            "A&\u{1f600}B\n\nLast"
        );
        assert!(seed.runs[0].style.bold);
        let duplicate = CanvasDocument::parse("<div class='quip-canvas-content'><p id='paragraph'>one</p><p id='paragraph'>two</p></div>").unwrap();
        assert!(
            Mapping::project(&shell, &duplicate, Some(&snapshot))
                .items
                .is_empty()
        );
    }
}
