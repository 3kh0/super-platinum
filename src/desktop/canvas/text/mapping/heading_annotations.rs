use super::*;
use super_platinum_core::canvas::wire::{EncodedField as F, Section};

fn snapshot(level: u64) -> CanvasCollabSnapshot {
    let mut snapshot =
        (*super::super::fixture::snapshot("<u><b>A&amp;\u{1f600}B</b></u><br><i>Last</i>", 1000))
            .clone();
    snapshot
        .file
        .extra
        .insert("access".into(), serde_json::json!("comment"));
    snapshot.thread = snapshot
        .thread
        .replace_fields(23, &[F::uint(23, 5).unwrap()])
        .unwrap();
    let section = snapshot
        .sections
        .iter_mut()
        .find(|s| s.id() == "paragraph")
        .unwrap();
    *section = Section::from_raw(
        section
            .raw()
            .replace_fields(8, &[F::bytes(8, b"aac").unwrap()])
            .unwrap()
            .replace_fields(10, &[F::uint(10, level).unwrap()])
            .unwrap(),
    )
    .unwrap();
    snapshot
}

fn export(body: &str) -> CanvasDocument {
    CanvasDocument::parse(&format!("<div class='quip-canvas-content'>{body}</div>")).unwrap()
}

#[test]
fn comment_only_heading_mapping_keeps_canonical_marks_and_no_edit_or_structure_grant() {
    let shell = ShellState::fixture(crate::media::MediaRegistry::default());
    for level in 1..=3 {
        let snapshot = snapshot(level);
        let document = export(&format!(
            "<h{level} id='paragraph'><span id='paragraph'>Lagging export</span></h{level}>"
        ));
        let map = Mapping::project(&shell, &document, Some(&snapshot));
        let item = map.items.get("paragraph").unwrap();
        let annotation = item.annotation.as_ref().unwrap();
        assert_eq!(
            annotation
                .runs
                .iter()
                .map(|run| run.text.as_str())
                .collect::<String>(),
            "A&\u{1f600}B\nLast"
        );
        assert!(annotation.runs[0].style.underline && annotation.runs[0].style.bold);
        assert!(annotation.runs.last().unwrap().style.italic);
        assert!(item.intent.is_none());
        assert!(item.structure.is_none());
        assert!(!item.replacement);
        assert!(!item.blocks.contains("Lagging export"));
        assert_eq!(
            snapshot.section("paragraph").unwrap().style().unwrap(),
            Some(level)
        );
    }
}

#[test]
fn heading_annotation_mapping_requires_unique_exact_export_level_and_carrier() {
    let shell = ShellState::fixture(crate::media::MediaRegistry::default());
    for level in 1..=3 {
        let snapshot = snapshot(level);
        let other = level % 3 + 1;
        for body in [
            "<p id='paragraph'>Same</p>".to_owned(),
            format!("<h{other} id='paragraph'>Same</h{other}>"),
            format!(
                "<h{level} id='paragraph'>Same</h{level}><h{level} id='paragraph'>Same</h{level}>"
            ),
            format!(
                "<h{level} id='paragraph'><span id='paragraph'>One</span><span id='paragraph'>Two</span></h{level}>"
            ),
        ] {
            assert!(
                Mapping::project(&shell, &export(&body), Some(&snapshot))
                    .items
                    .is_empty()
            );
        }
    }
}

#[test]
fn rendered_heading_does_not_create_comment_permission_or_placement() {
    let shell = ShellState::fixture(crate::media::MediaRegistry::default());
    let document = export("<h2 id='paragraph'>Rendered heading</h2>");
    let mut snapshot = snapshot(2);
    snapshot
        .file
        .extra
        .insert("access".into(), serde_json::json!("read"));
    assert!(
        Mapping::project(&shell, &document, Some(&snapshot))
            .items
            .is_empty()
    );
    snapshot
        .file
        .extra
        .insert("access".into(), serde_json::json!("comment"));
    let section = snapshot
        .sections
        .iter_mut()
        .find(|s| s.id() == "paragraph")
        .unwrap();
    *section = Section::from_raw(section.raw().remove(8).unwrap()).unwrap();
    assert!(
        Mapping::project(&shell, &document, Some(&snapshot))
            .items
            .is_empty()
    );
}
