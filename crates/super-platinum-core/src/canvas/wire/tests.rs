use super::*;
fn message(fields: Vec<EncodedField>) -> RawMessage {
    RawMessage::from_fields(&fields).unwrap()
}
fn uint(tag: u32, value: u64) -> EncodedField {
    EncodedField::uint(tag, value).unwrap()
}
fn bytes(tag: u32, value: &[u8]) -> EncodedField {
    EncodedField::bytes(tag, value).unwrap()
}
fn child(tag: u32, value: &RawMessage) -> EncodedField {
    EncodedField::message(tag, value).unwrap()
}
fn section(id: &str, sequence: u64, checked: Option<bool>) -> Section {
    let mut fields = vec![
        bytes(1, id.as_bytes()),
        uint(2, sequence),
        bytes(6, b"THREAD"),
        bytes(7, b"DOC"),
    ];
    if let Some(checked) = checked {
        fields.push(child(
            16,
            &message(vec![uint(2, u64::from(checked)), uint(1, 2)]),
        ));
    }
    Section::from_raw(message(fields)).unwrap()
}
fn document(sections: &[Section]) -> DocumentData {
    let mut fields = vec![bytes(15, b"DOC"), bytes(77, b"opaque document metadata")];
    fields.extend(sections.iter().map(|section| child(1, section.raw())));
    DocumentData::from_raw(message(fields)).unwrap()
}

