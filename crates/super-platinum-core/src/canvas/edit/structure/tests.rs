use super::*;
use crate::canvas::wire::{DocumentData, EditAck};
use automerge::{ROOT, ReadDoc};

fn t(tag: u32, value: &str) -> F {
    F::bytes(tag, value.as_bytes()).unwrap()
}
fn n(tag: u32, value: u64) -> F {
    F::uint(tag, value).unwrap()
}
fn m(tag: u32, value: &RawMessage) -> F {
    F::message(tag, value).unwrap()
}
fn msg(fields: &[F]) -> RawMessage {
    RawMessage::from_fields(fields).unwrap()
}
fn changed(section: &Section, field: F) -> Section {
    let raw = RawMessage::decode(field.raw()).unwrap();
    Section::from_raw(section.raw().merge_partial(&raw).unwrap()).unwrap()
}
pub(crate) fn fixture() -> CanvasCollabSnapshot {
    let mut snapshot = super::super::text::tests::fixture();
    snapshot.sections.retain(|s| s.id() != "parent");
    for s in &mut snapshot.sections {
        *s = changed(s, t(8, if s.id() == "title" { "aaa" } else { "aac" }));
    }
    // A noncanonical extension varint remains byte-identical through inventory
    // and untouched readback comparisons; no protobuf reencoding is used.
    let mut raw = snapshot.sections[0].raw().raw().to_vec();
    raw.extend_from_slice(&[0xc8, 0x3e, 0x81, 0x00]); // unknown tag1001, value1
    snapshot.sections[0] = Section::decode(&raw).unwrap();
    let other = changed(&changed(&snapshot.sections[0], t(1, "other")), t(8, "aae"));
    snapshot.sections.push(other);
    snapshot
}
fn insert(snapshot: &CanvasCollabSnapshot) -> PreparedStructureEdit {
    StructureIntent::insert(
        snapshot,
        "item",
        Side::After,
        &TextDraft::from_rtml("&amp;&lt;<br>\u{1f600}<b>X</b>").unwrap(),
        100,
    )
    .unwrap()
    .prepare(snapshot, 100)
    .unwrap()
}
fn canonical_ack(prepared: &PreparedStructureEdit, snapshot: &CanvasCollabSnapshot) -> Section {
    let update = Section::from_raw(prepared.document_data().message(1).unwrap().unwrap()).unwrap();
    let base = snapshot.section(update.id());
    let sequence = base.and_then(|s| s.sequence().unwrap()).unwrap_or(0) + 1000;
    let mut result = base.map_or_else(|| update.clone(), |s| s.merge_partial(&update).unwrap());
    for field in [
        n(2, sequence),
        t(6, snapshot.thread_id()),
        t(7, snapshot.document_id()),
    ] {
        result = changed(&result, field);
    }
    if let Some(p) = result.raw().string(8).unwrap().map(str::to_owned) {
        if p.ends_with(":temp") {
            result = changed(&result, t(8, position::base(&p).unwrap()));
            result = changed(&result, t(21, position::base(&p).unwrap()));
        }
    }
    let wire = result
        .crdt()
        .unwrap()
        .unwrap()
        .remove(12)
        .unwrap()
        .replace_fields(3, &[n(3, sequence)])
        .unwrap()
        .replace_fields(4, &[n(4, 1_000_000)])
        .unwrap();
    changed(&result, m(38, &wire))
}
fn ack(section: &Section) -> EditAck {
    EditAck::decode(msg(&[n(1, 1), m(2, &msg(&[m(1, section.raw())]))]).raw()).unwrap()
}
fn readback(snapshot: &CanvasCollabSnapshot, section: &Section) -> CanvasCollabSnapshot {
    let mut fresh = snapshot.clone();
    if let Some(i) = fresh.sections.iter().position(|s| s.id() == section.id()) {
        fresh.sections[i] = section.clone();
    } else {
        fresh.sections.push(section.clone());
    }
    fresh
}

#[test]
fn exact_source_position_edges_and_invalid_order() {
    for (before, after, expected) in [
        (None, None, "aaa:temp"),
        (None, Some("aaa"), "aaZ:temp"),
        (Some("aaa"), None, "aab:temp"),
        (Some("aaa"), Some("aac"), "aab:temp"),
        (Some("aaa"), Some("aab"), "aaaaaaa:temp"),
        (Some("aaZ"), Some("aaa"), "aaZaaaa:temp"),
    ] {
        assert_eq!(position::between(before, after).unwrap(), expected);
    }
    for (before, after) in [
        (Some("aaa"), Some("aaa")),
        (Some("aac"), Some("aaa")),
        (Some("aaa:temp"), None),
        (Some(""), None),
    ] {
        assert!(position::between(before, after).is_err());
    }
}

