//! Synthetic canonical adapter around real public split/ACK/readback proofs.
use super::*;
use super_platinum_core::canvas::edit::structure::{SplitIntent, SplitWitness};
pub(super) fn seed(corpus: &str, mode: &str) -> Arc<CanvasCollabSnapshot> {
    seed_variant(corpus, mode, false)
}
pub(super) fn seed_variant(corpus: &str, mode: &str, coalesced: bool) -> Arc<CanvasCollabSnapshot> {
    seed_cadence(corpus, mode, if coalesced { "revert" } else { "none" })
}
pub(super) fn seed_cadence(corpus: &str, mode: &str, cadence: &str) -> Arc<CanvasCollabSnapshot> {
    seed_reports(corpus, mode, cadence, None)
}
pub(super) fn seed_parent(corpus: &str, mode: &str, cadence: &str) -> Arc<CanvasCollabSnapshot> {
    seed_reports(corpus, mode, "none", Some(cadence))
}
pub(super) fn seed_child_delete(
    corpus: &str,
    mode: &str,
    variant: &str,
) -> Arc<CanvasCollabSnapshot> {
    match variant {
        "plain" => seed_reports(corpus, mode, "none", None),
        "revert" => seed_reports(corpus, mode, "none", Some("revert")),
        "rich-gap" => seed_reports(corpus, mode, "revert", Some("correction")),
        _ => panic!("Unsupported child deletion seed"),
    }
}
fn seed_reports(
    corpus: &str,
    mode: &str,
    cadence: &str,
    parent_cadence: Option<&str>,
) -> Arc<CanvasCollabSnapshot> {
    let (left, tail) = texts(corpus);
    let parent_reports = match parent_cadence {
        None => vec![],
        Some("correction") => vec![
            format!("{left} draftx"),
            format!("{left} draft"),
            format!("{left} revised"),
        ],
        Some("revert") => vec![format!("{left} draftx"), left.to_owned()],
        Some("erase") => vec![String::new()],
        _ => panic!("Unsupported parent fixture cadence"),
    };
    let parent_text = parent_reports.last().map(String::as_str).unwrap_or(left);
    let child_text = if cadence == "typed" {
        format!("{tail} typed")
    } else {
        tail.to_owned()
    };
    let source = format!("{left}{tail}");
    let parent = parent(mode);
    let mut initial = (*structure_fixture::snapshot()).clone();
    let row = initial
        .sections
        .iter_mut()
        .find(|s| s.id() == "paragraph")
        .unwrap();
    let mut raw = row
        .raw()
        .replace_fields(1, &[t(1, parent)])
        .unwrap()
        .replace_fields(12, &[m(12, &body(&source, true))])
        .unwrap()
        .remove(38)
        .unwrap();
    if mode != "bootstrap" {
        let seeds: Value =
            serde_json::from_str(include_str!("../split_fixture/premiddle-original.json")).unwrap();
        let seed = seeds["seeds"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["parentMode"] == mode && s["sourceRtml"] == escaped(&source))
            .unwrap();
        let wire = RawMessage::decode(
            &STANDARD
                .decode(seed["canonicalSourceSessionBase64"].as_str().unwrap())
                .unwrap(),
        )
        .unwrap()
        .replace_fields(1, &[t(1, parent)])
        .unwrap()
        .replace_fields(4, &[n(4, 8_000_000_000_000_000)])
        .unwrap();
        raw = raw.replace_fields(38, &[m(38, &wire)]).unwrap();
    }
    *row = Section::from_raw(raw).unwrap();
    let before = TextDraft::from_rtml(&escaped(&source)).unwrap();
    let w = SplitWitness::new(
        &before,
        left.encode_utf16().count(),
        &TextDraft::from_rtml(&escaped(left)).unwrap(),
        &TextDraft::from_rtml(&escaped(tail)).unwrap(),
    )
    .unwrap();
    let w = if parent_reports.is_empty() {
        w
    } else {
        let reports = parent_reports
            .iter()
            .map(|v| TextDraft::from_rtml(&escaped(v)).unwrap())
            .collect::<Vec<_>>();
        w.with_parent_updates(&reports).unwrap()
    };
    let w = match cadence {
        "none" => w,
        "typed" => w
            .with_child_updates(&[TextDraft::from_rtml(&escaped(&child_text)).unwrap()])
            .unwrap(),
        "revert" => w
            .with_child_updates(&[
                TextDraft::from_rtml(&escaped(&format!("{tail} typed"))).unwrap(),
                TextDraft::from_rtml(&escaped(tail)).unwrap(),
            ])
            .unwrap(),
        _ => panic!("Unsupported saved-pair fixture cadence"),
    };
    let now = checklist::save::now_usec().unwrap();
    let prepared = SplitIntent::capture(&initial, parent, now)
        .unwrap()
        .prepare(&initial, &w, now)
        .unwrap();
    let child = prepared.child_section().to_owned();
    let rows = prepared
        .document_data()
        .messages(1)
        .unwrap()
        .into_iter()
        .map(|update| {
            let id = update.string(1).unwrap().unwrap();
            let owner = id == parent;
            let raw = initial.section(id).map_or_else(
                || update.clone(),
                |s| s.raw().merge_partial(&update).unwrap(),
            );
            let seq = if owner { 2000 } else { 1000 };
            let pos = raw
                .string(8)
                .unwrap()
                .unwrap()
                .trim_end_matches(":temp")
                .to_owned();
            let wire = raw
                .message(38)
                .unwrap()
                .unwrap()
                .remove(12)
                .unwrap()
                .replace_fields(3, &[n(3, seq)])
                .unwrap()
                .replace_fields(4, &[n(4, 8_000_000_000_000_000)])
                .unwrap();
            let mut result = raw
                .replace_fields(2, &[n(2, seq)])
                .unwrap()
                .replace_fields(6, &[t(6, initial.thread_id())])
                .unwrap()
                .replace_fields(7, &[t(7, initial.document_id())])
                .unwrap()
                .replace_fields(
                    12,
                    &[m(
                        12,
                        &body(if owner { parent_text } else { &child_text }, owner),
                    )],
                )
                .unwrap()
                .replace_fields(38, &[m(38, &wire)])
                .unwrap();
            if !owner {
                result = result
                    .replace_fields(8, &[t(8, &pos)])
                    .unwrap()
                    .replace_fields(21, &[t(21, &pos)])
                    .unwrap();
            }
            result
        })
        .collect::<Vec<_>>();
    let mut fields = vec![t(15, initial.document_id())];
    fields.extend(rows.iter().map(|r| m(1, r)));
    let ack = EditAck::decode(msg(&[n(1, 1), m(2, &msg(&fields))]).raw()).unwrap();
    let accepted = prepared.validate_ack(&ack).unwrap();
    let mut fresh = initial.clone();
    fresh.sections.retain(|s| s.id() != parent);
    fresh
        .sections
        .extend(rows.into_iter().map(|r| Section::from_raw(r).unwrap()));
    assert!(accepted.confirms_readback(&fresh).unwrap());
    assert!(fresh.section(&child).is_some());
    Arc::new(fresh)
}
pub(super) fn child(s: &CanvasCollabSnapshot, owner: &str) -> String {
    s.section(owner)
        .unwrap()
        .crdt()
        .unwrap()
        .unwrap()
        .messages(10)
        .unwrap()[0]
        .string(3)
        .unwrap()
        .unwrap()
        .to_owned()
}
pub(super) fn saved(s: &CanvasCollabSnapshot) -> Value {
    json!({"sections":s.sections.iter().map(|s|STANDARD.encode(s.raw().raw())).collect::<Vec<_>>()})
}
pub(super) fn restored(v: &Value) -> Arc<CanvasCollabSnapshot> {
    let mut s = (*structure_fixture::snapshot()).clone();
    s.sections = v["sections"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| Section::decode(&STANDARD.decode(v.as_str().unwrap()).unwrap()).unwrap())
        .collect();
    Arc::new(s)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn saved_pair_delete_and_child_continuation() {
        for corpus in ["ascii", "unicode"] {
            for mode in ["bootstrap", "retained", "retained-zero"] {
                let source = seed(corpus, mode);
                let owner = parent(mode);
                let id = child(&source, owner);
                let p = super_platinum_core::canvas::edit::structure::StructureIntent::delete(
                    &source, owner, 100,
                )
                .unwrap()
                .prepare(&source, 100)
                .unwrap();
                assert_eq!(
                    p.checkpoint()
                        .unwrap()
                        .split_owner_child()
                        .unwrap()
                        .as_ref(),
                    source.section(&id)
                );
                let (a, fresh, _) = completion::delete(p, source.clone());
                assert!(a.confirms_readback(&fresh).unwrap());
                assert_eq!(fresh.section(&id), source.section(&id));
                assert_eq!(fresh.section(owner).unwrap().deleted().unwrap(), Some(true));
                let actions = super_platinum_core::canvas::edit::structure::paragraph_actions(
                    &fresh,
                    &[&id],
                    100,
                );
                let act = actions[0].as_ref().unwrap();
                assert!(act.insert_before && act.insert_after && act.delete);
                let intent = TextIntent::capture(&fresh, &id, 100).unwrap();
                let mut session = super_platinum_core::canvas::edit::EditSession::new();
                let p = intent
                    .prepare(
                        &fresh,
                        &mut session,
                        &TextDraft::from_rtml("Independent child edit").unwrap(),
                        100,
                    )
                    .unwrap();
                let (a, edited, _) = completion::text(p, fresh.clone());
                assert!(a.confirms_readback(&edited).unwrap());
                assert_eq!(edited.section(owner), fresh.section(owner));
            }
        }
    }
    #[test]
    fn coalesced_exact_revert_then_delete_and_bold_child() {
        for (corpus, mode) in [("ascii", "bootstrap"), ("unicode", "retained-zero")] {
            let source = seed_variant(corpus, mode, true);
            let owner = parent(mode);
            let id = child(&source, owner);
            let deletion = super_platinum_core::canvas::edit::structure::StructureIntent::delete(
                &source, owner, 100,
            )
            .unwrap()
            .prepare(&source, 100)
            .unwrap();
            let (accepted, fresh, _) = completion::delete(deletion, source.clone());
            assert!(accepted.confirms_readback(&fresh).unwrap());
            assert_eq!(fresh.section(&id), source.section(&id));
            let desired =
                TextDraft::from_rtml(&format!("<b>{} edited</b>", escaped(texts(corpus).1)))
                    .unwrap();
            let intent = TextIntent::capture(&fresh, &id, 100).unwrap();
            let mut session = super_platinum_core::canvas::edit::EditSession::new();
            let prepared = intent.prepare(&fresh, &mut session, &desired, 100).unwrap();
            let (accepted, edited, _) = completion::text(prepared, fresh.clone());
            assert!(accepted.confirms_readback(&edited).unwrap());
            assert_eq!(edited.section(owner), fresh.section(owner));
            let actions = super_platinum_core::canvas::edit::structure::paragraph_actions(
                &edited,
                &[&id],
                100,
            );
            let actions = actions[0].as_ref().unwrap();
            assert!(actions.insert_before && actions.insert_after && actions.delete);
            assert!(TextIntent::capture(&edited, &id, 100).is_ok());
        }
    }
}
