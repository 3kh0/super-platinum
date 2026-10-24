//! Fixed original-input histories before one middle split; no arbitrary fixture text.
use super::*;
use super_platinum_core::canvas::edit::{structure::SplitWitness, text::TextDraft};

pub(in crate::canvas::text) fn selector() -> &'static str {
    match std::env::var("SUPER_PLATINUM_CANVAS_SPLIT_PRE_ENTER").as_deref() {
        Ok("correction") => "correction",
        Ok("revert") => "revert",
        Ok("retype") => "retype",
        _ => "none",
    }
}
pub(in crate::canvas::text) fn enabled() -> bool {
    mode() == "middle" && selector() != "none"
}
pub(in crate::canvas::text) fn valid() -> bool {
    match std::env::var("SUPER_PLATINUM_CANVAS_SPLIT_PRE_ENTER") {
        Ok(value) => {
            value == "none"
                || (mode() == "middle"
                    && matches!(value.as_str(), "correction" | "revert" | "retype"))
        }
        Err(std::env::VarError::NotPresent) => true,
        Err(_) => false,
    }
}
pub(in crate::canvas::text) fn parent_text() -> &'static str {
    if selector() == "correction" {
        if corpus().name == "unicode" {
            "Caf\u{e9} & \u{65e5}\u{672c}\u{1f600} revised"
        } else {
            "Keep this revised"
        }
    } else {
        corpus().left
    }
}
fn reports_for(c: &Corpus, selector: &str) -> Vec<String> {
    match selector {
        "correction" => [" draftx", " draft", " revised"]
            .iter()
            .map(|suffix| format!("{}{suffix}{}", c.left, c.right))
            .collect(),
        "revert" => vec![format!("{} draftx{}", c.left, c.right), c.seed.into()],
        "retype" => vec![String::new(), c.seed.into()],
        _ => Vec::new(),
    }
}
pub(in crate::canvas::text) fn updates() -> Vec<TextDraft> {
    reports_for(corpus(), selector())
        .iter()
        .map(|text| TextDraft::from_rtml(&typing::escaped(text)).unwrap())
        .collect()
}
pub(in crate::canvas::text) fn witness(original: &TextDraft) -> SplitWitness {
    let reports = updates();
    let live = reports
        .last()
        .expect("pre-Enter fixture requires changed reports");
    SplitWitness::new_with_pre_enter_updates(
        original,
        live,
        parent_text().encode_utf16().count(),
        &TextDraft::from_rtml(&typing::escaped(parent_text())).unwrap(),
        &TextDraft::from_rtml(&typing::escaped(corpus().right)).unwrap(),
        &reports,
    )
    .unwrap()
    .with_child_updates(&typing_updates())
    .unwrap()
}
pub(in crate::canvas::text) fn parent_source(raw: &RawMessage) -> RawMessage {
    if parent_mode() == "bootstrap" {
        return raw.remove(38).unwrap();
    }
    let seeds: Value = serde_json::from_str(include_str!("premiddle-original.json")).unwrap();
    let seed = seeds["seeds"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["parentMode"] == parent_mode() && s["sourceRtml"] == corpus().seed_rtml)
        .expect("pre-middle retained history must match exact source and base");
    let wire = RawMessage::decode(
        &STANDARD
            .decode(seed["canonicalSourceSessionBase64"].as_str().unwrap())
            .unwrap(),
    )
    .unwrap()
    .replace_fields(4, &[n(4, 8_000_000_000_000_000)])
    .unwrap()
    .replace_fields(1, &[t(1, parent_id())])
    .unwrap();
    raw.replace_fields(1, &[t(1, parent_id())])
        .unwrap()
        .replace_fields(38, &[m(38, &wire)])
        .unwrap()
}
pub(in crate::canvas::text) fn snapshot(
    previous: &CanvasCollabSnapshot,
) -> Arc<CanvasCollabSnapshot> {
    let rows = previous
        .sections
        .iter()
        .map(|row| {
            if row.id() == "paragraph" {
                parent_source(
                    &row.raw()
                        .replace_fields(12, &[m(12, &body(corpus().seed_rtml, true))])
                        .unwrap(),
                )
            } else {
                row.raw().clone()
            }
        })
        .collect();
    rebuild(previous, rows)
}
pub(in crate::canvas::text) fn matches(
    checkpoint: &super_platinum_core::canvas::edit::structure::recovery::Checkpoint,
) -> bool {
    let Ok(v) = serde_json::to_value(checkpoint) else {
        return false;
    };
    let a = &v["action"]["SplitPreEnter"];
    let reports = updates()
        .iter()
        .map(|d| d.rtml().unwrap())
        .collect::<Vec<_>>();
    let children = typing_updates()
        .iter()
        .map(|d| d.rtml().unwrap())
        .collect::<Vec<_>>();
    v["version"] == 16
        && v["draft"] == corpus().seed_rtml
        && a["pre_enter_updates"] == json!(reports)
        && a["enter_draft"] == json!(reports.last().unwrap())
        && a["child_updates"] == json!(children)
        && a["parent_draft"] == typing::escaped(parent_text())
        && a["child_draft"] == typing::escaped(corpus().right)
        && a["offset_utf16"] == parent_text().encode_utf16().count()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pinned_preludes_preserve_tail_and_exact_revert_history() {
        for c in [&ASCII, &middle::UNICODE_TYPED] {
            let original = TextDraft::from_rtml(c.seed_rtml).unwrap();
            for selector in ["correction", "revert", "retype"] {
                let reports = reports_for(c, selector)
                    .iter()
                    .map(|s| TextDraft::from_rtml(&typing::escaped(s)).unwrap())
                    .collect::<Vec<_>>();
                let left = if selector == "correction" {
                    format!("{} revised", c.left)
                } else {
                    c.left.into()
                };
                let parent = TextDraft::from_rtml(&typing::escaped(&left)).unwrap();
                let child = TextDraft::from_rtml(&typing::escaped(c.right)).unwrap();
                let witness = SplitWitness::new_with_pre_enter_updates(
                    &original,
                    reports.last().unwrap(),
                    left.encode_utf16().count(),
                    &parent,
                    &child,
                    &reports,
                )
                .unwrap();
                assert_eq!(witness.child_draft().rtml().unwrap(), child.rtml().unwrap());
                assert_eq!(reports.len(), if selector == "correction" { 3 } else { 2 });
                if selector != "correction" {
                    assert!(reports.last() == Some(&original));
                }
            }
        }
    }
}