#[test]
fn identity_uses_canonical_shard_and_is_fixed_before_prepare() {
    let snapshot = fixture();
    let draft = TextDraft::default();
    let intent = StructureIntent::insert(&snapshot, "item", Side::After, &draft, 100).unwrap();
    let id = intent.target_section().to_owned();
    assert!(id.starts_with("temp:C:Dte"));
    assert_eq!(id.len(), 35);
    let prepared = intent.prepare(&snapshot, 100).unwrap();
    assert_eq!(prepared.target_section(), id);
    let mut custom = snapshot.clone();
    custom.document = custom.document.replace_fields(42, &[t(42, "xyz")]).unwrap();
    assert_eq!(
        position::section_id(&custom, "0123456789abcdef012345678").unwrap(),
        "temp:C:xyz0123456789abcdef012345678"
    );
    custom.document = custom.document.replace_fields(42, &[t(42, "xx")]).unwrap();
    assert!(position::section_id(&custom, "0123456789abcdef012345678").is_err());
}

#[test]
fn insertion_projection_and_zero_sequence_dirty_bootstrap() {
    let snapshot = fixture();
    let before = snapshot.sections.clone();
    let prepared = insert(&snapshot);
    let section = Section::from_raw(prepared.document_data().message(1).unwrap().unwrap()).unwrap();
    assert_eq!(
        section
            .raw()
            .fields()
            .iter()
            .map(|f| f.tag())
            .collect::<Vec<_>>(),
        vec![
            1, 2, 3, 8, 9, 10, 11, 12, 13, 16, 19, 20, 21, 29, 33, 35, 38
        ]
    );
    assert_eq!(section.sequence().unwrap(), Some(0));
    assert_eq!(prepared.sequence(), 1);
    let draft = text::canonical_draft(&section).unwrap();
    let wire = section.crdt().unwrap().unwrap();
    let doc = crdt::validate(&wire, &section, &draft, false, None, true).unwrap();
    assert_eq!(wire.uint(2).unwrap(), Some(0));
    assert_eq!(wire.uint(3).unwrap(), Some(0));
    assert_eq!(wire.fields().iter().filter(|f| f.tag() == 6).count(), 1);
    assert_ne!(wire.string(8).unwrap(), wire.string(12).unwrap());
    let (_, object) = doc.get(ROOT, "text").unwrap().unwrap();
    let marks = doc.marks(&object).unwrap();
    // The escaped source model has 13 ASCII units, then one non-BMP
    // scalar (two UTF-16 units); bold X must remain at 15..16.
    assert_eq!(
        doc.text(&object).unwrap(),
        "&amp;&lt;<br>\u{1f600}X\u{feff}"
    );
    assert!(
        marks
            .iter()
            .any(|m| m.name() == "b" && m.start == 15 && m.end == 16)
    );
    assert_eq!(snapshot.sections, before);
}

#[test]
fn captured_js_wire_vectors_load_with_the_same_rich_and_deleted_semantics() {
    let cases: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("fixtures/source-wire.json")).unwrap();
    for case in cases {
        let bytes = crate::slack::api::decode_canvas_base64(case["bytes_base64"].as_str().unwrap())
            .unwrap();
        let data = DocumentData::decode(&bytes).unwrap();
        let section = &data.sections()[0];
        let draft = TextDraft::from_rtml(case["rtml"].as_str().unwrap()).unwrap();
        let deleted = case["deleted"].as_bool().unwrap();
        let doc = crdt::validate(
            &section.crdt().unwrap().unwrap(),
            section,
            &draft,
            deleted,
            None,
            !deleted,
        )
        .unwrap_or_else(|error| panic!("source fixture {}: {error:?}", case["name"]));
        text::crdt::check_document_state(&doc, &draft, deleted).unwrap();
        if !deleted {
            let native =
                crdt::insert(section.id(), &draft, "0123456789ab", 1792000000000000).unwrap();
            let native = crdt::validate(&native, section, &draft, false, None, true).unwrap();
            text::crdt::check_document_state(&native, &draft, false).unwrap();
        }
    }
}

