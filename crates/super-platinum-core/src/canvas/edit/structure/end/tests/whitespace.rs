//! Canonical source whitespace clearing, independently pinned report patches.
use super::*;
use nonliteral::{attempt, processed, replace};
use serde_json::Value;
mod recovery;

fn vectors() -> Value {
    serde_json::from_str(include_str!("whitespace/original.json")).unwrap()
}

fn original_source(seed: &Value) -> (CanvasCollabSnapshot, String) {
    let mut s = fixture("Keep this", true);
    let id = "temp:C:abc0123456789abcdef012345678".to_owned();
    assert!(position::modern_id(&id));
    let bytes =
        crate::slack::api::decode_canvas_base64(seed["sessionBase64"].as_str().unwrap()).unwrap();
    let original = RawMessage::decode(&bytes).unwrap();
    // Only host ID and expiry are bound here. Base/history/heads remain the
    // actual original runtime bytes, including genuine acknowledged base0.
    let wire = original
        .replace_fields(1, &[t(1, &id)])
        .unwrap()
        .replace_fields(4, &[n(4, 1_000_000)])
        .unwrap();
    assert_eq!(wire.bytes(5).unwrap(), original.bytes(5).unwrap());
    let row = s.sections.iter_mut().find(|r| r.id() == "other").unwrap();
    for field in [
        t(1, &id),
        n(2, 1000),
        m(12, &body(seed["rtml"].as_str().unwrap(), false)),
        m(38, &wire),
    ] {
        *row = change(row, field);
    }
    (s, id)
}

fn apply_reference_patch(text: &mut Vec<u16>, patch: &Value) {
    let at = patch["path"][1].as_u64().unwrap() as usize;
    assert_eq!(patch["path"][0], "text");
    match patch["action"].as_str().unwrap() {
        "del" => {
            let count = patch["length"].as_u64().unwrap_or(1) as usize;
            text.drain(at..at + count);
        }
        "splice" => {
            text.splice(at..at, patch["value"].as_str().unwrap().encode_utf16());
        }
        action => panic!("unexpected reference patch {action}"),
    }
}

