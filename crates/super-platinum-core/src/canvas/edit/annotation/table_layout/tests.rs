use super::*;
use crate::canvas::wire::EncodedField as F;

#[test]
fn merged_out_list_ancestor_cannot_supply_native_containing_control_authority() {
    for merged in [false, true] {
        let mut rows = fixture(merged);
        let at = rows.iter().position(|s| s.id() == "second").unwrap();
        rows[at] = edit(&rows[at], number(9, 1));
        rows[at] = edit(&rows[at], number(10, 5));
        rows[at] = edit(&rows[at], message(12, &raw(&[])));
        let original = child("list-child", "o");
        let mut descendant = Section::from_raw(original.raw().remove(29).unwrap()).unwrap();
        for field in [
            text(21, "n-o"),
            message(13, &raw(&[text(1, "second"), number(4, 5)])),
            message(
                12,
                &raw(&[message(
                    1,
                    &raw(&[text(1, "<annotation id=\"old\">retained body</annotation>")]),
                )]),
            ),
        ] {
            descendant = edit(&descendant, field);
        }
        rows.push(descendant);
        rows.push(
            Section::from_raw(raw(&[
                text(1, "old"),
                number(2, 51),
                text(6, "thread"),
                text(7, "document"),
                text(8, "z"),
                number(9, 9),
                number(11, 1),
                message(12, &raw(&[message(7, &raw(&[text(3, "second")]))])),
            ]))
            .unwrap(),
        );
        let tables = Tables::capture_bodies(&rows).unwrap();
        assert_eq!(
            tables.containing_child("list-child").unwrap().merged_out,
            merged
        );
        assert_eq!(tables.admit_child(&rows[at]).is_err(), merged);
        let topology =
            super::super::control_topology::ControlTopology::from_sections_with_table_bodies(
                &rows, "thread", "document",
            )
            .unwrap();
        assert_eq!(
            topology.after_clear("old").unwrap(),
            if merged {
                "zzzzzz-orphaned-m".to_owned()
            } else {
                format!("n-{}", super::position::between(None, Some("o")).unwrap())
            }
        );
        assert!(Tables::capture(&rows).is_err());
    }
}

fn text(tag: u32, value: &str) -> F {
    F::bytes(tag, value.as_bytes()).unwrap()
}
fn number(tag: u32, value: u64) -> F {
    F::uint(tag, value).unwrap()
}
fn raw(fields: &[F]) -> RawMessage {
    RawMessage::from_fields(fields).unwrap()
}
fn message(tag: u32, value: &RawMessage) -> F {
    F::message(tag, value).unwrap()
}
fn edit(section: &Section, field: F) -> Section {
    // Construct a canonical observation, including intentionally foreign
    // thread/document negatives. Section::merge_partial correctly refuses an
    // identity migration before the Tables admission under test can inspect it.
    Section::from_raw(
        section
            .raw()
            .merge_partial(&RawMessage::decode(field.raw()).unwrap())
            .unwrap(),
    )
    .unwrap()
}
fn child(id: &str, order: &str) -> Section {
    Section::from_raw(raw(&[
        text(1, id),
        number(2, 41),
        text(6, "thread"),
        text(7, "document"),
        text(8, order),
        number(9, 0),
        number(10, 0),
        number(29, 1),
        message(12, &raw(&[message(1, &raw(&[text(1, "Kept text")]))])),
    ]))
    .unwrap()
}
fn cell(id: &str, column: &str, section: &str) -> RawMessage {
    raw(&[
        text(1, id),
        text(2, "row"),
        text(3, column),
        text(4, section),
    ])
}
fn fixture(merge: bool) -> Vec<Section> {
    let mut first = cell("cell1", "col1", "first");
    if merge {
        first = first
            .replace_fields(
                6,
                &[message(
                    6,
                    &raw(&[message(2, &raw(&[text(1, "col2"), text(2, "n")]))]),
                )],
            )
            .unwrap();
    }
    let layout = raw(&[
        message(1, &raw(&[text(1, "row"), text(2, "m")])),
        message(2, &raw(&[text(1, "col2"), text(2, "n")])),
        message(2, &raw(&[text(1, "col1"), text(2, "m")])),
        message(3, &first),
        message(3, &cell("cell2", "col2", "second")),
        number(5, 1),
        text(99, "retained future metadata"),
    ]);
    let table = Section::from_raw(raw(&[
        text(1, "table"),
        number(2, 31),
        text(6, "thread"),
        text(7, "document"),
        text(8, "l"),
        number(9, 33),
        number(10, 28),
        message(12, &raw(&[message(30, &layout)])),
    ]))
    .unwrap();
    vec![table, child("second", "n"), child("first", "m")]
}

