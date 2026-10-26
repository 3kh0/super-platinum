//! Original retained split cohort and two original replacement cycles.
use super::journal::{Store, started};
use super::*;
use crate::canvas::edit::structure::recovery::Checkpoint;
use automerge::transaction::Transactable;
use automerge::{Automerge, ReadDoc};
use serde_json::Value;

const OLD: &str = "survivor";
const OWNER: &str = "paragraph";
const CHILD: &str = "temp:C:xyz0123456789abcdef012345678";
const FIRST: &str = "temp:C:xyzabcdef0123456789abcdef012";
const SECOND: &str = "temp:C:xyz123456789abcdef0123456789";

fn vectors() -> Value {
    let v: Value = serde_json::from_str(include_str!("fixtures/retained-original.json")).unwrap();
    assert_eq!(
        v["sha256"],
        "ea6beac1deffc820996d810627d525b88955b36cb6e917e3387c6b16eb54b7b4"
    );
    v
}
fn snapshot(cycle: usize, key: &str) -> CanvasCollabSnapshot {
    let mut s = fixture();
    s.sections.retain(|row| row.id() == "title");
    for encoded in vectors()["vectors"][cycle][key].as_array().unwrap() {
        let bytes = crate::slack::api::decode_canvas_base64(encoded.as_str().unwrap()).unwrap();
        let mut row = Section::decode(&bytes).unwrap();
        let pos = row.raw().string(8).unwrap().unwrap().to_owned();
        if let Some(path) = row.raw().string(21).unwrap().filter(|path| *path != pos) {
            assert_eq!(path, format!("{pos}:temp"));
            row = changed(&row, t(21, &pos));
        }
        let original_wire = row.crdt().unwrap().unwrap();
        let wire = original_wire
            .replace_fields(4, &[n(4, 8_000_000_000_000_000_000)])
            .unwrap();
        for field in [t(6, s.thread_id()), t(7, s.document_id()), m(38, &wire)] {
            row = changed(&row, field);
        }
        assert_eq!(
            row.crdt().unwrap().unwrap().bytes(5).unwrap(),
            original_wire.bytes(5).unwrap()
        );
        s.sections.push(row);
    }
    s
}
fn prepared_for(s: &CanvasCollabSnapshot, id: &str) -> PreparedReplacement {
    let mut intent = ReplacementIntent::main_canvas(s, id, 100).unwrap();
    if id == OLD {
        intent.replacement = FIRST.into();
    }
    if id == FIRST {
        intent.replacement = SECOND.into();
    }
    intent.prepare(s, 100).unwrap()
}
fn witnessed() -> CanvasCollabSnapshot {
    let source = snapshot(0, "inputRows");
    let p = prepared_for(&source, OLD);
    let rows = pair(&p, &source);
    let accepted = p.validate_ack(&ack(&rows)).unwrap();
    accepted.certify_readback(&applied(&source, &rows)).unwrap()
}
fn applied(s: &CanvasCollabSnapshot, rows: &[Section; 2]) -> CanvasCollabSnapshot {
    let mut result = s.clone();
    result.sections.retain(|row| row.id() != rows[0].id());
    result.sections.extend(rows.iter().cloned());
    result
}
fn replace(s: &mut CanvasCollabSnapshot, id: &str, row: Section) {
    *s.sections.iter_mut().find(|row| row.id() == id).unwrap() = row;
}
fn assert_ineligible(s: &CanvasCollabSnapshot, id: &str) {
    assert!(!can_replace_main_canvas(s, id, 100));
    assert!(ReplacementIntent::main_canvas(s, id, 100).is_err());
    assert!(
        paragraph_actions(s, &[id], 100)[0]
            .as_ref()
            .is_none_or(|a| !a.replace_main_canvas)
    );
}