#[test]
fn original_whitespace_and_feff_histories_match_every_clear_and_typing_patch() {
    let data = vectors();
    for case in data["cases"].as_array().unwrap() {
        let seed = &data["seeds"][case["seed"].as_u64().unwrap() as usize];
        let label = format!("{}/{}/{}", seed["cohort"], seed["shape"], case["cadence"]);
        let (s, id) = original_source(seed);
        let reports = case["reports"].as_array().unwrap();
        assert_eq!(reports[0], "");
        let updates = reports[1..]
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect::<Vec<_>>();
        let p = attempt(&s, &updates);
        let source = s.section(&id).unwrap().crdt().unwrap().unwrap();
        let before = Automerge::load(source.bytes(5).unwrap().unwrap()).unwrap();
        let after = Automerge::load(p.child_wire.bytes(5).unwrap().unwrap()).unwrap();
        for c in before.get_changes(&[]) {
            assert_eq!(
                after.get_change_by_hash(&c.hash()).unwrap().raw_bytes(),
                c.raw_bytes(),
                "{label}"
            );
        }
        let patches = case["patches"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|r| r.as_array().unwrap())
            .collect::<Vec<_>>();
        let changes = after.get_changes(&before.get_heads());
        assert_eq!(changes.len(), patches.len(), "{label}");
        let mut expected = format!("{}\u{feff}", seed["rtml"].as_str().unwrap())
            .encode_utf16()
            .collect::<Vec<_>>();
        let mut previous = before.get_heads();
        let (_, object) = after.get(automerge::ROOT, "text").unwrap().unwrap();
        for (step, (c, patch)) in changes.iter().zip(patches).enumerate() {
            assert_eq!(c.deps(), previous, "{label},step={step}");
            assert_eq!(
                c.actor_id().to_string(),
                format!("{}00000000", p.session_id())
            );
            assert_eq!(c.seq(), step as u64 + 1);
            apply_reference_patch(&mut expected, patch);
            let at = after.fork_at(&[c.hash()]).unwrap();
            assert_eq!(
                at.text(&object).unwrap(),
                String::from_utf16(&expected).unwrap(),
                "{label},step={step},patch={patch}"
            );
            assert!(at.marks(&object).unwrap().is_empty());
            // Every reference patch is replayed as its own real change. This
            // checks FEFF-leading delete@1 rather than assuming delete@0.
            let actual = after.diff(&previous, &[c.hash()]);
            assert_eq!(actual.len(), 1, "{label},step={step}");
            match (&actual[0].action, patch["action"].as_str().unwrap()) {
                (automerge::PatchAction::DeleteSeq { index, length }, "del") => {
                    assert_eq!(*index as u64, patch["path"][1].as_u64().unwrap(), "{label}");
                    assert_eq!(
                        *length as u64,
                        patch["length"].as_u64().unwrap_or(1),
                        "{label}"
                    );
                }
                (
                    automerge::PatchAction::SpliceText {
                        index,
                        value,
                        marks,
                    },
                    "splice",
                ) => {
                    assert_eq!(*index as u64, patch["path"][1].as_u64().unwrap(), "{label}");
                    assert_eq!(
                        value.make_string(),
                        patch["value"].as_str().unwrap(),
                        "{label}"
                    );
                    assert!(marks.is_none());
                }
                other => panic!("{label},step={step}: {other:?}"),
            }
            previous = vec![c.hash()];
        }
        let expected_final = updates.last().copied().unwrap_or("");
        assert_eq!(p.child_draft().rtml().unwrap(), expected_final);
        let mut heads = processed(&source);
        heads.push(after.get_heads()[0].to_string());
        heads.extend(processed(&source));
        assert_eq!(processed(&p.child_wire), heads);
        for tag in [2, 3] {
            assert_eq!(p.child_wire.uint(tag).unwrap(), source.uint(tag).unwrap());
        }
        assert_eq!(p.child_wire.string(8).unwrap(), source.string(8).unwrap());
        assert_eq!(
            p.child_wire.messages(11).unwrap(),
            source.messages(11).unwrap()
        );
        assert!(!p.child_wire.has(4) && !p.child_wire.has(10));
        assert_eq!(
            p.child_wire.string(12).unwrap(),
            Some(after.get_heads()[0].to_string().as_str())
        );
        assert_eq!(
            p.parent_wire,
            validation::project_parent_session(
                &s.section("item").unwrap().crdt().unwrap().unwrap()
            )
            .unwrap()
        );
        let rows = pair(&p, &s);
        let accepted = p.validate_ack(&ack(&rows)).unwrap();
        let fresh = readback(&s, &rows);
        assert!(accepted.confirms_readback(&fresh).unwrap(), "{label}");
        assert_eq!(fresh.sections.len(), s.sections.len());
        assert_eq!(
            fresh.section(&id).unwrap().raw().string(8).unwrap(),
            s.section(&id).unwrap().raw().string(8).unwrap()
        );
        let continued = save_text(&fresh, &id, "Continued &amp; paragraph");
        assert_eq!(continued.section("item"), fresh.section("item"));
        assert_eq!(
            continued
                .section(&id)
                .unwrap()
                .crdt()
                .unwrap()
                .unwrap()
                .uint(2)
                .unwrap(),
            source.uint(2).unwrap()
        );
        let checkpoint = p.checkpoint().unwrap();
        assert_eq!(serde_json::to_value(&checkpoint).unwrap()["version"], 7);
    }
}

