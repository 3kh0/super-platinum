//! Cached, exact text anchors. Canonical text is never taken from exports.
use crate::{
    blocks::{BlockCtx, block_nodes},
    model::RichNode,
    state::ShellState,
};
use std::{collections::BTreeMap, sync::Arc};
use super_platinum_core::canvas::edit::text::TextIntent;
#[cfg(test)]
use super_platinum_core::{canvas::CanvasDocument, slack::api::CanvasCollabSnapshot};

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
mod prepare;
pub(in crate::canvas) use prepare::Prepared;
impl Mapping {
    /// Renderer-only preview assembly. Canonical eligibility has already run on
    /// the blocking pool and remains bound to the worker's exact owner.
    pub(in crate::canvas) fn assemble(
        shell: &ShellState,
        prepared: Prepared,
        replacement: bool,
    ) -> Self {
        let Some(workspace) = shell
            .core
            .active_team
            .as_ref()
            .and_then(|id| shell.core.workspaces.get(id))
        else {
            return Self::default();
        };
        let items = prepared
            .items
            .into_iter()
            .map(|(id, item)| {
                let preview = item
                    .blocks
                    .iter()
                    .flat_map(|block| block_nodes(BlockCtx::new(workspace, &shell.media), block))
                    .collect();
                (
                    id,
                    Item {
                        intent: item.intent,
                        annotation: item.annotation,
                        structure: item.structure,
                        replacement: replacement && item.replacement,
                        blocks: serde_json::to_string(&item.blocks).unwrap(),
                        preview,
                    },
                )
            })
            .collect();
        Self { owner: None, items }
    }
    #[cfg(test)]
    pub fn project(
        shell: &ShellState,
        document: &CanvasDocument,
        snapshot: Option<&CanvasCollabSnapshot>,
    ) -> Self {
        let Some(snapshot) = snapshot else {
            return Self::default();
        };
        let replacement = super::checklist::presentation(shell).is_some_and(|view| {
            !view.loading
                && !view.stale
                && view
                    .snapshot
                    .as_ref()
                    .is_some_and(|s| s.file_id() == snapshot.file_id())
        });
        Self::assemble(shell, Prepared::project(document, snapshot), replacement)
    }
}

#[cfg(test)]
mod heading_annotations;

#[cfg(test)]
mod annotation_tests {
    use super::*;
    #[test]
    fn comment_only_mapping_keeps_blank_lines_without_granting_body_edits() {
        use super_platinum_core::canvas::wire::EncodedField as F;
        let shell = ShellState::fixture(crate::media::MediaRegistry::default());
        let mut snapshot =
            (*super::super::fixture::snapshot("<u><b>A&amp;\u{1f600}B</b></u><br><br>Last", 1000))
                .clone();
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
        assert!(seed.runs[0].style.underline);
        assert!(item.blocks.contains("underline"));
        let duplicate = CanvasDocument::parse("<div class='quip-canvas-content'><p id='paragraph'>one</p><p id='paragraph'>two</p></div>").unwrap();
        assert!(
            Mapping::project(&shell, &duplicate, Some(&snapshot))
                .items
                .is_empty()
        );
    }
}

#[cfg(test)]
mod heading_tests {
    use super::*;
    use super_platinum_core::canvas::wire::{EncodedField as F, Section};

    #[test]
    fn underline_mapping_keeps_canonical_marks_and_does_not_grant_structure() {
        let shell = ShellState::fixture(crate::media::MediaRegistry::default());
        let snapshot = super::super::fixture::snapshot("<u>A<b>bold</b></u><br>Last", 1000);
        let export = CanvasDocument::parse(
            "<div class='quip-canvas-content'><p id='paragraph'>Lagging export</p></div>",
        )
        .unwrap();
        let map = Mapping::project(&shell, &export, Some(&snapshot));
        let item = map.items.get("paragraph").unwrap();
        let seed = item.intent.as_ref().unwrap().seed();
        assert!(
            seed.runs
                .iter()
                .any(|run| run.style.underline && run.style.bold)
        );
        assert!(item.blocks.contains("underline"));
        assert!(!item.blocks.contains("Lagging export"));
        assert!(item.structure.is_none());
        assert!(item.annotation.is_none());
        assert!(!item.replacement);
    }

    #[test]
    fn heading_mapping_requires_exact_level_and_identity_without_structural_actions() {
        let shell = ShellState::fixture(crate::media::MediaRegistry::default());
        for level in 1..=3 {
            let mut snapshot =
                (*super::super::fixture::snapshot("<b>Same title</b>", 1000)).clone();
            let section = snapshot
                .sections
                .iter_mut()
                .find(|s| s.id() == "paragraph")
                .unwrap();
            *section = Section::from_raw(
                section
                    .raw()
                    .replace_fields(10, &[F::uint(10, level).unwrap()])
                    .unwrap(),
            )
            .unwrap();
            let export = CanvasDocument::parse(&format!("<div class='quip-canvas-content'><h{level} id='paragraph'><span id='paragraph'>Lagging export</span></h{level}></div>")).unwrap();
            let map = Mapping::project(&shell, &export, Some(&snapshot));
            let item = map.items.get("paragraph").unwrap();
            assert_eq!(
                item.intent.as_ref().unwrap().heading_level(),
                Some(level as u8)
            );
            assert!(item.structure.is_none());
            assert!(item.annotation.is_none());
            assert!(!item.replacement);
            assert!(item.blocks.contains("Same title"));
            assert!(!item.blocks.contains("Lagging export"));
            for html in [
                "<p id='paragraph'>Same title</p>".to_owned(),
                format!(
                    "<h{} id='paragraph'>Same title</h{}>",
                    level % 3 + 1,
                    level % 3 + 1
                ),
                format!(
                    "<h{level} id='paragraph'>Same title</h{level}><h{level} id='paragraph'>Same title</h{level}>"
                ),
                format!(
                    "<h{level} id='paragraph'><span id='paragraph'>one</span><span id='paragraph'>two</span></h{level}>"
                ),
            ] {
                let export = CanvasDocument::parse(&format!(
                    "<div class='quip-canvas-content'>{html}</div>"
                ))
                .unwrap();
                assert!(
                    Mapping::project(&shell, &export, Some(&snapshot))
                        .items
                        .is_empty(),
                    "{html}"
                );
            }
        }
    }
}