#[test]
fn stale_inventory_including_unknown_bytes_title_order_and_owner_refuses_preflight() {
    let snapshot = fixture();
    for field in [t(1001, "changed extension"), t(8, "aad"), n(2, 10001)] {
        let prepared = insert(&snapshot);
        let mut fresh = snapshot.clone();
        fresh.sections[0] = changed(&fresh.sections[0], field);
        assert!(prepared.validate_snapshot(&fresh, 100).is_err());
    }
    let mut fresh = snapshot.clone();
    fresh.document = fresh.document.replace_fields(1001, &[n(1001, 1)]).unwrap();
    assert!(insert(&snapshot).validate_snapshot(&fresh, 100).is_err());
    let mut fresh = snapshot.clone();
    fresh.sections.retain(|s| s.id() != "title");
    assert!(insert(&snapshot).validate_snapshot(&fresh, 100).is_err());
    let mut fresh = snapshot.clone();
    fresh.thread = fresh
        .thread
        .replace_fields(106, &[t(106, "changed-secret-path")])
        .unwrap();
    assert!(insert(&snapshot).validate_snapshot(&fresh, 100).is_err());
}

#[test]
fn unsupported_structure_positions_layout_and_content_identity_never_silently_flatten() {
    for field in [
        t(8, "aaa"),
        t(8, "aac:temp"),
        n(29, 1),
        n(10, 7),
        m(13, &msg(&[t(1, "orphan")])),
        m(16, &msg(&[t(29, "content-id")])),
    ] {
        let mut snapshot = fixture();
        snapshot.sections[0] = changed(&snapshot.sections[0], field);
        assert!(StructureIntent::delete(&snapshot, "item", 100).is_err());
    }
    let mut snapshot = fixture();
    snapshot.sections.retain(|s| s.id() == "item");
    let inventory = Inventory::capture(&snapshot).unwrap();
    assert!(
        inventory.safe_delete("item").is_err(),
        "no inferred focus or replacement suppression"
    );
    let mut snapshot = fixture();
    snapshot.sections.retain(|s| s.id() != "other");
    assert!(
        StructureIntent::delete(&snapshot, "item", 100).is_err(),
        "title may be renderer-hidden"
    );
}

#[test]
fn ack_requires_exact_id_local_sequence_owner_sent_head_and_no_extra_updates() {
    let snapshot = fixture();
    for field in [
        t(1, "other-id"),
        n(20, 2),
        n(2, 0),
        t(6, "other-thread"),
        t(7, "other-document"),
        n(29, 1),
        n(3, 1),
        t(8, "zzz"),
        t(17, "legacy-id"),
    ] {
        let prepared = insert(&snapshot);
        let section = changed(&canonical_ack(&prepared, &snapshot), field);
        assert!(prepared.validate_ack(&ack(&section)).is_err());
    }
    let prepared = insert(&snapshot);
    let section = canonical_ack(&prepared, &snapshot);
    let wrong = crdt::insert(
        section.id(),
        &TextDraft::from_rtml("&amp;&lt;<br>\u{1f600}<b>X</b>").unwrap(),
        "fedcba987654",
        100,
    )
    .unwrap()
    .replace_fields(3, &[n(3, 1000)])
    .unwrap()
    .replace_fields(4, &[n(4, 1_000_000)])
    .unwrap();
    assert!(
        prepared
            .validate_ack(&ack(&changed(&section, m(38, &wrong))))
            .is_err()
    );
    let prepared = insert(&snapshot);
    let section = canonical_ack(&prepared, &snapshot);
    let wire = section.crdt().unwrap().unwrap();
    let wire = wire
        .replace_fields(6, &[t(6, wire.string(8).unwrap().unwrap())])
        .unwrap();
    assert!(
        prepared
            .validate_ack(&ack(&changed(&section, m(38, &wire))))
            .is_err(),
        "known pending head alone does not prove server processed it"
    );
    let prepared = insert(&snapshot);
    let section = canonical_ack(&prepared, &snapshot);
    let data = msg(&[m(1, section.raw()), m(1, snapshot.sections[0].raw())]);
    assert!(
        prepared
            .validate_ack(&EditAck::decode(msg(&[n(1, 1), m(2, &data)]).raw()).unwrap())
            .is_err()
    );
}