#[test]
fn both_original_cycles_admit_only_the_independent_live_target() {
    for (cycle, id, count, next) in [(0, OLD, 4, "aaj:temp"), (1, FIRST, 5, "aak:temp")] {
        let s = if cycle == 0 {
            snapshot(0, "inputRows")
        } else {
            witnessed()
        };
        assert_eq!(s.sections.len(), count);
        Inventory::capture(&s).unwrap();
        let actions = paragraph_actions(&s, &[OWNER, CHILD, id, "title"], 100);
        assert!(actions[0].is_none() && actions[1].is_none() && actions[3].is_none());
        let action = actions[2].as_ref().unwrap();
        assert!(action.replace_main_canvas && !action.delete);
        assert!(StructureIntent::delete(&s, id, 100).is_err());
        let p = prepared_for(&s, id);
        let updates = p.document_data().messages(1).unwrap();
        assert_eq!(updates.len(), 2);
        assert_eq!(updates[0].string(1).unwrap(), Some(id));
        assert_eq!(updates[0].boolean(3).unwrap(), Some(true));
        assert_eq!(updates[1].string(8).unwrap(), Some(next));
        let original_bytes = crate::slack::api::decode_canvas_base64(
            vectors()["vectors"][cycle]["request"].as_str().unwrap(),
        )
        .unwrap();
        let original = RawMessage::decode(&original_bytes)
            .unwrap()
            .messages(1)
            .unwrap();
        assert_eq!(original.len(), 2);
        assert_eq!(original[0].uint(20).unwrap(), Some((cycle + 1) as u64));
        for (actual, expected) in updates.iter().zip(&original) {
            assert_eq!(actual.boolean(3).unwrap(), expected.boolean(3).unwrap());
            assert_eq!(actual.uint(2).unwrap(), expected.uint(2).unwrap());
            assert_eq!(
                actual.message(38).unwrap().unwrap().uint(2).unwrap(),
                expected.message(38).unwrap().unwrap().uint(2).unwrap()
            );
        }
        assert_eq!(
            text::canonical_draft(&Section::from_raw(updates[1].clone()).unwrap())
                .unwrap()
                .rtml()
                .unwrap(),
            ""
        );
        assert_eq!(p.document_data().messages(14).unwrap().len(), 1);
        p.checkpoint().unwrap();
    }
}

#[test]
fn original_zero_base_actor_and_deletion_suffix_are_explicitly_supported() {
    let s = snapshot(1, "canonicalRows");
    // Original raw history is not local provenance, even when syntax is sound.
    assert!(Inventory::capture(&s).is_err());
    let row = s.section(FIRST).unwrap();
    let wire = row.crdt().unwrap().unwrap();
    assert_eq!(wire.uint(2).unwrap(), Some(0));
    let doc = Automerge::load(wire.bytes(5).unwrap().unwrap()).unwrap();
    assert_eq!(
        doc.get_changes(&[])[0].actor_id().to_hex_string(),
        "ffeeddccbbaa99887766"
    );
    assert!(split::gap_tombstone(&s.sections, row).is_err());
    split::independent_history(row, 0).unwrap();
    assert!(!can_replace_main_canvas(&s, SECOND, 100));
    let source = witnessed();
    let p = prepared_for(&source, FIRST);
    let rows = pair(&p, &source);
    let accepted = p.validate_ack(&ack(&rows)).unwrap();
    let witnessed = accepted.certify_readback(&applied(&source, &rows)).unwrap();
    assert!(can_replace_main_canvas(&witnessed, SECOND, 100));
    prepared_for(&witnessed, SECOND).checkpoint().unwrap();
}

#[test]
fn repeated_core_pairs_keep_all_prior_rows_exact_without_fixed_row_cap() {
    let mut s = snapshot(0, "inputRows");
    let mut id = OLD.to_owned();
    for _ in 0..3 {
        let p = prepared_for(&s, &id);
        let checkpoint = p.checkpoint().unwrap();
        let roundtrip: Checkpoint =
            serde_json::from_value(serde_json::to_value(&checkpoint).unwrap()).unwrap();
        let store = Store::new();
        let receipt = started(&mut store.open(), roundtrip);
        assert!(store.open().get(checkpoint.scope()).unwrap().unwrap() == receipt);
        let rows = pair(&p, &s);
        let next = rows[1].id().to_owned();
        let accepted = p.validate_ack(&ack(&rows)).unwrap();
        let current = applied(&s, &rows);
        assert!(accepted.confirms_readback(&current).unwrap());
        for prior in &s.sections {
            if prior.id() != id {
                assert_eq!(current.section(prior.id()).unwrap(), prior);
            }
        }
        s = accepted.certify_readback(&current).unwrap();
        Inventory::capture(&s).unwrap();
        id = next;
    }
    assert_eq!(s.sections.len(), 7);
    assert!(can_replace_main_canvas(&s, &id, 100));
}

