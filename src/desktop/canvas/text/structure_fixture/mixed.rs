//! Synthetic mixed canonical/export shape; no session, transport or custom renderer.
use super::*;

pub(super) fn requested() -> bool {
    crate::fixture::is_fixture()
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("canvas-structure-mixed" | "canvas-structure-mixed-narrow")
        )
}
fn section(base: &Section, id: &str, position: &str, kind: u64, style: u64, text: &str) -> Section {
    let mut raw = base.raw().clone();
    for field in [
        t(1, id),
        t(8, position),
        t(21, position),
        n(9, kind),
        n(10, style),
        m(12, &msg(&[m(1, &msg(&[t(1, text), t(2, "en")]))])),
    ] {
        raw = raw
            .merge_partial(&RawMessage::decode(field.raw()).unwrap())
            .unwrap();
    }
    Section::from_raw(raw).unwrap()
}
pub(super) fn snapshot() -> Arc<CanvasCollabSnapshot> {
    let mut snapshot = (*super::snapshot()).clone();
    let base = snapshot.section("paragraph").unwrap();
    let heading = section(base, "mixed-heading", "aab", 0, 1, "Plan and context");
    let list = section(base, "mixed-list", "aad", 1, 5, "");
    let child = section(
        base,
        "mixed-child",
        "aadb",
        0,
        0,
        "Keep this list item unchanged",
    );
    let child = Section::from_raw(
        child
            .raw()
            .replace_fields(13, &[m(13, &msg(&[t(1, "mixed-list"), n(4, 5)]))])
            .unwrap()
            .replace_fields(21, &[t(21, "aad-aadb")])
            .unwrap(),
    )
    .unwrap();
    let unknown = section(
        base,
        "mixed-unknown",
        "aadd",
        987,
        765,
        "Retained embedded content",
    );
    let mut bytes = unknown.raw().raw().to_vec();
    bytes.extend_from_slice(&[0xc8, 0x3e, 0x81, 0x00]);
    snapshot
        .sections
        .extend([heading, list, child, Section::decode(&bytes).unwrap()]);
    Arc::new(snapshot)
}
pub(super) fn document(snapshot: &CanvasCollabSnapshot) -> CanvasDocument {
    let mut rows: Vec<_> = snapshot
        .sections
        .iter()
        .filter(|s| s.deleted().ok().flatten() != Some(true) && s.id() != "title")
        .collect();
    rows.sort_by_key(|s| {
        s.raw()
            .string(21)
            .ok()
            .flatten()
            .or_else(|| s.raw().string(8).ok().flatten())
            .unwrap_or("")
    });
    let mut html = "<div class='quip-canvas-content'><h1>Mixed Canvas verification</h1>".to_owned();
    for row in rows {
        match row.id() {
            "mixed-heading" => html.push_str("<h2 id='mixed-heading'>Plan and context</h2>"),
            "mixed-list" => {
                html.push_str("<ul><li id='mixed-child'>Keep this list item unchanged</li></ul>")
            }
            "mixed-child" => (),
            "mixed-unknown" => html
                .push_str("<blockquote id='mixed-unknown'>Retained embedded content</blockquote>"),
            _ => {
                let text = row
                    .raw()
                    .message(12)
                    .unwrap()
                    .unwrap()
                    .message(1)
                    .unwrap()
                    .unwrap();
                // Only synthetic admitted IDs and core-produced RTML reach this fixture export.
                html.push_str(&format!(
                    "<p id='{}'>{}</p>",
                    row.id(),
                    text.string(1).unwrap().unwrap_or("")
                ));
            }
        }
    }
    html.push_str("</div>");
    CanvasDocument::parse(&html).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    use super_platinum_core::canvas::edit::structure::{Side, paragraph_actions};
    use super_platinum_core::canvas::edit::text::TextDraft;
    #[test]
    fn mixed_fixture_opens_existing_paragraph_actions_only() {
        let snapshot = snapshot();
        let targets = [
            "paragraph",
            "neighbor",
            "mixed-heading",
            "mixed-list",
            "mixed-child",
            "mixed-unknown",
        ];
        let actions = paragraph_actions(&snapshot, &targets, 100);
        assert!(actions[..2].iter().all(|a| {
            a.as_ref()
                .is_some_and(|a| a.insert_before && a.insert_after && a.delete)
        }));
        assert!(actions[2..].iter().all(Option::is_none));
        document(&snapshot);
    }
    #[test]
    fn mixed_fixture_existing_receipt_path_preserves_every_sibling() {
        let original = snapshot();
        let prepared = StructureIntent::insert(
            &original,
            "paragraph",
            Side::After,
            &TextDraft::from_rtml("Inserted <b>mixed</b> paragraph").unwrap(),
            100,
        )
        .unwrap()
        .prepare(&original, 100)
        .unwrap();
        let (accepted, current) = super::super::accepted(prepared, &original, 100);
        assert!(accepted.confirms_readback(&current).unwrap());
        for old in &original.sections {
            assert_eq!(
                current.section(old.id()).unwrap().raw().raw(),
                old.raw().raw()
            );
        }
        document(&current);
    }
}