#[test]
fn insertion_readback_requires_full_exact_inventory_and_unknown_bytes() {
    let snapshot = fixture();
    let prepared = insert(&snapshot);
    let canonical = canonical_ack(&prepared, &snapshot);
    let accepted = prepared.validate_ack(&ack(&canonical)).unwrap();
    let fresh = readback(&snapshot, &canonical);
    assert!(accepted.confirms_readback(&fresh).unwrap());
    let mut missing = fresh.clone();
    missing.sections.retain(|s| s.id() != "item");
    assert!(!accepted.confirms_readback(&missing).unwrap());
    let mut changed = fresh.clone();
    changed.sections[0] = super::tests::changed(&changed.sections[0], n(1001, 1));
    assert!(
        !accepted.confirms_readback(&changed).unwrap(),
        "equivalent value with reencoded unknown bytes is still a different raw inventory"
    );
    let mut extra = fresh.clone();
    extra
        .sections
        .push(super::tests::changed(&snapshot.sections[0], t(1, "extra")));
    assert!(!accepted.confirms_readback(&extra).unwrap());
}

#[test]
fn deletion_preserves_rich_history_and_unknown_fields_requires_sealed_tombstone() {
    let snapshot = fixture();
    let prepared = StructureIntent::delete(&snapshot, "item", 100)
        .unwrap()
        .prepare(&snapshot, 100)
        .unwrap();
    let update = Section::from_raw(prepared.document_data().message(1).unwrap().unwrap()).unwrap();
    assert_eq!(
        update
            .raw()
            .fields()
            .iter()
            .map(|f| f.tag())
            .collect::<Vec<_>>(),
        vec![1, 2, 3, 9, 13, 19, 20, 33, 38]
    );
    assert!(!update.raw().has(12));
    assert!(!update.raw().has(8));
    let draft = text::canonical_draft(snapshot.section("item").unwrap()).unwrap();
    let wire = update.crdt().unwrap().unwrap();
    crdt::validate(&wire, &update, &draft, true, None, false).unwrap();
    assert_eq!(wire.fields().iter().filter(|f| f.tag() == 6).count(), 3);
    let canonical = canonical_ack(&prepared, &snapshot);
    let accepted = prepared.validate_ack(&ack(&canonical)).unwrap();
    let mut fresh = snapshot.clone();
    fresh.sections.retain(|s| s.id() != "item");
    assert!(accepted.confirms_readback(&fresh).unwrap());
    assert!(!accepted.confirms_readback(&snapshot).unwrap());
    assert!(
        accepted
            .confirms_readback(&readback(&snapshot, &canonical))
            .unwrap()
    );
}

#[test]
fn deletion_acks_cannot_drop_unrelated_unknown_or_language_fields() {
    let snapshot = fixture();
    let prepared = StructureIntent::delete(&snapshot, "item", 100)
        .unwrap()
        .prepare(&snapshot, 100)
        .unwrap();
    let section = canonical_ack(&prepared, &snapshot);
    let section = changed(&section, n(1001, 1));
    assert!(prepared.validate_ack(&ack(&section)).is_err());
}

#[test]
fn inserted_target_readback_requires_present_valid_ack_history_and_processed_head() {
    let snapshot = fixture();
    let prepared = insert(&snapshot);
    let canonical = canonical_ack(&prepared, &snapshot);
    let accepted = prepared.validate_ack(&ack(&canonical)).unwrap();
    let acknowledged = canonical.crdt().unwrap().unwrap();
    let missing = Section::from_raw(canonical.raw().remove(38).unwrap()).unwrap();
    let malformed = changed(
        &canonical,
        m(
            38,
            &acknowledged
                .replace_fields(5, &[t(5, "not automerge")])
                .unwrap(),
        ),
    );
    let replacement = crdt::insert(
        canonical.id(),
        &text::canonical_draft(&canonical).unwrap(),
        "fedcba987654",
        100,
    )
    .unwrap()
    .replace_fields(3, &[n(3, canonical.sequence().unwrap().unwrap())])
    .unwrap()
    .replace_fields(4, &[n(4, 1_000_000)])
    .unwrap();
    let replacement = changed(&canonical, m(38, &replacement));
    let unprocessed = changed(
        &canonical,
        m(
            38,
            &acknowledged
                .replace_fields(6, &[t(6, acknowledged.string(8).unwrap().unwrap())])
                .unwrap(),
        ),
    );
    for target in [missing, malformed, replacement, unprocessed] {
        assert!(
            !accepted
                .confirms_readback(&readback(&snapshot, &target))
                .unwrap()
        );
    }
    // This is reconciliation only: the sealed direct-ACK receipt remains valid.
    assert_eq!(accepted.target_section(), canonical.id());
    assert!(
        accepted
            .confirms_readback(&readback(&snapshot, &canonical))
            .unwrap()
    );
}

