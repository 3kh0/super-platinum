//! Actual native F source/request/ACK/readback, with its opaque Section extension.
//! The envelope follows the native fixture and captured checkpoint scope; row bytes are exact.
use super::*;
use automerge::{Automerge, ReadDoc, transaction::Transactable};
use serde_json::Value;

fn vector() -> Value {
    let v: Value = serde_json::from_str(include_str!("fixtures/native-f-opaque.json")).unwrap();
    assert_eq!(
        v["sha256"],
        "de142f87bcb90fdbe1958eac9308032f55f39b31fc6d3cef8d39907d06fe97d1"
    );
    v
}
fn bytes(v: &Value) -> Vec<u8> {
    crate::slack::api::decode_canvas_base64(v.as_str().unwrap()).unwrap()
}
fn snapshot(v: &Value, key: &str) -> CanvasCollabSnapshot {
    use crate::canvas::wire::LoadDataResponse;
    use crate::slack::api::CanvasControllerInit;
    let scope = &v["scope"];
    let workspace = crate::config::WorkspaceSession {
        team_id: scope["workspace"].as_str().unwrap().into(),
        enterprise_id: None,
        user_id: scope["user"].as_str().unwrap().into(),
        name: "Synthetic".into(),
        url: "https://synthetic.slack.com/".into(),
        token: "synthetic".into(),
    };
    let options = raw(&[
        t(1, "quip-user"),
        t(33, &workspace.user_id),
        t(34, &workspace.team_id),
    ]);
    let init = CanvasControllerInit::parse(
        serde_json::json!({
            "ok":true, "quip_user_id":"quip-user",
            "init_options":crate::slack::api::encode_canvas_base64(options.raw())
        }),
        &workspace,
    )
    .unwrap();
    let thread_id = v["thread"].as_str().unwrap();
    let document_id = v["document"].as_str().unwrap();
    let file = serde_json::from_value(serde_json::json!({
        "id":scope["file"], "filetype":"quip", "quip_thread_id":thread_id, "editable":true
    }))
    .unwrap();
    let thread = raw(&[
        t(1, thread_id),
        n(2, 1000),
        t(10, document_id),
        n(22, 13),
        n(23, 13),
        n(119, 1),
    ]);
    // Native text fixture envelope plus structure fixture's modern-ID prefix.
    // Section rows below are neither renamed nor re-encoded.
    let document = raw(&[
        t(1, document_id),
        n(2, 1000),
        t(6, thread_id),
        n(12, 1),
        m(69, &raw(&[t(1, "Fixture title")])),
        t(42, "xyz"),
    ]);
    let sections = v[key]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| Section::decode(&bytes(b)).unwrap())
        .collect::<Vec<_>>();
    let mut payload = vec![m(3, &thread), m(6, &document)];
    payload.extend(sections.iter().map(|r| m(7, r.raw())));
    let mut index = vec![t(1, &format!("Section/{document_id}"))];
    index.extend(
        sections
            .iter()
            .filter(|r| r.deleted().unwrap() != Some(true))
            .map(|r| m(2, &raw(&[t(1, r.id())]))),
    );
    let response = raw(&[n(1, 1), m(2, &raw(&payload)), m(3, &raw(&index))]);
    let load = LoadDataResponse::decode(raw(&[m(2, &response)]).raw()).unwrap();
    let s = CanvasCollabSnapshot::from_responses(file, init, load).unwrap();
    assert_eq!(s.sections, sections);
    assert_eq!(s.file_id(), scope["file"].as_str().unwrap());
    assert_eq!(s.init.workspace_id(), scope["workspace"].as_str().unwrap());
    assert_eq!(s.init.options().string(33).unwrap(), scope["user"].as_str());
    for row in &s.sections {
        assert_eq!(row.thread_id().unwrap(), Some(s.thread_id()));
        assert_eq!(row.document_id().unwrap(), Some(s.document_id()));
    }
    s
}
pub(super) fn captured() -> (
    CanvasCollabSnapshot,
    CanvasCollabSnapshot,
    AcceptedReplacement,
) {
    let v = vector();
    let source = snapshot(&v, "inputRows");
    let readback = snapshot(&v, "readbackRows");
    let request = RawMessage::decode(&bytes(&v["request"])).unwrap();
    let update = request.messages(1).unwrap().remove(0);
    let wire = update.message(38).unwrap().unwrap();
    let doc = Automerge::load(wire.bytes(5).unwrap().unwrap()).unwrap();
    let head = doc.get_heads()[0];
    let now =
        u64::try_from(doc.get_change_by_hash(&head).unwrap().timestamp()).unwrap() * 1_000_000;
    let mut intent = ReplacementIntent::main_canvas(&source, "neighbor", now).unwrap();
    intent.session = v["session"].as_str().unwrap().into();
    intent.replacement = v["replacement"].as_str().unwrap().into();
    let prepared = intent.prepare(&source, now).unwrap();
    // Reconstruct the sealed pair through the real constructor, not a fabricated receipt.
    assert_eq!(prepared.document_data().raw(), request.raw());
    let accepted = prepared
        .validate_ack(&EditAck::decode(&bytes(&v["ack"])).unwrap())
        .unwrap();
    assert!(accepted.confirms_readback(&readback).unwrap());
    (source, readback, accepted)
}
fn replace(s: &mut CanvasCollabSnapshot, id: &str, row: Section) {
    *s.sections.iter_mut().find(|r| r.id() == id).unwrap() = row;
}