#[test]
fn raw_table_cell_membership_is_exact_and_merge_host_is_not_its_removed_child() {
    let sections = fixture(true);
    let tables = Tables::capture(&sections).unwrap();
    let first = tables.child("first").unwrap();
    let second = tables.child("second").unwrap();
    assert_eq!(
        (
            first.table.as_str(),
            first.cell.as_str(),
            first.row.as_str(),
            first.column.as_str()
        ),
        ("table", "cell1", "row", "col1")
    );
    assert!(!first.merged_out);
    assert!(second.merged_out);
    assert!(tables.child("table").is_none());
    tables.admit_child(&sections[2]).unwrap();
    assert!(tables.admit_child(&sections[1]).is_err());
    tables.current(&sections).unwrap();
    // Unknown metadata is retained; it cannot change under the same receipt.
    let mut changed = sections.clone();
    let layout = changed[0]
        .raw()
        .message(12)
        .unwrap()
        .unwrap()
        .message(30)
        .unwrap()
        .unwrap();
    let changed_layout = layout
        .replace_fields(99, &[text(99, "different metadata")])
        .unwrap();
    changed[0] = edit(
        &changed[0],
        message(12, &raw(&[message(30, &changed_layout)])),
    );
    assert!(Tables::capture(&changed).is_ok());
    assert!(tables.current(&changed).is_err());
}

#[test]
fn actual_all_sibling_boundary_stops_at_top_level_but_keeps_deleted_layout_siblings() {
    let mut sections = fixture(false);
    let tables = Tables::capture(&sections).unwrap();
    assert_eq!(tables.previous_position(sections[2].id()).unwrap(), None);
    assert_eq!(
        tables
            .previous_position(sections[1].id())
            .unwrap()
            .as_deref(),
        Some("m")
    );
    sections[2] = edit(&sections[2], number(3, 1));
    assert_eq!(
        Tables::capture(&sections)
            .unwrap()
            .previous_position(sections[1].id())
            .unwrap()
            .as_deref(),
        Some("m")
    );
    let boundary = child("ordinary", "mm");
    sections.push(Section::from_raw(boundary.raw().remove(29).unwrap()).unwrap());
    assert_eq!(
        Tables::capture(&sections)
            .unwrap()
            .previous_position(sections[1].id())
            .unwrap(),
        None
    );
    let duplicate = child("duplicate", "m");
    sections.push(Section::from_raw(duplicate.raw().remove(29).unwrap()).unwrap());
    assert!(Tables::capture(&sections).is_err());
}

#[test]
fn absent_flag_foreign_owner_duplicate_assignment_and_missing_child_are_refused() {
    let sections = fixture(false);
    let mut missing = sections.clone();
    missing.pop();
    assert!(Tables::capture(&missing).is_err());
    for field in [
        number(29, 0),
        text(6, "foreign-thread"),
        text(7, "foreign-document"),
        message(13, &raw(&[text(1, "other")])),
    ] {
        let mut changed = sections.clone();
        changed[2] = edit(&changed[2], field);
        assert!(Tables::capture(&changed).is_err());
    }
    let mut duplicate = sections.clone();
    let layout = duplicate[0]
        .raw()
        .message(12)
        .unwrap()
        .unwrap()
        .message(30)
        .unwrap()
        .unwrap();
    let cells = [
        message(3, &cell("cell1", "col1", "first")),
        message(3, &cell("cell2", "col2", "first")),
    ];
    duplicate[0] = edit(
        &duplicate[0],
        message(
            12,
            &raw(&[message(30, &layout.replace_fields(3, &cells).unwrap())]),
        ),
    );
    assert!(Tables::capture(&duplicate).is_err());
}

#[test]
fn deleted_child_membership_still_has_exact_table_custody() {
    let mut sections = fixture(false);
    sections[1] = edit(&sections[1], number(3, 1));
    let original = Tables::capture(&sections).unwrap();
    assert!(original.child("second").is_some());
    let mut changed_deleted = sections.clone();
    changed_deleted[1] = edit(&changed_deleted[1], text(99, "changed deleted child"));
    assert!(original.current(&changed_deleted).is_err());
    let mut removed = sections.clone();
    removed.remove(1);
    assert!(original.current(&removed).is_err());
    let layout = sections[0]
        .raw()
        .message(12)
        .unwrap()
        .unwrap()
        .message(30)
        .unwrap()
        .unwrap();
    let swapped = [
        message(3, &cell("cell1", "col1", "second")),
        message(3, &cell("cell2", "col2", "first")),
    ];
    sections[0] = edit(
        &sections[0],
        message(
            12,
            &raw(&[message(30, &layout.replace_fields(3, &swapped).unwrap())]),
        ),
    );
    assert!(Tables::capture(&sections).is_ok());
    assert!(original.current(&sections).is_err());
}