#[test]
fn present_tombstone_readback_requires_history_while_complete_absence_uses_sealed_ack() {
    let snapshot = fixture();
    let prepared = StructureIntent::delete(&snapshot, "item", 100)
        .unwrap()
        .prepare(&snapshot, 100)
        .unwrap();
    let canonical = canonical_ack(&prepared, &snapshot);
    let accepted = prepared.validate_ack(&ack(&canonical)).unwrap();
    let missing = Section::from_raw(canonical.raw().remove(38).unwrap()).unwrap();
    let wire = canonical.crdt().unwrap().unwrap();
    let malformed = changed(
        &canonical,
        m(38, &wire.replace_fields(5, &[t(5, "invalid")]).unwrap()),
    );
    for target in [missing, malformed] {
        assert!(
            !accepted
                .confirms_readback(&readback(&snapshot, &target))
                .unwrap()
        );
    }
    let mut absent = snapshot.clone();
    absent.sections.retain(|section| section.id() != "item");
    assert!(accepted.confirms_readback(&absent).unwrap());
    assert!(
        accepted
            .confirms_readback(&readback(&snapshot, &canonical))
            .unwrap()
    );
}

#[test]
fn insertion_ack_rejects_unrequested_status_attributes_and_unknown_semantic_fields() {
    let snapshot = fixture();
    for field in [
        m(15, &msg(&[])),
        m(16, &msg(&[n(4, 0), t(29, "unrequested-content-id")])),
        m(16, &msg(&[n(4, 0), n(1001, 1)])),
        n(1001, 1),
    ] {
        let prepared = insert(&snapshot);
        let canonical = changed(&canonical_ack(&prepared, &snapshot), field);
        assert!(prepared.validate_ack(&ack(&canonical)).is_err());
    }
    let prepared = insert(&snapshot);
    let canonical = canonical_ack(&prepared, &snapshot);
    assert!(prepared.validate_ack(&ack(&canonical)).is_ok());
}

#[test]
fn inserted_base_zero_history_can_anchor_then_delete_and_restore_original_inventory() {
    let original = fixture();
    let insertion = insert(&original);
    let inserted = canonical_ack(&insertion, &original);
    assert_eq!(inserted.crdt().unwrap().unwrap().uint(2).unwrap(), Some(0));
    let insertion_receipt = insertion.validate_ack(&ack(&inserted)).unwrap();
    let current = readback(&original, &inserted);
    assert!(insertion_receipt.confirms_readback(&current).unwrap());
    // A fresh independent operation can use the newly canonical row as anchor.
    StructureIntent::insert(
        &current,
        inserted.id(),
        Side::After,
        &TextDraft::default(),
        100,
    )
    .unwrap()
    .prepare(&current, 100)
    .unwrap();
    let deletion = StructureIntent::delete(&current, inserted.id(), 100)
        .unwrap()
        .prepare(&current, 100)
        .unwrap();
    let outgoing = deletion.document_data().message(1).unwrap().unwrap();
    assert_eq!(
        outgoing.message(38).unwrap().unwrap().uint(2).unwrap(),
        Some(0)
    );
    let tombstone = canonical_ack(&deletion, &current);
    let deletion_receipt = deletion.validate_ack(&ack(&tombstone)).unwrap();
    assert!(
        deletion_receipt
            .confirms_readback(&readback(&current, &tombstone))
            .unwrap()
    );
    assert!(deletion_receipt.confirms_readback(&original).unwrap());
}

#[test]
fn base_zero_continuation_is_not_a_general_legacy_identity_bypass() {
    let snapshot = fixture();
    let insertion = insert(&snapshot);
    let inserted = canonical_ack(&insertion, &snapshot);
    let legacy = changed(&inserted, t(1, "legacy-zero"));
    let wire = legacy
        .crdt()
        .unwrap()
        .unwrap()
        .replace_fields(1, &[t(1, "legacy-zero")])
        .unwrap();
    let legacy = changed(&legacy, m(38, &wire));
    let current = readback(&snapshot, &legacy);
    assert!(StructureIntent::delete(&current, "legacy-zero", 100).is_err());
    assert!(
        StructureIntent::insert(
            &current,
            "legacy-zero",
            Side::After,
            &TextDraft::default(),
            100
        )
        .is_err()
    );
}

