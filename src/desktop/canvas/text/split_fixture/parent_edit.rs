//! Fixed first-paragraph reports after one middle Enter.
use super::*;
use super_platinum_core::canvas::edit::{structure::SplitWitness, text::TextDraft};

pub(in crate::canvas::text) fn selector() -> &'static str {
    match std::env::var("SUPER_PLATINUM_CANVAS_SPLIT_MIDDLE_PARENT").as_deref() {
        Ok("correction") => "correction",
        Ok("erase") => "erase",
        Ok("revert") => "revert",
        _ => "none",
    }
}
pub(in crate::canvas::text) fn cadence() -> &'static str {
    match std::env::var("SUPER_PLATINUM_CANVAS_SPLIT_MIDDLE_CADENCE").as_deref() {
        Ok("child-first") => "child-first",
        Ok("interleaved") => "interleaved",
        _ => "parent-first",
    }
}
pub(in crate::canvas::text) fn enabled() -> bool {
    mode() == "middle" && selector() != "none"
}
pub(in crate::canvas::text) fn valid() -> bool {
    let parent = std::env::var("SUPER_PLATINUM_CANVAS_SPLIT_MIDDLE_PARENT");
    let order = std::env::var("SUPER_PLATINUM_CANVAS_SPLIT_MIDDLE_CADENCE");
    let parent_ok = match parent {
        Ok(v) => {
            v == "none"
                || (mode() == "middle" && matches!(v.as_str(), "correction" | "erase" | "revert"))
        }
        Err(std::env::VarError::NotPresent) => true,
        Err(_) => false,
    };
    let order_ok = match order {
        Ok(v) => {
            v == "parent-first"
                || (enabled() && matches!(v.as_str(), "child-first" | "interleaved"))
        }
        Err(std::env::VarError::NotPresent) => true,
        Err(_) => false,
    };
    parent_ok && order_ok
}
pub(in crate::canvas::text) fn initial_text() -> &'static str {
    pre_enter::parent_text()
}
pub(in crate::canvas::text) fn final_text() -> &'static str {
    match (
        selector(),
        corpus().name == "unicode",
        pre_enter::selector() == "correction",
    ) {
        ("erase", _, _) => "",
        ("correction", false, false) => "Keep this revised",
        ("correction", false, true) => "Keep this revised revised",
        ("correction", true, false) => "Caf\u{e9} & \u{65e5}\u{672c}\u{1f600} revised",
        ("correction", true, true) => "Caf\u{e9} & \u{65e5}\u{672c}\u{1f600} revised revised",
        _ => initial_text(),
    }
}
pub(in crate::canvas::text) fn updates() -> Vec<TextDraft> {
    let left = initial_text();
    let reports = match selector() {
        "correction" => [" draftx", " draft", " revised"]
            .iter()
            .map(|s| format!("{left}{s}"))
            .collect(),
        "erase" => vec![String::new()],
        "revert" => vec![format!("{left} draftx"), left.into()],
        _ => Vec::new(),
    };
    reports
        .iter()
        .map(|s| TextDraft::from_rtml(&typing::escaped(s)).unwrap())
        .collect()
}
pub(in crate::canvas::text) fn witness(original: &TextDraft) -> SplitWitness {
    let witness = if pre_enter::enabled() {
        pre_enter::witness(original)
    } else {
        SplitWitness::new(
            original,
            initial_text().encode_utf16().count(),
            &TextDraft::from_rtml(&typing::escaped(initial_text())).unwrap(),
            &TextDraft::from_rtml(&typing::escaped(corpus().right)).unwrap(),
        )
        .unwrap()
        .with_child_updates(&typing_updates())
        .unwrap()
    };
    witness.with_parent_updates(&updates()).unwrap()
}
pub(in crate::canvas::text) fn matches(
    checkpoint: &super_platinum_core::canvas::edit::structure::recovery::Checkpoint,
) -> bool {
    let Ok(v) = serde_json::to_value(checkpoint) else {
        return false;
    };
    let a = &v["action"]["SplitParent"];
    let reports =
        |drafts: Vec<TextDraft>| drafts.iter().map(|d| d.rtml().unwrap()).collect::<Vec<_>>();
    let pre = reports(pre_enter::updates());
    v["version"] == 17
        && v["draft"] == corpus().seed_rtml
        && a["pre_enter_updates"] == json!(pre)
        && a["enter_draft"] == json!(pre.last().map(String::as_str).unwrap_or(corpus().seed_rtml))
        && a["parent_updates"] == json!(reports(updates()))
        && a["child_updates"] == json!(reports(typing_updates()))
        && a["parent_draft"] == typing::escaped(initial_text())
        && a["child_draft"] == typing::escaped(corpus().right)
        && a["offset_utf16"] == initial_text().encode_utf16().count()
}