#[test]
fn every_canonical_whitespace_scalar_and_single_br_position_clears_on_both_bases() {
    let alphabet = [0x20, 0xa0, 0x1680]
        .into_iter()
        .chain(0x2000..=0x200b)
        .chain([0x2028, 0x2029, 0x202f, 0x205f, 0x3000, 0xfeff])
        .map(|n| char::from_u32(n).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(alphabet.len(), 21);
    let mut shapes = alphabet.iter().map(char::to_string).collect::<Vec<_>>();
    let mixed = alphabet.iter().collect::<String>();
    shapes.extend([
        mixed.clone(),
        format!("<br>{mixed}"),
        format!("{mixed}<br>"),
        format!("{mixed}<br>{mixed}"),
        "\u{feff}\u{feff}".into(),
    ]);
    for shape in shapes {
        for zero in [false, true] {
            let (s, id) = nonliteral::source(&shape, zero);
            assert_eq!(
                super::super::clearing::source_draft(s.section(&id).unwrap())
                    .unwrap()
                    .rtml()
                    .unwrap(),
                shape
            );
            let p = attempt(&s, &[]);
            assert!(p.child_draft() == &TextDraft::default());
            let rows = pair(&p, &s);
            assert!(
                p.validate_ack(&ack(&rows))
                    .unwrap()
                    .confirms_readback(&readback(&s, &rows))
                    .unwrap()
            );
            assert_eq!(
                serde_json::to_value(p.checkpoint().unwrap()).unwrap()["version"],
                7
            );
        }
    }
}

#[test]
fn source_allowance_does_not_relax_parent_typing_or_raw_grammar() {
    for raw in [
        "\u{feff}",
        "\u{feff}\u{feff}",
        " \u{feff}",
        "\u{2028}",
        "\u{2029}",
    ] {
        let d = draft(raw);
        assert_eq!(d.rtml().unwrap(), raw);
        assert!(
            EndWitness::new(&d, raw.encode_utf16().count(), &d, &TextDraft::default()).is_err()
        );
        assert!(witness("Keep this").with_child_updates(&[d]).is_err());
    }
    let (s, id) = nonliteral::source(" ", false);
    for raw in [
        "",
        "\t",
        "\n",
        "\r",
        "\u{b}",
        "\u{c}",
        "&#65279;",
        "&nbsp;",
        "<br/>",
        "<BR>",
        " <br><br> ",
        "<b> </b>",
        "\u{85}",
        "\u{200c}",
        "\u{200d}",
        "a",
    ] {
        let changed = replace(&s, &id, m(12, &body(raw, true)));
        assert!(
            super::super::clearing::source_draft(changed.section(&id).unwrap()).is_err(),
            "raw={raw:?}"
        );
    }
    // A real two-BR history must still refuse reuse rather than insert a duplicate.
    let (two, _) = nonliteral::source(" <br><br> ", false);
    assert!(EndIntent::capture(&two, "item", 100).is_err());
    let (nonempty, _) = nonliteral::source("Ordinary paragraph", false);
    let new = EndIntent::capture(&nonempty, "item", 100).unwrap();
    assert!(!new.reuses_child() && !new.clears_child());
}

#[test]
fn whitespace_clear_ack_requires_exact_body_history_language_and_fresh_source() {
    let data = vectors();
    for seed in data["seeds"].as_array().unwrap() {
        let (s, id) = original_source(seed);
        let p = attempt(&s, &[]);
        let rows = pair(&p, &s);
        assert!(p.validate_ack(&ack(&rows[..1])).is_err());
        assert!(p.validate_ack(&ack(&rows[1..])).is_err());
        let no_body = Section::from_raw(rows[1].raw().remove(12).unwrap()).unwrap();
        assert!(p.validate_ack(&ack(&[rows[0].clone(), no_body])).is_err());
        let old = s.section(&id).unwrap();
        for (label, field) in [
            ("old-body", m(12, &old.raw().message(12).unwrap().unwrap())),
            ("changed-language", m(12, &body("", true))),
            ("old-history", m(38, &old.crdt().unwrap().unwrap())),
        ] {
            assert!(
                p.validate_ack(&ack(&[rows[0].clone(), change(&rows[1], field)]))
                    .is_err(),
                "seed={},case={label}",
                seed["shape"]
            );
        }
        let accepted = p.validate_ack(&ack(&rows)).unwrap();
        let fresh = readback(&s, &rows);
        let old_body = m(12, &old.raw().message(12).unwrap().unwrap());
        assert!(
            !accepted
                .confirms_readback(&replace(&fresh, &id, old_body))
                .unwrap()
        );
        let wire = old.crdt().unwrap().unwrap();
        for changed in [
            replace(&s, &id, n(2, 2000)),
            replace(&s, &id, m(12, &body(" ", false))),
            replace(&s, &id, m(38, &wire.remove(4).unwrap())),
            replace(
                &s,
                &id,
                m(38, &wire.replace_fields(4, &[n(4, 99)]).unwrap()),
            ),
            replace(&s, "title", m(12, &body("Changed", false))),
        ] {
            assert!(
                p.validate_snapshot(&changed, 100).is_err(),
                "seed={}",
                seed["shape"]
            );
        }
        let mut ordered = s.sections.iter().collect::<Vec<_>>();
        ordered.sort_by_key(|s| s.raw().string(8).unwrap().unwrap());
        assert!(super::super::reuse::candidate(&ordered, "item").is_err());
    }
}