#[test]
fn deletion_ack_allows_identity_reordering_but_keeps_exact_unknown_encoded_bytes() {
    let snapshot = fixture();
    let prepare = || {
        StructureIntent::delete(&snapshot, "item", 100)
            .unwrap()
            .prepare(&snapshot, 100)
            .unwrap()
    };
    let prepared = prepare();
    let canonical = canonical_ack(&prepared, &snapshot);
    // Move only the reviewed identity tags without decoding their payloads.
    let mut fields = canonical.raw().fields().iter().collect::<Vec<_>>();
    fields.sort_by_key(|field| match field.tag() {
        1 | 6 | 7 => field.tag(),
        _ => 0,
    });
    let mut bytes = Vec::new();
    for field in fields {
        bytes.extend_from_slice(canonical.raw().encoded_field(field).unwrap());
    }
    let reordered = Section::decode(&bytes).unwrap();
    assert_ne!(canonical.raw(), reordered.raw());
    prepared.validate_ack(&ack(&reordered)).unwrap();
    // Same logical value, different unknown encoding: do not normalize it away.
    let prepared = prepare();
    let canonical = canonical_ack(&prepared, &snapshot);
    let reencoded = changed(&canonical, n(1001, 1));
    assert!(prepared.validate_ack(&ack(&reencoded)).is_err());
}

#[test]
fn deletion_ack_preserves_repeated_unknown_field_order() {
    let mut snapshot = fixture();
    let index = snapshot
        .sections
        .iter()
        .position(|s| s.id() == "item")
        .unwrap();
    snapshot.sections[index] = Section::from_raw(
        snapshot.sections[index]
            .raw()
            .replace_fields(1002, &[t(1002, "first"), t(1002, "second")])
            .unwrap(),
    )
    .unwrap();
    let prepared = StructureIntent::delete(&snapshot, "item", 100)
        .unwrap()
        .prepare(&snapshot, 100)
        .unwrap();
    let canonical = canonical_ack(&prepared, &snapshot);
    let reversed = Section::from_raw(
        canonical
            .raw()
            .replace_fields(1002, &[t(1002, "second"), t(1002, "first")])
            .unwrap(),
    )
    .unwrap();
    assert!(prepared.validate_ack(&ack(&reversed)).is_err());
}

#[test]
fn deletion_ack_preserves_relative_order_of_distinct_unknown_fields() {
    let mut snapshot = fixture();
    let index = snapshot
        .sections
        .iter()
        .position(|s| s.id() == "item")
        .unwrap();
    let mut bytes = snapshot.sections[index].raw().raw().to_vec();
    bytes.extend_from_slice(t(1002, "first").raw());
    bytes.extend_from_slice(t(1003, "second").raw());
    snapshot.sections[index] = Section::decode(&bytes).unwrap();
    let prepared = StructureIntent::delete(&snapshot, "item", 100)
        .unwrap()
        .prepare(&snapshot, 100)
        .unwrap();
    let canonical = canonical_ack(&prepared, &snapshot);
    // Move tag1002 after tag1003 without changing either tag's encoded bytes.
    let reordered = changed(&canonical, t(1002, "first"));
    assert!(prepared.validate_ack(&ack(&reordered)).is_err());
}

#[test]
fn action_projection_preserves_core_eligibility_without_allocating_write_intents() {
    let snapshot = fixture();
    let actions = paragraph_actions(&snapshot, &["item", "title", "missing"], 100);
    let item = actions[0].as_ref().unwrap();
    assert!(item.insert_before && item.insert_after && item.delete);
    assert!(actions[1].is_none() && actions[2].is_none());
    let mut sole = snapshot.clone();
    sole.sections.retain(|s| s.id() != "other");
    assert!(
        !paragraph_actions(&sole, &["item"], 100)[0]
            .as_ref()
            .unwrap()
            .delete
    );
    let prepared = insert(&snapshot);
    let canonical = canonical_ack(&prepared, &snapshot);
    let current = readback(&snapshot, &canonical);
    assert!(
        paragraph_actions(&current, &[canonical.id()], 100)[0]
            .as_ref()
            .unwrap()
            .delete
    );
}

mod base_zero_text;

mod mixed;
mod start;
