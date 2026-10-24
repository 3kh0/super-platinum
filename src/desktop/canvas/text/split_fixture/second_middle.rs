//! Fixed two-interior-Enter fixture, with creation identities kept separate from DOM order.
use super::*;
use super_platinum_core::canvas::edit::{
    structure::{SplitSequenceTarget, SplitWitness},
    text::TextDraft,
};

pub(in crate::canvas::text) fn selector() -> &'static str {
    match std::env::var("SUPER_PLATINUM_CANVAS_SECOND_MIDDLE").as_deref() {
        Ok("parent") => "parent",
        Ok("child") => "child",
        _ => "none",
    }
}
pub(in crate::canvas::text) fn cadence() -> &'static str {
    if std::env::var("SUPER_PLATINUM_CANVAS_SECOND_MIDDLE_CADENCE").as_deref() == Ok("edited") {
        "edited"
    } else {
        "straight"
    }
}
pub(in crate::canvas::text) fn enabled() -> bool {
    selector() != "none" && mode() == "middle"
}
pub(in crate::canvas::text) fn valid() -> bool {
    let target = std::env::var("SUPER_PLATINUM_CANVAS_SECOND_MIDDLE");
    let order = std::env::var("SUPER_PLATINUM_CANVAS_SECOND_MIDDLE_CADENCE");
    let target_ok = match target {
        Ok(v) => v == "none" || (mode() == "middle" && matches!(v.as_str(), "parent" | "child")),
        Err(std::env::VarError::NotPresent) => true,
        _ => false,
    };
    let order_ok = match order {
        Ok(v) => v == "straight" || (enabled() && v == "edited"),
        Err(std::env::VarError::NotPresent) => true,
        _ => false,
    };
    target_ok
        && order_ok
        && (!enabled()
            || (typing() == "none"
                && !pre_enter::enabled()
                && !parent_edit::enabled()
                && parent_edit::cadence() == "parent-first"))
}
pub(in crate::canvas::text) fn visual_order() -> [usize; 3] {
    if selector() == "parent" {
        [0, 2, 1]
    } else {
        [0, 1, 2]
    }
}
pub(in crate::canvas::text) fn second_left() -> &'static str {
    if selector() == "child" {
        "Second "
    } else if corpus().name == "unicode" {
        "Caf\u{e9} & "
    } else {
        "Keep "
    }
}
pub(in crate::canvas::text) fn final_text(index: usize) -> &'static str {
    let unicode = corpus().name == "unicode";
    match (index, selector(), cadence(), unicode) {
        (0, _, "edited", _) => "",
        (0, "parent", _, _) => second_left(),
        (0, _, _, _) => corpus().left,
        (1, "child", _, _) => "Second ",
        (1, "parent", "edited", false) => "Second paragraph changed",
        (1, "parent", "edited", true) => "Second \u{65e5}\u{672c}\u{1f600} & paragraph changed",
        (1, _, _, _) => corpus().right,
        (2, "parent", "straight", false) => "this",
        (2, "parent", "straight", true) => "\u{65e5}\u{672c}\u{1f600}",
        (2, "parent", "edited", false) => "this revised revised",
        (2, "parent", "edited", true) => "\u{65e5}\u{672c}\u{1f600} revised revised",
        (2, "child", "straight", false) => "paragraph",
        (2, "child", "straight", true) => "\u{65e5}\u{672c}\u{1f600} & paragraph",
        (2, "child", "edited", false) => "paragraph revised revised",
        (2, "child", "edited", true) => "\u{65e5}\u{672c}\u{1f600} & paragraph revised revised",
        _ => unreachable!("fixed three-member fixture"),
    }
}
pub(in crate::canvas::text) fn edited(index: usize) -> String {
    if index == 0 {
        corpus().edited.into()
    } else {
        format!("{} edited", final_text(index))
    }
}
pub(in crate::canvas::text) fn review() -> Value {
    let w = witness(&TextDraft::from_rtml(corpus().seed_rtml).unwrap());
    let plain = |d: &TextDraft| d.runs.iter().map(|r| r.text.as_str()).collect::<String>();
    let kinds = ["parent", "child1", "child2"];
    json!({"target":selector(),"before":plain(w.second_enter_draft().unwrap()),"left":plain(w.second_initial_parent_draft().unwrap()),"right":plain(w.second_initial_child_draft().unwrap()),
        "creation":(0..3).map(|i|json!({"kind":kinds[i],"text":final_text(i),"emptyLabel":if final_text(i).is_empty(){Some("Empty paragraph")}else{None}})).collect::<Vec<_>>(),
        "visualOrder":visual_order().map(|i|kinds[i]),"historyNotice":"Both paragraph splits and the edits between them remain part of this unresolved change."})
}
pub(in crate::canvas::text) fn matches(
    checkpoint: &super_platinum_core::canvas::edit::structure::recovery::Checkpoint,
) -> bool {
    let Ok(value) = serde_json::to_value(checkpoint) else {
        return false;
    };
    let a = &value["action"]["SplitSequence"];
    let w = witness(&TextDraft::from_rtml(corpus().seed_rtml).unwrap());
    let reports =
        |drafts: &[TextDraft]| drafts.iter().map(|d| d.rtml().unwrap()).collect::<Vec<_>>();
    value["version"] == 18
        && value["draft"] == corpus().seed_rtml
        && a["offset_utf16"] == corpus().offset
        && a["parent_draft"] == corpus().left_rtml
        && a["child_draft"] == typing::escaped(corpus().right)
        && a["parent_updates"] == json!(reports(w.parent_updates()))
        && a["child_updates"] == json!(reports(w.child_updates()))
        && a["second_target"]
            == if selector() == "parent" {
                "Original"
            } else {
                "FirstChild"
            }
        && a["second_enter_draft"] == w.second_enter_draft().unwrap().rtml().unwrap()
        && a["second_offset_utf16"] == second_left().encode_utf16().count()
        && a["second_parent_draft"] == w.second_initial_parent_draft().unwrap().rtml().unwrap()
        && a["second_child_draft"] == w.second_initial_child_draft().unwrap().rtml().unwrap()
        && a["final_updates"] == json!(w.final_updates().unwrap().each_ref().map(|v| reports(v)))
}
fn draft(value: &str) -> TextDraft {
    TextDraft::from_rtml(&typing::escaped(value)).unwrap()
}
pub(in crate::canvas::text) fn witness(original: &TextDraft) -> SplitWitness {
    let mut w = SplitWitness::new(
        original,
        corpus().offset,
        &draft(corpus().left),
        &draft(corpus().right),
    )
    .unwrap();
    let target = if selector() == "parent" { 0 } else { 1 };
    let mut before = [corpus().left.to_owned(), corpus().right.to_owned()];
    if cadence() == "edited" {
        let target_updates = [" draftx", " draft", " revised"]
            .map(|suffix| draft(&format!("{}{suffix}", before[target])));
        let other_updates = [draft(&format!("{} changed", before[1 - target]))];
        w = if target == 0 {
            w.with_parent_updates(&target_updates)
                .unwrap()
                .with_child_updates(&other_updates)
                .unwrap()
        } else {
            w.with_parent_updates(&other_updates)
                .unwrap()
                .with_child_updates(&target_updates)
                .unwrap()
        };
        before[target].push_str(" revised");
        before[1 - target].push_str(" changed");
    }
    let left = second_left();
    let right = before[target].strip_prefix(left).unwrap();
    w = w
        .with_second_split(
            if target == 0 {
                SplitSequenceTarget::Original
            } else {
                SplitSequenceTarget::FirstChild
            },
            &draft(&before[target]),
            left.encode_utf16().count(),
            &draft(left),
            &draft(right),
        )
        .unwrap();
    if cadence() == "edited" {
        let first_child = if target == 0 {
            before[1].as_str()
        } else {
            left
        };
        w = w
            .with_final_updates(
                &[TextDraft::default()],
                &[draft(&format!("{first_child} draftx")), draft(first_child)],
                &[
                    draft(&format!("{right} draftx")),
                    draft(&format!("{right} draft")),
                    draft(&format!("{right} revised")),
                ],
            )
            .unwrap();
    }
    w
}