#[test]
fn unknown_fields_and_noncanonical_encodings_round_trip_exactly() {
    let mut raw = vec![0x08, 0x81, 0x00]; // Known tag with legal nonminimal varint.
    raw.extend_from_slice(&[0xa1, 0x06, 1, 2, 3, 4, 5, 6, 7, 8]); // Unknown fixed64 tag100.
    raw.extend_from_slice(&[0xad, 0x06, 1, 2, 3, 4]); // Unknown fixed32 tag101.
    raw.extend_from_slice(bytes(102, b"private opaque bytes").raw());
    let parsed = RawMessage::decode(&raw).unwrap();
    assert_eq!(parsed.raw(), raw);
    assert_eq!(parsed.uint(1), Ok(Some(1)));
    let updated = parsed.replace_fields(1, &[uint(1, 2)]).unwrap();
    assert_eq!(&updated.raw()[..raw.len() - 3], &raw[3..]);
    assert_eq!(updated.uint(1), Ok(Some(2)));
    assert!(!format!("{parsed:?}").contains("private"));
    assert_eq!(parsed.replace_fields(1, &[uint(2, 1)]), Err(Error::Tag));
}
#[test]
fn exact_integer_and_empty_present_values_are_not_defaults_or_floats() {
    let raw = message(vec![uint(1, u64::MAX), bytes(2, b""), uint(3, 0)]);
    assert_eq!(raw.uint(1), Ok(Some(u64::MAX)));
    assert_eq!(raw.bytes(2), Ok(Some(&[][..])));
    assert_eq!(raw.boolean(3), Ok(Some(false)));
    assert_eq!(raw.boolean(4), Ok(None));
    assert_eq!(message(vec![uint(1, 2)]).boolean(1), Err(Error::Boolean));
}
#[test]
fn malformed_and_oversized_inputs_fail_without_partial_success() {
    for raw in [&[0][..], &[0x80], &[0x0a, 5, 1], &[9, 1], &[13, 1]] {
        assert!(RawMessage::decode(raw).is_err());
    }
    assert_eq!(RawMessage::decode(&[0x0b]), Err(Error::WireType));
    let mut overflow = vec![8];
    overflow.extend_from_slice(&[0xff; 9]);
    overflow.push(2);
    assert_eq!(RawMessage::decode(&overflow), Err(Error::Varint));
    assert_eq!(
        RawMessage::decode(&vec![0; MAX_BYTES + 1]),
        Err(Error::TooLarge)
    );
    assert_eq!(
        RawMessage::decode(&[8, 0].repeat(MAX_FIELDS + 1)),
        Err(Error::TooManyFields)
    );
    assert_eq!(EncodedField::uint(0, 0).err(), Some(Error::Tag));
    assert_eq!(
        EncodedField::bytes(1, &vec![0; MAX_BYTES]).err(),
        Some(Error::TooLarge)
    );
    assert_eq!(message(vec![bytes(1, &[0xff])]).string(1), Err(Error::Utf8));
}
#[test]
fn singular_duplicates_and_wrong_wire_types_are_rejected() {
    let duplicate = message(vec![uint(1, 1), uint(1, 2)]);
    assert_eq!(duplicate.uint(1), Err(Error::Duplicate));
    assert_eq!(
        message(vec![bytes(1, b"one")]).uint(1),
        Err(Error::WrongType)
    );
    let invalid = message(vec![bytes(1, b"SECTION"), uint(16, 1)]);
    assert_eq!(Section::from_raw(invalid), Err(Error::WrongType));
    let invalid = message(vec![
        bytes(1, b"SECTION"),
        child(16, &message(vec![uint(2, 1), uint(2, 0)])),
    ]);
    assert_eq!(Section::from_raw(invalid), Err(Error::Duplicate));
    assert_eq!(Section::decode(&[]), Err(Error::Identity));
}
#[test]
fn canonical_partial_merge_keeps_unrelated_sections_content_and_unknown_fields() {
    let original = section("A", 10, Some(false));
    let original = Section::from_raw(
        original
            .raw()
            .merge_partial(&message(vec![
                child(12, &message(vec![bytes(1, b"Synthetic content")])),
                bytes(123, b"future metadata"),
            ]))
            .unwrap(),
    )
    .unwrap();
    let other = section("B", 9, Some(true));
    let before = document(&[original.clone(), other.clone()]);
    let partial = Section::from_raw(message(vec![
        bytes(1, b"A"),
        uint(2, 11),
        child(16, &message(vec![uint(2, 1)])),
    ]))
    .unwrap();
    let updated = before
        .merge_partial(&DocumentData::from_raw(message(vec![child(1, partial.raw())])).unwrap())
        .unwrap();
    assert_eq!(updated.sections().len(), 2);
    assert_eq!(updated.sections()[0].checked(), Ok(Some(true)));
    assert_eq!(updated.sections()[0].sequence(), Ok(Some(11)));
    assert_eq!(
        updated.sections()[0].raw().bytes(123),
        Ok(Some(&b"future metadata"[..]))
    );
    assert_eq!(
        updated.sections()[0].raw().bytes(12),
        original.raw().bytes(12)
    );
    assert_eq!(updated.sections()[1].raw().raw(), other.raw().raw());
    assert_eq!(updated.raw().bytes(77), before.raw().bytes(77));
    // Present nested message replaces that field; no guessed recursive merge.
    assert_eq!(
        updated.sections()[0].attributes().unwrap().unwrap().uint(1),
        Ok(None)
    );
    let cleared = Section::from_raw(message(vec![
        bytes(1, b"A"),
        child(16, &message(vec![])),
        uint(3, 1),
    ]))
    .unwrap();
    let result = updated.sections()[0].merge_partial(&cleared).unwrap();
    assert_eq!(result.checked(), Ok(None));
    assert_eq!(result.deleted(), Ok(Some(true)));
    assert!(result.raw().has(16));
}
#[test]
fn merges_reject_identity_substitution_and_duplicate_collection_members() {
    let a = section("A", 1, None);
    let b = section("B", 1, None);
    assert_eq!(a.merge_partial(&b), Err(Error::IdentityChanged));
    let foreign = Section::from_raw(message(vec![bytes(1, b"A"), bytes(7, b"OTHER")])).unwrap();
    assert_eq!(a.merge_partial(&foreign), Err(Error::IdentityChanged));
    assert_eq!(
        DocumentData::from_raw(message(vec![child(1, a.raw()), child(1, a.raw())])),
        Err(Error::Duplicate)
    );
    let foreign = DocumentData::from_raw(message(vec![bytes(15, b"OTHER")])).unwrap();
    assert_eq!(
        document(&[a]).merge_partial(&foreign),
        Err(Error::IdentityChanged)
    );
}
#[test]
fn acknowledgement_and_authoritative_fetch_decode_the_same_section() {
    let target = section("A", 12000, Some(true));
    let doc = document(&[target.clone()]);
    let raw = message(vec![
        uint(1, 1),
        child(2, doc.raw()),
        uint(6, 0),
        bytes(90, b"future acknowledgement"),
    ]);
    let ack = EditAck::decode(raw.raw()).unwrap();
    assert_eq!(ack.changed, Some(true));
    assert_eq!(ack.is_new_edit_session, Some(false));
    assert_eq!(ack.final_data.as_ref().unwrap().sections()[0].id(), "A");
    assert_eq!(
        ack.final_data.as_ref().unwrap().sections()[0].checked(),
        Ok(Some(true))
    );
    assert_eq!(ack.raw().raw(), raw.raw());
    let payload = message(vec![child(7, target.raw())]);
    let changes = message(vec![child(7, &payload)]);
    let fetch = FetchLatest::decode(message(vec![child(1, &changes)]).raw()).unwrap();
    assert_eq!(fetch.sections().unwrap()[0].raw().raw(), target.raw().raw());
    // Missing or false changed is distinct from a success receipt.
    assert_eq!(EditAck::decode(&[]).unwrap().changed, None);
    assert_eq!(
        EditAck::decode(message(vec![uint(1, 0)]).raw())
            .unwrap()
            .changed,
        Some(false)
    );
    assert!(EditAck::decode(message(vec![uint(1, 1), bytes(2, &[0x0a, 0xff])]).raw()).is_err());
}
#[test]
fn loader_preserves_metadata_and_refuses_incomplete_preserialized_projection() {
    let target = section("A", 1, Some(false));
    let payload = message(vec![
        child(7, target.raw()),
        child(3, &message(vec![bytes(1, b"THREAD")])),
    ]);
    let response = message(vec![uint(1, 1), child(2, &payload)]);
    let wrapper = message(vec![child(2, &response), child(3, &message(vec![]))]);
    let load = LoadDataResponse::decode(wrapper.raw()).unwrap();
    assert_eq!(load.success(), Ok(Some(true)));
    assert_eq!(load.sections().unwrap()[0].id(), "A");
    assert_eq!(
        load.payload().unwrap().unwrap().messages(3).unwrap()[0].string(1),
        Ok(Some("THREAD"))
    );
    let wrapper = wrapper
        .replace_fields(
            3,
            &[child(3, &message(vec![bytes(1, b"opaque packed items")]))],
        )
        .unwrap();
    let load = LoadDataResponse::decode(wrapper.raw()).unwrap();
    assert!(load.payload().is_err());
    assert!(load.preserialized().is_some());
    let error =
        LoadDataResponse::decode(message(vec![bytes(1, b"Synthetic server error")]).raw()).unwrap();
    assert_eq!(error.error(), Ok(Some("Synthetic server error")));
    assert!(!format!("{error:?}").contains("Synthetic server error"));
}

#[test]
fn repeated_integers_decode_mixed_packed_and_unpacked_with_bounds() {
    let raw = message(vec![
        uint(23, 0),
        bytes(23, &[13, 0xac, 2]),
        uint(23, u64::MAX),
        bytes(23, &[]),
    ]);
    assert_eq!(raw.uints(23), Ok(vec![0, 13, 300, u64::MAX]));
    assert_eq!(raw.uints(22), Ok(vec![]));
    assert_eq!(raw.uint(23), Err(Error::Duplicate));
    assert_eq!(
        message(vec![bytes(23, &[0x80])]).uints(23),
        Err(Error::Truncated)
    );
    assert_eq!(
        message(vec![bytes(23, &vec![0; MAX_FIELDS + 1])]).uints(23),
        Err(Error::TooManyFields)
    );
    let mut wrong = Vec::new();
    write_varint((23 << 3) | 5, &mut wrong);
    wrong.extend_from_slice(&[0; 4]);
    assert_eq!(
        RawMessage::decode(&wrong).unwrap().uints(23),
        Err(Error::WrongType)
    );
}
