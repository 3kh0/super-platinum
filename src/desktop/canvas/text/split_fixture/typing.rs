//! Fixed native typing cases; no caller-authored content enters the fixture.
use super::*;
use super_platinum_core::canvas::edit::text::TextDraft;

pub(super) fn selector() -> &'static str {
    match std::env::var("SUPER_PLATINUM_CANVAS_SPLIT_TYPING").as_deref() {
        Ok("correction") => "correction",
        Ok("erase") => "erase",
        Ok("restore") => "restore",
        _ => "none",
    }
}
pub(super) fn valid() -> bool {
    match std::env::var("SUPER_PLATINUM_CANVAS_SPLIT_TYPING") {
        Ok(value) => {
            value == "none"
                || matches!(value.as_str(), "correction" | "erase")
                || ((!is_end() || is_empty() || is_repeat()) && value == "restore")
        }
        Err(std::env::VarError::NotPresent) => true,
        Err(_) => false,
    }
}
fn values(selector: &str, unicode: bool) -> &'static [&'static str] {
    match (selector, unicode) {
        ("correction", false) => &["New", "New draftx", "New draft", "New paragraph"],
        ("correction", true) => &[
            "New",
            "New \u{65e5}\u{672c}\u{1f600} draftx",
            "New \u{65e5}\u{672c}\u{1f600} draft",
            "New \u{65e5}\u{672c}\u{1f600} paragraph",
        ],
        ("erase", _) => &["x", ""],
        _ => &[],
    }
}
pub(super) fn reports() -> &'static [&'static str] {
    if is_empty() || is_repeat() {
        return empty::reports(selector(), corpus().name == "unicode");
    }
    if is_start() {
        return super::start_typing::reports(selector(), corpus().name == "unicode");
    }
    if !is_end() {
        middle::reports(selector(), corpus().name == "unicode")
    } else {
        values(selector(), corpus().name == "unicode")
    }
}
pub(super) fn child() -> &'static str {
    if is_start() || is_repeat() {
        ""
    } else {
        reports().last().copied().unwrap_or(corpus().right)
    }
}
pub(super) fn escaped(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
pub(super) fn updates() -> Vec<TextDraft> {
    reports()
        .iter()
        .map(|text| TextDraft::from_rtml(&escaped(text)).unwrap())
        .collect()
}
pub(super) fn continued() -> &'static str {
    if is_empty() || is_repeat() {
        "Continued paragraph"
    } else if selector() == "none" && !is_start() {
        corpus().edited
    } else if corpus().name == "unicode" {
        "Continued \u{65e5}\u{672c}\u{1f600} paragraph"
    } else {
        "Continued paragraph"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn correction_and_erase_keep_ordered_reports_including_final_empty() {
        for unicode in [false, true] {
            let correction = values("correction", unicode);
            assert_eq!(correction.len(), 4);
            assert_eq!(format!("{}x", correction[2]), correction[1]);
            assert!(correction[3].ends_with("paragraph"));
            assert_eq!(values("erase", unicode), &["x", ""]);
            for text in correction.iter().chain(values("erase", unicode)) {
                assert_eq!(
                    TextDraft::from_rtml(&escaped(text))
                        .unwrap()
                        .rtml()
                        .unwrap(),
                    escaped(text)
                );
            }
        }
    }
}
