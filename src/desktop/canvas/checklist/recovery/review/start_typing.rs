//! Saved start insertion and original edit, with explicitly current comparison.
use dioxus::prelude::*;
use super_platinum_core::canvas::edit::structure::recovery::journal::LocalReview;
pub(super) fn comparison(review: &LocalReview) -> Element {
    let Some(saved) = review.start_typing() else {
        return rsx! {};
    };
    let before = saved.original_text();
    let after = saved.parent_text();
    let blank = saved.child_text();
    let current = review
        .observed_sections()
        .find(|s| s.id() == saved.original_id())
        .and_then(super::split::section_text);
    rsx! {
        div {"data-review-start-typing":"true",
            div {"data-review-start-before":"true",p {strong {"Original paragraph before the edit"}} div {class:"canvas-review-preview","{before}"}}
            div {"data-review-start-after":"true",p {strong {"Intended original paragraph"}} div {class:"canvas-review-preview","{after}"} if after.is_empty(){p {"Empty paragraph"}}}
            div {"data-review-start-new":"true",p {strong {"Intended new paragraph"}} div {class:"canvas-review-preview","{blank}"} p {"data-empty-paragraph":"true","Empty paragraph"}}
            p {"The new empty paragraph belongs before the original paragraph."}
            div {"data-review-start-current":saved.original_id(),p {strong {"Current original paragraph"}}
                if let Some(text)=current {div {class:"canvas-review-preview","{text}"}}
                else {p {"A text preview is unavailable in the current canvas."}}
            }
            p {"This is a current comparison, not confirmation that the saved changes succeeded."}
        }
    }
}