#[test]
fn native_f_opaque_tombstone_requires_exact_witnessed_certification() {
    let (source, readback, accepted) = captured();
    let old = readback.section("neighbor").unwrap();
    assert!(
        source
            .section("neighbor")
            .unwrap()
            .crdt()
            .unwrap()
            .is_none()
    );
    let extension = |row: &Section| {
        row.raw()
            .fields()
            .iter()
            .filter(|f| f.tag() == 1001)
            .flat_map(|f| row.raw().encoded_field(f).unwrap().to_vec())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        extension(old),
        extension(source.section("neighbor").unwrap())
    );
    assert!(!extension(old).is_empty());
    assert!(
        split::independent_history(old, 0).is_err(),
        "generic closed grammar is unchanged"
    );
    assert!(
        Inventory::capture(&readback).is_err(),
        "raw rows cannot mint provenance"
    );
    let certified = accepted.certify_readback(&readback).unwrap();
    assert_eq!(certified.sections, readback.sections);
    Inventory::capture(&certified).unwrap();
    assert!(can_replace_main_canvas(
        &certified,
        accepted.replacement_section(),
        100
    ));
    for row in [
        changed(old, n(1001, 2)),
        Section::from_raw(old.raw().remove(1001).unwrap()).unwrap(),
        changed(old, t(1002, "unwitnessed")),
    ] {
        let mut altered = readback.clone();
        replace(&mut altered, "neighbor", row.clone());
        assert!(!accepted.confirms_readback(&altered).unwrap_or(false));
        assert!(accepted.certify_readback(&altered).is_err());
        assert!(carry_replacement_provenance(&certified, &altered).is_err());
        replace(&mut altered, "neighbor", row);
        altered.replacement_provenance = certified.replacement_provenance.clone();
        assert!(Inventory::capture(&altered).is_err());
    }
    for id in ["paragraph", "neighbor"] {
        let mut orphan = certified.clone();
        orphan.sections.retain(|r| r.id() != id);
        assert!(Inventory::capture(&orphan).is_err());
        orphan.replacement_provenance = None;
        assert!(carry_replacement_provenance(&certified, &orphan).is_err());
        if id == "paragraph" {
            assert!(Inventory::capture(&orphan).is_err());
        } else {
            // No unproven ordinary tombstone remains: the intact retained split
            // pair plus an independent live row is valid without a certificate.
            // Carry must still refuse losing its previously certified neighbor.
            Inventory::capture(&orphan).unwrap();
            orphan.sections.retain(|r| r.id() != "paragraph");
            assert!(Inventory::capture(&orphan).is_err(), "missing split owner");
        }
    }
}

