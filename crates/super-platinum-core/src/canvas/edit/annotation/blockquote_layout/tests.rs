use super::*;
use crate::canvas::wire::{EncodedField as F, RawMessage};
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
fn changed(s: &Section, f: F) -> Section {
    Section::from_raw(
        s.raw()
            .merge_partial(&RawMessage::decode(f.raw()).unwrap())
            .unwrap(),
    )
    .unwrap()
}
fn quote(id: &str, position: &str, children: &[&str], nested: bool) -> Section {
    let children = children.iter().map(|id| text(1, id)).collect::<Vec<_>>();
    Section::from_raw(raw(&[
        text(1, id),
        number(2, 5),
        text(6, "thread"),
        text(7, "document"),
        text(8, position),
        number(9, 71),
        number(10, 50),
        number(29, u64::from(nested)),
        message(12, &raw(&[message(65, &raw(&children))])),
        text(99, "unchanged future metadata"),
    ]))
    .unwrap()
}
fn child(id: &str, position: &str) -> Section {
    Section::from_raw(raw(&[
        text(1, id),
        number(2, 5),
        text(6, "thread"),
        text(7, "document"),
        text(8, position),
        number(9, 0),
        number(10, 0),
        number(29, 1),
        message(12, &raw(&[message(1, &raw(&[text(1, "text")]))])),
    ]))
    .unwrap()
}

#[test]
fn loaded_deep_quote_tree_is_retained_without_applying_new_insertion_flattening() {
    let sections = vec![
        quote("outer", "b", &["middle"], false),
        quote("middle", "c", &["inner"], true),
        quote("inner", "d", &["first", "second"], true),
        child("first", "e"),
        child("second", "f"),
    ];
    let index = Quotes::capture(&sections).unwrap();
    assert_eq!(index.owner("second"), Some("inner"));
    assert_eq!(index.outer_root("second"), Some("outer"));
    assert_eq!(
        index.previous_position("second").unwrap().as_deref(),
        Some("e")
    );
    index.live_chain(&sections[4]).unwrap();
    index.current_except_anchor(&sections, None).unwrap();
    let mut edited = sections.clone();
    edited[1] = changed(&edited[1], number(35, 1));
    assert!(
        Quotes::capture(&edited)
            .unwrap()
            .live_chain(&edited[4])
            .is_err()
    );
    assert!(index.current_except_anchor(&edited, None).is_err());
    // Source records are untouched, including the original depth and unknown field.
    assert_eq!(
        sections[1].raw().string(99).unwrap(),
        Some("unchanged future metadata")
    );
}

#[test]
fn duplicate_missing_foreign_and_cyclic_quote_owners_never_form_an_inventory() {
    let base = vec![quote("outer", "b", &["first"], false), child("first", "c")];
    Quotes::capture(&base).unwrap();
    for root in [
        quote("outer", "b", &["first", "first"], false),
        quote("outer", "b", &["missing"], false),
    ] {
        assert!(Quotes::capture(&[root, base[1].clone()]).is_err());
    }
    assert!(Quotes::capture(&[base[0].clone(), changed(&base[1], text(6, "foreign"))]).is_err());
    assert!(
        Quotes::capture(&[quote("a", "b", &["b"], true), quote("b", "c", &["a"], true)]).is_err()
    );
    assert!(Quotes::capture(&[quote("a", "b", &["a"], true)]).is_err());
    assert!(Quotes::capture(&[base[0].clone(), changed(&base[1], number(29, 0))]).is_err());
}

#[test]
fn deleted_quote_child_and_ordinary_list_descendant_bytes_remain_canonical_custody() {
    let root = quote("outer", "b", &["list"], false);
    let mut list = child("list", "c");
    for f in [number(9, 1), number(10, 5), message(12, &raw(&[]))] {
        list = changed(&list, f);
    }
    let mut descendant = child("deleted", "d");
    for f in [
        number(29, 0),
        number(3, 1),
        message(13, &raw(&[text(1, "list"), number(4, 5)])),
        text(21, "c-d"),
    ] {
        descendant = changed(&descendant, f);
    }
    let all = vec![root, list, descendant];
    let index = Quotes::capture(&all).unwrap();
    assert_eq!(index.outer_root("deleted"), Some("outer"));
    assert!(index.live_chain(&all[2]).is_err());
    let mut different = all.clone();
    different[2] = changed(&different[2], text(99, "changed deleted child"));
    assert!(
        index
            .current_except_anchor(&different, Some("list"))
            .is_err()
    );
    assert!(
        index
            .current_except_anchor(&all[..2], Some("list"))
            .is_err()
    );
    index.current_except_anchor(&all, Some("list")).unwrap();
}

#[test]
fn merged_out_outer_table_cell_never_grants_nested_quote_body_authority() {
    let q = quote("quote", "b", &["text"], true);
    let body = child("text", "c");
    let table = Section::from_raw(raw(&[
        text(1, "table"),
        number(2, 5),
        text(6, "thread"),
        text(7, "document"),
        text(8, "a"),
        number(9, 33),
        number(10, 28),
        message(
            12,
            &raw(&[message(
                30,
                &raw(&[
                    message(1, &raw(&[text(1, "row"), text(2, "a")])),
                    message(2, &raw(&[text(1, "left"), text(2, "a")])),
                    message(2, &raw(&[text(1, "right"), text(2, "b")])),
                    message(
                        3,
                        &raw(&[
                            text(1, "first"),
                            text(2, "row"),
                            text(3, "left"),
                            message(6, &raw(&[message(2, &raw(&[text(1, "right")]))])),
                        ]),
                    ),
                    message(
                        3,
                        &raw(&[
                            text(1, "second"),
                            text(2, "row"),
                            text(3, "right"),
                            text(4, "quote"),
                        ]),
                    ),
                ]),
            )]),
        ),
    ]))
    .unwrap();
    let sections = vec![table, q, body];
    let index = super::super::table_layout::Tables::capture_quotes(&sections).unwrap();
    assert!(index.containing_child("text").unwrap().merged_out);
    assert!(index.admit_body(&sections[2]).is_err());
    assert!(super::super::table_layout::Tables::capture_bodies(&sections).is_err());
}
