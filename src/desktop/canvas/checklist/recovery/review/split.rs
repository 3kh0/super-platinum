//! Read-only presentation of the complete bound split review, never edit authority.
use dioxus::prelude::*;
use super_platinum_core::canvas::{
    edit::{structure::recovery::journal::LocalReview, text::TextDraft},
    wire::Section,
};

pub(super) fn section_text(section: &Section) -> Option<String> {
    let content = section.raw().message(12).ok()??;
    // Ordinary text and document title are the two safe textual content shapes.
    let tag = match section.section_type().ok()?.unwrap_or(0) {
        0 => 1,
        64 => 58,
        _ => return None,
    };
    let text = content.message(tag).ok()??;
    let draft = TextDraft::from_rtml(text.string(1).ok()??).ok()?;
    Some(draft.runs.into_iter().map(|run| run.text).collect())
}

pub(super) fn literal_empty(section: &Section) -> bool {
    section
        .section_type()
        .is_ok_and(|value| value.unwrap_or(0) == 0)
        && section.style().is_ok_and(|value| value.unwrap_or(0) == 0)
        && section
            .raw()
            .message(12)
            .ok()
            .flatten()
            .is_some_and(|content| {
                content
                    .message(1)
                    .ok()
                    .flatten()
                    .is_some_and(|text| text.string(1).ok().flatten() == Some(""))
            })
        && section_text(section).as_deref() == Some("")
}

// Descriptions accompany the exact text, so invisible content is not presented
// as literal empty. Only the admitted plain source forms receive these labels.
fn invisible_content(rtml: &str) -> Option<(&'static str, String)> {
    if !rtml.is_empty() && rtml.chars().all(|c| c == ' ') {
        let n = rtml.len();
        Some((
            "spaces",
            if n == 1 {
                "1 space".into()
            } else {
                format!("{n} spaces")
            },
        ))
    } else if !rtml.is_empty() && rtml.chars().all(|c| c == '\u{a0}') {
        let n = rtml.chars().count();
        Some((
            "nbsp",
            if n == 1 {
                "Non-breaking space".into()
            } else {
                format!("{n} non-breaking spaces")
            },
        ))
    } else if rtml == "<br>" {
        Some(("br", "Line break".into()))
    } else {
        whitespace_content(rtml)
    }
}
// Mirror only the admitted source alphabet for a descriptive
// label. This never grants editing authority or normalizes the retained text.
fn source_whitespace(c: char) -> bool {
    matches!(
        c,
        ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200b}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202f}'
                | '\u{205f}'
                | '\u{3000}'
                | '\u{feff}'
    )
}
fn whitespace_content(rtml: &str) -> Option<(&'static str, String)> {
    if rtml.is_empty() {
        return None;
    }
    let pieces = rtml.split("<br>").collect::<Vec<_>>();
    if pieces.len() > 2
        || !pieces
            .iter()
            .all(|part| part.chars().all(source_whitespace))
    {
        return None;
    }
    let draft = TextDraft::from_rtml(rtml).ok()?;
    if draft.rtml().ok()?.as_str() != rtml {
        return None;
    }
    Some(if pieces.len() == 2 {
        ("whitespace-br", "Whitespace and line break".into())
    } else {
        ("whitespace", "Whitespace".into())
    })
}
fn current_invisible(section: &Section) -> Option<(&'static str, String)> {
    if section.section_type().ok()?.unwrap_or(0) != 0 || section.style().ok()?.unwrap_or(0) != 0 {
        return None;
    }
    let content = section.raw().message(12).ok()??;
    let text = content.message(1).ok()??;
    invisible_content(text.string(1).ok()??)
}