#[test]
fn native_f_certificate_survives_content_then_second_replacement() {
    let (_, readback, accepted) = captured();
    let source = accepted.certify_readback(&readback).unwrap();
    let id = accepted.replacement_section();
    let p = text::TextIntent::capture(&source, id, 100)
        .unwrap()
        .prepare(
            &source,
            &mut super::super::super::super::EditSession::new(),
            &TextDraft::from_rtml("<b>Edited independent paragraph</b>").unwrap(),
            100,
        )
        .unwrap();
    let update = Section::from_raw(p.document_data().message(1).unwrap().unwrap()).unwrap();
    let old = source.section(id).unwrap();
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
    let text_receipt = p.validate_ack(&ack(&[row.clone()])).unwrap();
    let mut fresh = readback.clone();
    replace(&mut fresh, id, row);
    assert!(text_receipt.confirms_readback(&fresh).unwrap());
    assert!(Inventory::capture(&fresh).is_err());
    let carried = carry_replacement_provenance(&source, &fresh).unwrap();
    let p = ReplacementIntent::main_canvas(&carried, id, 100)
        .unwrap()
        .prepare(&fresh, 100)
        .unwrap();
    p.checkpoint().unwrap(); // Inert serialized evidence retains exact opaque bytes too.
    let rows = pair(&p, &carried);
    let next = p.validate_ack(&ack(&rows)).unwrap();
    fresh.sections.retain(|r| r.id() != id);
    fresh.sections.extend(rows);
    let pending = carry_replacement_provenance(&source, &fresh).unwrap();
    assert!(Inventory::capture(&pending).is_err());
    let done = next.certify_readback(&pending).unwrap();
    assert_eq!(done.section("neighbor"), source.section("neighbor"));
    assert!(can_replace_main_canvas(
        &done,
        next.replacement_section(),
        100
    ));
}

#[test]
fn witnessed_metadata_does_not_relax_structural_or_history_guards() {
    let (_, readback, _) = captured();
    let old = readback.section("neighbor").unwrap();
    for field in [
        n(9, 1),
        n(10, 1),
        n(11, 1),
        n(29, 1),
        m(13, &raw(&[t(1, "parent")])),
        m(15, &raw(&[])),
        n(35, 1),
        n(36, 1),
        n(39, 1),
        t(21, "other-path"),
        n(25, 1),
        m(16, &raw(&[n(29, 1)])),
    ] {
        assert!(split::witnessed_independent_history(&changed(old, field), 0).is_err());
    }
    let wire = old.crdt().unwrap().unwrap();
    let mut doc = Automerge::load(wire.bytes(5).unwrap().unwrap())
        .unwrap()
        .with_actor("0123456789ab00000000".parse().unwrap());
    let mut tx = doc.transaction();
    tx.put(automerge::ROOT, "deleted", true).unwrap();
    tx.put(automerge::ROOT, "unknown", true).unwrap();
    tx.commit();
    let mut processed = wire
        .fields()
        .iter()
        .filter(|f| f.tag() == 6)
        .map(|f| {
            let value = RawMessage::decode(wire.encoded_field(f).unwrap()).unwrap();
            t(6, value.string(6).unwrap().unwrap())
        })
        .collect::<Vec<_>>();
    processed.push(t(6, &doc.get_heads()[0].to_string()));
    let wire = wire
        .replace_fields(5, &[F::bytes(5, &doc.save()).unwrap()])
        .unwrap()
        .replace_fields(6, &processed)
        .unwrap();
    assert!(split::witnessed_independent_history(&changed(old, m(38, &wire)), 0).is_err());
}
