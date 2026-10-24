//! Inert comparison for owner deletion; observed content cannot certify delivery.
use super::*;
pub(super) fn comparison(review: &LocalReview) -> Element {
    let Some(child) = review.split_owner_child() else {
        return rsx! {};
    };
    let source = split::section_text(child).unwrap_or_else(|| "Unavailable paragraph text".into());
    let current = review.observed_sections().find(|s| s.id() == child.id());
    let current_text = current
        .and_then(split::section_text)
        .unwrap_or_else(|| "Paragraph is absent from this read".into());
    let original = review.draft_text();
    let source_deleted = child.deleted().ok().flatten() == Some(true);
    let current_deleted = current.is_some_and(|s| s.deleted().ok().flatten() == Some(true));
    let source_heading = if source_deleted {
        "Previously deleted paragraph to preserve"
    } else {
        "Paragraph to preserve"
    };
    let current_heading = if current_deleted {
        "Deleted paragraph in the current read"
    } else {
        "Preserved paragraph in the current read"
    };
    rsx! {
        div {"data-review-owner-delete":"original",h4 {"Paragraph requested for deletion"}div {class:"canvas-review-preview","{original}"}if original.is_empty(){p {"Empty paragraph"}}}
        div {"data-review-owner-delete":"child","data-review-deleted":source_deleted.to_string(),h4 {"{source_heading}"}div {class:"canvas-review-preview","{source}"}if source.is_empty(){p {"Empty paragraph"}}}
        div {"data-review-owner-delete":"current-child","data-review-deleted":current_deleted.to_string(),h4 {"{current_heading}"}div {class:"canvas-review-preview","{current_text}"}if current_text.is_empty(){p {"Empty paragraph"}}}
    }
}
