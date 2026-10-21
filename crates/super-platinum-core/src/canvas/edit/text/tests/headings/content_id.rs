use super::*;

fn attrs(id: &str) -> RawMessage {
    let mut bytes = msg(&[
        t(29, id),
        n(4, 0),
        n(13, 0),
        t(177, "opaque-a"),
        t(177, "opaque-b"),
    ])
    .raw()
    .to_vec();
    // Unknown field 1001 uses a noncanonical varint: preserve bytes, not a map.
    bytes.extend_from_slice(&[0xc8, 0x3e, 0x81, 0x00]);
    RawMessage::decode(&bytes).unwrap()
}
fn with_attrs(s: &mut CanvasCollabSnapshot, attributes: &RawMessage) {
    s.sections[0] = Section::from_raw(
        s.sections[0]
            .raw()
            .replace_fields(16, &[m(16, attributes)])
            .unwrap(),
    )
    .unwrap();
}

#[test]
fn heading_content_id_all_levels_histories_copy_full_raw_attributes_and_merge_ack() {
    for level in 1..=3 {
        for base in [None, Some(0), Some(9000)] {
            for id in ["content-id", " "] {
                let mut s = heading(level, base);
                let a = attrs(id);
                with_attrs(&mut s, &a);
                let original = s.sections.clone();
                let p = prepare(&s, &mut EditSession::new());
                let update = p.document_data().message(1).unwrap().unwrap();
                assert_eq!(update.message(16).unwrap().unwrap().raw(), a.raw());
                assert_eq!(
                    update.fields().iter().map(|f| f.tag()).collect::<Vec<_>>(),
                    [1, 2, 9, 12, 13, 16, 19, 20, 33, 38]
                );
                let actual_base = update
                    .message(38)
                    .unwrap()
                    .unwrap()
                    .uint(2)
                    .unwrap()
                    .unwrap();
                // No initial history bootstraps at the original section's
                // positive sequence; only explicit Some(0) is the zero cohort.
                let expected_base = base.unwrap_or(s.sections[0].sequence().unwrap().unwrap());
                assert_eq!(
                    actual_base, expected_base,
                    "level={level} initial_base={base:?} id={id:?}"
                );
                let response = ack_raw(&p);
                assert_eq!(
                    response.message(38).unwrap().unwrap().uint(2).unwrap(),
                    Some(actual_base)
                );
                for raw in [response.clone(), response.remove(16).unwrap()] {
                    let accepted = p.validate_ack(&ack(&raw)).unwrap();
                    assert_eq!(
                        accepted.baseline.attributes().unwrap().unwrap().raw(),
                        a.raw()
                    );
                    let mut current = readback(&s, &accepted);
                    assert!(accepted.confirms_readback(&current).unwrap());
                    // A complete canonical row can carry the same outer attrs
                    // at a different position than the omitted-attrs ACK merge.
                    let attrs_front = RawMessage::decode(
                        &[
                            m(16, &a).raw(),
                            current.sections[0].raw().remove(16).unwrap().raw(),
                        ]
                        .concat(),
                    )
                    .unwrap();
                    current.sections[0] = Section::from_raw(attrs_front).unwrap();
                    assert!(accepted.confirms_readback(&current).unwrap());
                    let seq = current.sections[0].sequence().unwrap().unwrap() + 1;
                    let wire = current.sections[0]
                        .crdt()
                        .unwrap()
                        .unwrap()
                        .replace_fields(3, &[n(3, seq)])
                        .unwrap();
                    current.sections[0] = Section::from_raw(
                        current.sections[0]
                            .raw()
                            .replace_fields(2, &[n(2, seq)])
                            .unwrap()
                            .replace_fields(38, &[m(38, &wire)])
                            .unwrap(),
                    )
                    .unwrap();
                    // Existing base0 policy requires the exact ACK sequence;
                    // positive-base readback alone admits a later revision.
                    assert_eq!(
                        accepted.confirms_readback(&current).unwrap(),
                        actual_base > 0,
                        "level={level} initial_base={base:?} wire_base={actual_base} id={id:?} ack_attrs={}",
                        raw.has(16)
                    );
                    with_attrs(&mut current, &attrs("different"));
                    assert!(!accepted.confirms_readback(&current).unwrap());
                }
                for bad in [
                    attrs("different"),
                    attrs(""),
                    a.remove(177).unwrap(),
                    // Equal values in a different nested field order are not
                    // the original captured attribute bytes.
                    a.replace_fields(29, &[t(29, id)]).unwrap(),
                    msg(&[]),
                ] {
                    assert!(
                        p.validate_ack(&ack(&response.replace_fields(16, &[m(16, &bad)]).unwrap()))
                            .is_err()
                    );
                }
                assert_eq!(s.sections, original);
            }
        }
    }
}

#[test]
fn heading_content_id_malformed_duplicate_and_drift_refuse_without_new_authority() {
    let mut s = heading(2, None);
    for a in [msg(&[n(29, 1)]), msg(&[t(29, "a"), t(29, "a")])] {
        with_attrs(&mut s, &a);
        assert!(TextIntent::capture(&s, ID, 100).is_err());
    }
    for field in [n(16, 1), m(16, &attrs("a"))] {
        let duplicate = s.sections[0]
            .raw()
            .replace_fields(16, &[field.clone(), field])
            .unwrap();
        assert!(Section::from_raw(duplicate).is_err());
    }
    s = heading(2, None);
    with_attrs(&mut s, &attrs("a"));
    let intent = TextIntent::capture(&s, ID, 100).unwrap();
    let mut fresh = s.clone();
    with_attrs(&mut fresh, &attrs("b"));
    assert!(
        intent
            .prepare(&fresh, &mut EditSession::new(), &changed(), 100)
            .is_err()
    );
    // Existing paragraph projection remains unchanged, even with content_id.
    fresh.sections[0] = Section::from_raw(
        fresh.sections[0]
            .raw()
            .replace_fields(10, &[n(10, 0)])
            .unwrap(),
    )
    .unwrap();
    assert!(
        !prepare(&fresh, &mut EditSession::new())
            .document_data()
            .message(1)
            .unwrap()
            .unwrap()
            .has(16)
    );
}