#[test]
fn retained_successor_reserves_position_in_intent_and_v2_checkpoint() {
    let mut s = snapshot(0, "inputRows");
    let moved = changed(&changed(s.section(OLD).unwrap(), t(8, "aad")), t(21, "aad"));
    replace(&mut s, OLD, moved);
    let p = prepared_for(&s, OLD);
    let inserted = p.document_data().messages(1).unwrap()[1].clone();
    assert_eq!(inserted.string(8).unwrap(), Some("aadaaaa:temp"));
    let c = p.checkpoint().unwrap();
    let mut value = serde_json::to_value(&c).unwrap();
    value["action"]["Replace"]["position"] = "aae:temp".into();
    let forged: Checkpoint = serde_json::from_value(value).unwrap();
    assert!(Store::new().open().begin(forged).is_err());
}

#[test]
fn missing_split_members_and_mutated_full_inventory_are_refused() {
    let source = witnessed();
    for missing in [OWNER, CHILD] {
        let mut s = source.clone();
        s.sections.retain(|row| row.id() != missing);
        assert_ineligible(&s, FIRST);
    }
    for variant in 0..7 {
        let mut s = source.clone();
        let old = s.section(OLD).unwrap();
        let row = match variant {
            0 => changed(old, t(7, "foreign")),
            1 => changed(old, t(8, "aaj")),
            2 => changed(old, m(13, &raw(&[t(1, OWNER)]))),
            3 => changed(old, n(1005, 1)),
            4 => changed(old, m(12, &raw(&[m(1, &raw(&[t(1, "forged")]))]))),
            5 => changed(old, m(38, &old.crdt().unwrap().unwrap().remove(6).unwrap())),
            _ => changed(
                old,
                m(
                    38,
                    &old.crdt()
                        .unwrap()
                        .unwrap()
                        .replace_fields(8, &[t(8, &"0".repeat(64))])
                        .unwrap(),
                ),
            ),
        };
        replace(&mut s, OLD, row);
        assert_ineligible(&s, FIRST);
    }
}

#[test]
fn incoming_and_outgoing_split_edges_never_become_independent_replacement() {
    let s = snapshot(0, "inputRows");
    let rows = s.sections.iter().collect::<Vec<_>>();
    let owner = s.section(OWNER).unwrap();
    let child = s.section(CHILD).unwrap();
    assert!(super::super::super::retained::independent(rows.iter().copied(), owner).is_err());
    assert!(super::super::super::retained::independent(rows.iter().copied(), child).is_err());
    let mut changed_source = s.clone();
    let ow = owner.crdt().unwrap().unwrap();
    let info = ow
        .message(10)
        .unwrap()
        .unwrap()
        .replace_fields(3, &[t(3, OLD)])
        .unwrap();
    replace(
        &mut changed_source,
        OWNER,
        changed(
            owner,
            m(38, &ow.replace_fields(10, &[m(10, &info)]).unwrap()),
        ),
    );
    assert_ineligible(&changed_source, OLD);
}

#[test]
fn pair_readback_requires_all_retained_bytes_and_both_effects() {
    let source = witnessed();
    let p = prepared_for(&source, FIRST);
    let rows = pair(&p, &source);
    let accepted = p.validate_ack(&ack(&rows)).unwrap();
    let current = applied(&source, &rows);
    assert!(accepted.confirms_readback(&current).unwrap());
    for missing in [OWNER, CHILD, OLD, rows[1].id()] {
        let mut changed_source = current.clone();
        changed_source.sections.retain(|r| r.id() != missing);
        assert!(!accepted.confirms_readback(&changed_source).unwrap_or(false));
    }
    for id in [OWNER, CHILD, OLD, "title"] {
        let mut changed_source = current.clone();
        replace(
            &mut changed_source,
            id,
            changed(current.section(id).unwrap(), n(1005, 9)),
        );
        assert!(!accepted.confirms_readback(&changed_source).unwrap_or(false));
    }
    let mut absent_old = current.clone();
    absent_old.sections.retain(|row| row.id() != FIRST);
    assert!(accepted.confirms_readback(&absent_old).unwrap());
    let mut partial = source.clone();
    replace(&mut partial, FIRST, rows[0].clone());
    assert!(!accepted.confirms_readback(&partial).unwrap_or(false));
    assert!(Inventory::capture(&partial).is_err());
}

