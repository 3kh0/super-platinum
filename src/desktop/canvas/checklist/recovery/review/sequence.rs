//! Inert ordered sequence review: original, intended members, and CURRENT comparison.
use super::*;
use super_platinum_core::canvas::edit::text::TextDraft;
fn visible(rtml: &str) -> String {
    TextDraft::from_rtml(rtml)
        .map(|d| d.runs.into_iter().map(|r| r.text).collect())
        .unwrap_or_else(|_| "Content preview unavailable for this section.".into())
}
pub(super) fn comparison(review: &LocalReview) -> Element {
    let Some(end) = review.end().filter(|e| e.is_sequence()) else {
        return rsx! {};
    };
    let members = end.appended_members().collect::<Vec<_>>();
    let rows = review.observed_sections().collect::<Vec<_>>();
    rsx! {
        div {"data-review-sequence":"original",strong{"Original paragraph"}div{class:"canvas-review-preview","{end.original_text()}"}}
        div {"data-review-sequence":"parent","data-parent-reports":end.parent_report_count(),strong{"Intended original paragraph"}div{class:"canvas-review-preview","{end.parent_text()}"}if end.parent_text().is_empty(){p{"data-parent-empty":"true","Empty paragraph"}}}
        if end.parent_report_count()>0 || members.iter().any(|m|m.report_count()>0){p{"data-review-sequence-history":"true","Edits remain part of this unresolved change, including paragraphs that became empty again."}}
        for (index,member) in members.iter().enumerate(){
            {let text=visible(member.intended_rtml());let label=format!("Intended appended paragraph {}",index+1);rsx!{
                div {"data-review-sequence":"member","data-member-id":member.id(),"data-member-index":index,"data-member-reused":member.reused(),"data-member-reports":member.report_count(),
                    strong{"{label}"}
                    if let Some(source)=member.original_rtml(){div{"data-member-source":"true",strong{"Original following paragraph"}div{class:"canvas-review-source","{visible(source)}"}if source.is_empty(){p{"Empty paragraph"}}}}
                    div{class:"canvas-review-preview","{text}"}
                    if text.is_empty(){p{"data-member-empty":"true","Empty paragraph"}}
                }
            }}
        }
        strong{"Current canvas : read-only comparison"}
        p{"This is the latest loaded content. Matching text does not establish whether the saved change succeeded."}
        if !rows.iter().any(|r|r.id()==end.parent_id()){p{"data-review-missing":"parent","The original paragraph is absent from this loaded view."}}
        for member in &members {if !rows.iter().any(|r|r.id()==member.id()){p{"data-review-missing":member.id(),"An intended appended paragraph is absent from this loaded view."}}}
        for row in rows {{let text=super::split::section_text(row).unwrap_or_else(||"Content preview unavailable for this section.".into());let deleted=row.deleted().ok().flatten()==Some(true);let empty=!deleted&&members.iter().any(|m|m.id()==row.id())&&super::split::literal_empty(row);rsx!{
            div{"data-review-current":row.id(),"data-review-deleted":deleted,
                strong{if row.id()==end.parent_id(){"Current original paragraph"}else if members.iter().any(|m|m.id()==row.id()){"Current appended paragraph"}else if row.section_type().ok().flatten()==Some(64){"Current title"}else{"Other current section"}}
                if deleted{p{"Marked deleted in this loaded view."}}div{class:"canvas-review-preview","{text}"}if empty{p{"Empty paragraph"}}
            }
        }}}
    }
}
