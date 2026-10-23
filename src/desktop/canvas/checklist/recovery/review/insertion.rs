//! Inert insertion preview and explicitly current anchor comparison.
use dioxus::prelude::*;
use super_platinum_core::canvas::edit::structure::{Side, recovery::journal::LocalReview};

pub(super) fn comparison(review: &LocalReview) -> Element {
    let Some((anchor, side)) = review.insertion_anchor() else {
        return rsx! {};
    };
    let side = match side {
        Side::Before => "before",
        Side::After => "after",
    };
    let text = review.draft_text();
    let current = review.observed_sections().find(|s| s.id() == anchor);
    let current_text = current.and_then(super::split::section_text);
    rsx! {
        div {"data-review-insertion":"true","data-anchor":anchor,"data-side":side,
            div {"data-review-insert-draft":"true",
                p {strong {"Intended new paragraph"}}
                div {class:"canvas-review-preview","{text}"}
                if text.is_empty() {p {"data-empty-paragraph":"true","Empty paragraph"}}
            }
            p {"The saved request inserts this paragraph {side} the paragraph shown below."}
            div {"data-review-insert-current":anchor,
                p {strong {"Current anchor paragraph"}}
                if let Some(text) = current_text {div {class:"canvas-review-preview","{text}"}}
                else {p {"A text preview is unavailable in the current canvas."}}
            }
            p {"This is a current comparison, not confirmation that the saved insertion succeeded."}
        }
    }
}