pub(super) fn comparison(review: &LocalReview) -> Element {
    let (parent_id, child_id, original, parent, child) = if let Some(pair) = review.split() {
        (
            pair.parent_id(),
            pair.child_id(),
            pair.original_text(),
            pair.parent_text(),
            pair.child_text(),
        )
    } else if let Some(pair) = review.end() {
        (
            pair.parent_id(),
            pair.child_id(),
            pair.original_text(),
            pair.parent_text(),
            pair.child_text(),
        )
    } else {
        return rsx! {};
    };
    let original_child = review
        .end()
        .and_then(|end| end.original_child_rtml())
        .map(|rtml| {
            let text = TextDraft::from_rtml(rtml)
                .map(|draft| {
                    draft
                        .runs
                        .into_iter()
                        .map(|run| run.text)
                        .collect::<String>()
                })
                .unwrap_or_else(|_| "Content preview unavailable for this section.".into());
            let (kind, label) = invisible_content(rtml).unwrap_or((
                "unknown",
                "Content preview unavailable for this section.".into(),
            ));
            (text, kind, label)
        });
    let second = review
        .end()
        .and_then(|end| end.second_child_id().zip(end.second_child_text()));
    let empty_enter = review.end().is_some_and(|end| end.is_empty_enter());
    let rows = review.observed_sections().collect::<Vec<_>>();
    let parent_present = rows.iter().any(|row| row.id() == parent_id);
    let child_present = rows.iter().any(|row| row.id() == child_id);
    rsx! {
        div {"data-review-split":"original",
            strong {"Original paragraph"}
            div {class:"canvas-review-preview","{original}"}
            if empty_enter {p {"data-empty-enter-label":"original","Empty paragraph"}}
        }
        if let Some((text, kind, label)) = original_child {
            div {"data-review-original-child":"true","data-content-kind":kind,
                strong {"Original following paragraph"}
                div {class:"canvas-review-preview","{text}"}
                p {"data-content-label":"true","{label}"}
            }
        }
        div {"data-review-split":"parent",
            strong {"Intended first paragraph"}
            div {class:"canvas-review-preview","{parent}"}
            if empty_enter {p {"data-empty-enter-label":"parent","Empty paragraph"}}
        }
        if let Some(pair) = review.split().filter(|pair| pair.has_child_updates()) {
            div {"data-review-initial-child":"true",
                strong {"Initial second paragraph"}
                div {class:"canvas-review-preview","{pair.initial_child_text()}"}
                p {"This was the second paragraph immediately after the split, before the later edits below."}
            }
        }
        if review.end().is_some_and(|end| end.is_empty_typing()) {
            p {"data-review-empty-typing":"true","The second paragraph was edited after Enter. Those edits remain part of this unresolved change, even if the paragraph is empty again."}
        }
        div {"data-review-split":"child",
            strong {"Intended second paragraph"}
            div {class:"canvas-review-preview","{child}"}
            if (review.is_end() || review.split().is_some_and(|pair| pair.has_child_updates())) && child.is_empty() {p {"Empty paragraph"}}
        }
        if review.end().is_some_and(|end| end.is_repeat_typing()) {
            p {"data-review-repeat-typing":"true","The third paragraph was edited after the second Enter. Those edits remain part of this unresolved change, even if the paragraph is empty again."}
        }
        if let Some((_,text))=second {
            div {"data-review-split":"secondChild",
                strong {"Intended third paragraph"}
                div {class:"canvas-review-preview","{text}"}
                if text.is_empty() {p {"Empty paragraph"}}
            }
        }
        strong {"Current canvas : read-only comparison"}
        p {"This is the latest loaded content. Matching text does not establish whether the saved change succeeded."}
        if !parent_present {p {"data-review-missing":"parent","The original paragraph is absent from this loaded view."}}
        if !child_present {p {"data-review-missing":"child","The intended second paragraph is absent from this loaded view."}}
        if let Some((id,_))=second {if !rows.iter().any(|row|row.id()==id) {p {"data-review-missing":"secondChild","The intended third paragraph is absent from this loaded view."}}}
        for row in rows {
            {
                let label = if row.id() == parent_id {"Current first paragraph"}
                    else if row.id() == child_id {"Current second paragraph"}
                    else if second.is_some_and(|(id,_)|row.id()==id) {"Current third paragraph"}
                    else if row.section_type().ok().flatten() == Some(64) {"Current title"}
                    else {"Other current section"};
                let deleted = row.deleted().ok().flatten() == Some(true);
                let text = section_text(row).unwrap_or_else(|| "Content preview unavailable for this section.".into());
                let empty_reused = !deleted && review.end().is_some_and(|end| (end.reuses_child() && end.child_id() == row.id()) || (empty_enter && end.parent_id()==row.id())) && literal_empty(row);
                let invisible = (!deleted && review.end().is_some_and(|end| end.clears_child() && end.child_id() == row.id())).then(|| current_invisible(row)).flatten();
                rsx! {div {"data-review-current":row.id(),"data-review-deleted":deleted,
                    strong {"{label}"}
                    if deleted {p {"Marked deleted in this loaded view."}}
                    div {class:"canvas-review-preview","{text}"}
                    if empty_reused {p {"Empty paragraph"}}
                    if let Some((kind, label)) = invisible {p {"data-content-kind":kind,"data-content-label":"true","{label}"}}
                }}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invisible_source_descriptions_preserve_distinct_shapes() {
        assert_eq!(
            invisible_content("   "),
            Some(("spaces", "3 spaces".into()))
        );
        assert_eq!(
            invisible_content("\u{a0}"),
            Some(("nbsp", "Non-breaking space".into()))
        );
        assert_eq!(invisible_content("<br>"), Some(("br", "Line break".into())));
        for other in ["", "\n", "<br><br>", "<b> </b>", "&nbsp;", "text"] {
            assert!(invisible_content(other).is_none());
        }
        let draft = TextDraft::from_rtml("<br>").unwrap();
        assert_eq!(
            draft
                .runs
                .iter()
                .map(|run| run.text.as_str())
                .collect::<String>(),
            "\n"
        );
    }
    #[test]
    fn whitespace_descriptions_keep_exact_mixed_and_single_break_sources() {
        let alphabet = " \u{a0}\u{1680}\u{2000}\u{2001}\u{2002}\u{2003}\u{2004}\u{2005}\u{2006}\u{2007}\u{2008}\u{2009}\u{200a}\u{200b}\u{2028}\u{2029}\u{202f}\u{205f}\u{3000}\u{feff}";
        assert_eq!(
            invisible_content(alphabet),
            Some(("whitespace", "Whitespace".into()))
        );
        assert_eq!(
            invisible_content(" \u{2028}\u{a0}\u{2029}\u{200b}"),
            Some(("whitespace", "Whitespace".into()))
        );
        for raw in [
            format!("<br>{alphabet}"),
            format!("{alphabet}<br>"),
            format!("{alphabet}<br>{alphabet}"),
        ] {
            assert_eq!(
                invisible_content(&raw),
                Some(("whitespace-br", "Whitespace and line break".into()))
            );
            assert_eq!(TextDraft::from_rtml(&raw).unwrap().rtml().unwrap(), raw);
        }
        for raw in ["\u{feff} ", " \u{feff}", "<br>\u{feff}"] {
            assert!(invisible_content(raw).is_some(), "{raw:?}");
            assert_eq!(TextDraft::from_rtml(raw).unwrap().rtml().unwrap(), raw);
        }
        for c in alphabet.chars() {
            assert!(invisible_content(&c.to_string()).is_some());
        }
        for raw in [
            " <br><br> ",
            "\u{200c}",
            "\u{200d}",
            "\t",
            "\r",
            "\n",
            "&nbsp;",
            "&#32;",
            "<BR>",
            "<br/>",
            "<br />",
            "<br class='x'>",
            "<span> </span>",
            "x",
            " \u{0085}",
        ] {
            assert!(invisible_content(raw).is_none(), "{raw:?}");
        }
    }
    #[test]
    fn plain_comparison_previews_title_and_ordinary_text_from_bound_rows() {
        let snapshot = crate::canvas::text::split_recovery_fixture::snapshot();
        assert_eq!(
            section_text(snapshot.section("title").unwrap()).as_deref(),
            Some("Fixture title")
        );
        assert_eq!(
            section_text(snapshot.section("paragraph").unwrap()).as_deref(),
            Some("Keep thisSecond paragraph")
        );
        assert_eq!(
            section_text(snapshot.section("neighbor").unwrap()).as_deref(),
            Some("Another paragraph")
        );
    }
    #[test]
    fn empty_current_label_requires_literal_plain_body() {
        use super_platinum_core::canvas::wire::{EncodedField as F, RawMessage};
        let source = crate::canvas::text::structure_fixture::snapshot();
        for (rtml, expected) in [
            ("", true),
            (" ", false),
            ("<br>", false),
            ("<b></b>", false),
        ] {
            let text = RawMessage::from_fields(&[F::bytes(1, rtml.as_bytes()).unwrap()]).unwrap();
            let content = RawMessage::from_fields(&[F::message(1, &text).unwrap()]).unwrap();
            let row = Section::from_raw(
                source
                    .section("paragraph")
                    .unwrap()
                    .raw()
                    .replace_fields(12, &[F::message(12, &content).unwrap()])
                    .unwrap(),
            )
            .unwrap();
            assert_eq!(literal_empty(&row), expected);
            let heading = Section::from_raw(
                row.raw()
                    .replace_fields(10, &[F::uint(10, 1).unwrap()])
                    .unwrap(),
            )
            .unwrap();
            assert!(!literal_empty(&heading));
        }
    }
}
