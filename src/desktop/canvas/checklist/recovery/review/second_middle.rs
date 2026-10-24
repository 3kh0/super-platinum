//! Inert comparison of two retained splits and the intended visual paragraph order.
use dioxus::prelude::*;
use super_platinum_core::canvas::edit::structure::{
    SplitSequenceTarget, recovery::journal::LocalReview,
};

pub(super) fn comparison(review: &LocalReview) -> Element {
    let Some(split) = review.split() else {
        return rsx! {};
    };
    let target = match split.second_target() {
        Some(SplitSequenceTarget::Original) => "parent",
        Some(SplitSequenceTarget::FirstChild) => "child",
        None => return rsx! {},
    };
    let before = split.second_enter_text().unwrap_or_default();
    let left = split.second_initial_parent_text().unwrap_or_default();
    let right = split.second_initial_child_text().unwrap_or_default();
    let texts = split.member_texts();
    let ids = split.member_ids();
    let order = split.visual_order();
    let rows = review.observed_sections().collect::<Vec<_>>();
    rsx! {
        div { "data-review-split":"original",
            strong { "Original paragraph" }
            div { class:"canvas-review-preview", "{split.original_text()}" }
        }
        div { "data-review-first-halves":"true",
            strong { "Paragraphs immediately after the first Enter" }
            div { class:"canvas-review-preview", "{split.initial_parent_text()}" }
            div { class:"canvas-review-preview", "{split.initial_child_text()}" }
        }
        div { "data-review-second-middle":"true", "data-target":target,
            p { "data-second-history":"true", "Both paragraph splits and the edits between them remain part of this unresolved change." }
            div { "data-second-before":"true",
                strong { "Paragraph before the second Enter" }
                div { class:"canvas-review-preview", "{before}" }
            }
            div { "data-second-left":"true",
                strong { "First part immediately after the second Enter" }
                div { class:"canvas-review-preview", "{left}" }
            }
            div { "data-second-right":"true",
                strong { "Second part immediately after the second Enter" }
                div { class:"canvas-review-preview", "{right}" }
            }
        }
        div { "data-second-members":"true",
        strong { "Intended paragraphs" }
        for (position, index) in order.into_iter().enumerate() {
            {
                let text = texts[index];
                let kind = ["parent", "child1", "child2"][index];
                rsx! { div { "data-second-member":kind, "data-creation-index":"{index}",
                    strong { "Paragraph {position + 1}" }
                    div { class:"canvas-review-preview", "{text}" }
                    if text.is_empty() { p { "data-member-empty":"true", "Empty paragraph" } }
                } }
            }
        }
        }
        strong { "Current canvas : read-only comparison" }
        p { "This is the latest loaded content. Matching text does not establish whether the saved change succeeded." }
        for (index, id) in ids.into_iter().enumerate() {
            if !rows.iter().any(|row| row.id() == id) {
                p { "data-review-missing":["parent", "child1", "child2"][index], "An intended paragraph is absent from this loaded view." }
            }
        }
        for row in rows {
            {
                let text = super::split::section_text(row).unwrap_or_else(|| "Content preview unavailable for this section.".into());
                let deleted = row.deleted().ok().flatten() == Some(true);
                rsx! { div { "data-review-current":row.id(), "data-review-deleted":deleted,
                    strong { "Current section" }
                    if deleted { p { "Marked deleted in this loaded view." } }
                    div { class:"canvas-review-preview", "{text}" }
                } }
            }
        }
    }
}