#[test]
fn fresh_capture_rechecks_retained_bytes_and_uncertain_pair_ack_stays_rejected() {
    let source = snapshot(0, "inputRows");
    let intent = ReplacementIntent::main_canvas(&source, OLD, 100).unwrap();
    let mut mutated = source.clone();
    replace(
        &mut mutated,
        CHILD,
        changed(source.section(CHILD).unwrap(), n(1005, 17)),
    );
    assert!(intent.prepare(&mutated, 100).is_err());
    for variant in 0..4 {
        let p = prepared_for(&source, OLD);
        let rows = pair(&p, &source);
        let bad = match variant {
            0 => vec![rows[0].clone()],
            1 => vec![rows[1].clone()],
            2 => vec![rows[0].clone(), rows[0].clone()],
            _ => vec![changed(&rows[0], t(33, "foreign-0")), rows[1].clone()],
        };
        if variant == 2 {
            let data = raw(&bad.iter().map(|row| m(1, row.raw())).collect::<Vec<_>>());
            assert!(EditAck::decode(raw(&[n(1, 1), m(2, &data)]).raw()).is_err());
        } else {
            assert!(p.validate_ack(&ack(&bad)).is_err());
        }
    }
}

#[test]
fn independent_tombstone_rejects_extra_root_write_even_with_current_processed_head() {
    let mut s = witnessed();
    let old = s.section(OLD).unwrap();
    let wire = old.crdt().unwrap().unwrap();
    let mut doc = Automerge::load(wire.bytes(5).unwrap().unwrap())
        .unwrap()
        .with_actor("0123456789ab00000000".parse().unwrap());
    let mut tx = doc.transaction();
    tx.put(automerge::ROOT, "deleted", true).unwrap();
    tx.put(automerge::ROOT, "unknown", true).unwrap();
    tx.commit();
    let head = doc.get_heads()[0].to_string();
    let mut processed = wire
        .fields()
        .iter()
        .filter(|f| f.tag() == 6)
        .map(|f| {
            let value = RawMessage::decode(wire.encoded_field(f).unwrap()).unwrap();
            t(6, value.string(6).unwrap().unwrap())
        })
        .collect::<Vec<_>>();
    processed.push(t(6, &head));
    let wire = wire
        .replace_fields(5, &[F::bytes(5, &doc.save()).unwrap()])
        .unwrap()
        .replace_fields(6, &processed)
        .unwrap();
    let forged = changed(old, m(38, &wire));
    replace(&mut s, OLD, forged);
    assert_ineligible(&s, FIRST);
}

#[test]
fn witnessed_origin_survives_content_edit_fresh_read_and_next_replacement() {
    let source = witnessed();
    let text = text::TextIntent::capture(&source, FIRST, 100).unwrap();
    let p = text
        .prepare(
            &source,
            &mut super::super::super::super::EditSession::new(),
            &TextDraft::from_rtml("<b>Edited independent paragraph</b>").unwrap(),
            100,
        )
        .unwrap();
    let update = Section::from_raw(p.document_data().message(1).unwrap().unwrap()).unwrap();
    let old = source.section(FIRST).unwrap();
    let sequence = old.sequence().unwrap().unwrap() + 1000;
    let mut row = old.merge_partial(&update).unwrap();
    let wire = row
        .crdt()
        .unwrap()
        .unwrap()
        .remove(12)
        .unwrap()
        .replace_fields(3, &[n(3, sequence)])
        .unwrap()
        .replace_fields(4, &[n(4, 1_000_000)])
        .unwrap();
    row = changed(&changed(&row, n(2, sequence)), m(38, &wire));
    let accepted = p.validate_ack(&ack(&[row.clone()])).unwrap();
    let mut fresh = source.clone();
    fresh.replacement_provenance = None;
    replace(&mut fresh, FIRST, row);
    assert!(accepted.confirms_readback(&fresh).unwrap());
    assert!(Inventory::capture(&fresh).is_err());
    let carried = carry_replacement_provenance(&source, &fresh).unwrap();
    let pair = prepared_for(&carried, FIRST);
    let rows = super::pair(&pair, &carried);
    let accepted = pair.validate_ack(&ack(&rows)).unwrap();
    let certified = accepted
        .certify_readback(&applied(&carried, &rows))
        .unwrap();
    assert!(can_replace_main_canvas(&certified, SECOND, 100));
    assert_eq!(certified.section(OLD), source.section(OLD));
}

#[test]
fn private_certificate_checks_mutable_rows_scope_and_inert_restart_separately() {
    let source = witnessed();
    for id in [OWNER, CHILD, OLD] {
        let mut fresh = source.clone();
        fresh.replacement_provenance = None;
        fresh.sections.retain(|row| row.id() != id);
        assert!(carry_replacement_provenance(&source, &fresh).is_err());
        let mut mutated = source.clone();
        replace(
            &mut mutated,
            id,
            changed(source.section(id).unwrap(), n(1005, 9)),
        );
        assert!(Inventory::capture(&mutated).is_err());
        assert!(carry_replacement_provenance(&mutated, &source).is_err());
    }
    let mut other = fixture();
    other.sections = source.sections.clone();
    other.replacement_provenance = None;
    other.sections[0] = changed(&other.sections[0], t(7, "other-document"));
    assert!(carry_replacement_provenance(&source, &other).is_err());
    let workspace = crate::config::WorkspaceSession {
        team_id: source.init.workspace_id().into(),
        enterprise_id: None,
        user_id: "Uforeign".into(),
        name: "Fixture".into(),
        url: "https://fixture.slack.com".into(),
        token: "fixture-only".into(),
    };
    let options = source
        .init
        .options()
        .replace_fields(33, &[t(33, "Uforeign")])
        .unwrap();
    let mut foreign = source.clone();
    foreign.replacement_provenance = None;
    foreign.init = crate::slack::api::CanvasControllerInit::parse(
        serde_json::json!({
            "ok":true, "quip_user_id": source.init.quip_user_id(),
            "init_options": crate::slack::api::encode_canvas_base64(options.raw())
        }),
        &workspace,
    )
    .unwrap();
    assert!(carry_replacement_provenance(&source, &foreign).is_err());

    let checkpoint = prepared_for(&source, FIRST).checkpoint().unwrap();
    let encoded = serde_json::to_value(&checkpoint).unwrap();
    assert!(encoded.get("provenance").is_some());
    let restored: Checkpoint = serde_json::from_value(encoded.clone()).unwrap();
    let store = Store::new();
    let receipt = started(&mut store.open(), restored);
    assert!(store.open().get(checkpoint.scope()).unwrap().unwrap() == receipt);
    let mut cold = source.clone();
    cold.replacement_provenance = None;
    assert!(ReplacementIntent::main_canvas(&cold, FIRST, 100).is_err());
    let mut tampered = encoded;
    tampered["provenance"]["scope"]["user"] = "foreign".into();
    let tampered: Checkpoint = serde_json::from_value(tampered).unwrap();
    assert!(Store::new().open().begin(tampered).is_err());

    let old = prepared(&fixture()).checkpoint().unwrap();
    let bytes = serde_json::to_string(&old).unwrap();
    assert!(!bytes.contains("provenance"));
    let old: Checkpoint = serde_json::from_str(&bytes).unwrap();
    assert_eq!(serde_json::to_string(&old).unwrap(), bytes);
}

#[test]
fn captured_structural_intents_transport_existing_certificate_to_raw_preflight() {
    let source = witnessed();
    let mut raw = source.clone();
    raw.replacement_provenance = None;
    assert!(Inventory::capture(&raw).is_err());
    let insert = StructureIntent::insert(
        &source,
        FIRST,
        Side::After,
        &TextDraft::from_rtml("Another paragraph").unwrap(),
        100,
    )
    .unwrap();
    insert.prepare(&raw, 100).unwrap().checkpoint().unwrap();
    ReplacementIntent::main_canvas(&source, FIRST, 100)
        .unwrap()
        .prepare(&raw, 100)
        .unwrap()
        .checkpoint()
        .unwrap();
}

#[test]
fn second_pair_read_carries_prior_evidence_without_certifying_new_tombstone() {
    let source = witnessed();
    let prepared = prepared_for(&source, FIRST);
    let rows = pair(&prepared, &source);
    let accepted = prepared.validate_ack(&ack(&rows)).unwrap();
    let mut fresh = applied(&source, &rows);
    fresh.replacement_provenance = None;
    let carried = carry_replacement_provenance(&source, &fresh).unwrap();
    assert_eq!(carried.sections, fresh.sections);
    assert!(Inventory::capture(&carried).is_err());
    assert!(ReplacementIntent::main_canvas(&carried, SECOND, 100).is_err());
    let certified = accepted.certify_readback(&carried).unwrap();
    Inventory::capture(&certified).unwrap();
    assert!(can_replace_main_canvas(&certified, SECOND, 100));

    // Merely carrying a matching old certificate cannot admit any other delta.
    let mut wrong = fresh.clone();
    replace(
        &mut wrong,
        FIRST,
        changed(fresh.section(FIRST).unwrap(), n(1005, 7)),
    );
    let carried_wrong = carry_replacement_provenance(&source, &wrong).unwrap();
    assert!(Inventory::capture(&carried_wrong).is_err());
    assert!(accepted.certify_readback(&carried_wrong).is_err());
}
