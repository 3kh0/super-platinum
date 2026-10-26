use super::*;
#[test]
fn two_pairs_retain_both_split_histories_and_prior_ordinary_tombstone() {
    let initial = seed();
    let mut source = initial.clone();
    let mut target = "neighbor".to_owned();
    for expected_count in [5, 6] {
        let intent = ReplacementIntent::main_canvas(&source, &target, 100).unwrap();
        let replacement = intent.replacement_section().to_owned();
        let p = intent.prepare(&source, 100).unwrap();
        let (ack, fresh, _) = pair::response(p.document_data(), &source, false);
        let a = p.validate_ack(&ack).unwrap();
        assert!(a.confirms_readback(&fresh).unwrap());
        assert_eq!(fresh.sections.len(), expected_count);
        for row in source.sections.iter().filter(|s| s.id() != target) {
            assert_eq!(fresh.section(row.id()), Some(row));
        }
        assert_eq!(
            fresh.section(&target).unwrap().deleted().unwrap(),
            Some(true)
        );
        for variant in ["missing", "mutated"] {
            assert!(
                !a.confirms_readback(&pair::preflight_source(&fresh, variant))
                    .unwrap_or(false)
            );
        }
        owner_first::canonical(&fresh, &replacement, "").unwrap();
        source = Arc::new(a.certify_readback(&fresh).unwrap());
        target = replacement;
        if expected_count == 5 {
            let mut session = super_platinum_core::canvas::edit::EditSession::new();
            let edit = TextIntent::capture(&source, &target, 100)
                .unwrap()
                .prepare(
                    &source,
                    &mut session,
                    &TextDraft::from_rtml("Replacement <b>edited</b>").unwrap(),
                    100,
                )
                .unwrap();
            let (accepted, edited, _) = completion::text(edit, source.clone());
            assert!(accepted.confirms_readback(&edited).unwrap());
            owner_first::unchanged(&source, &edited, &target).unwrap();
            source = edited;
        }
    }
    owner_first::canonical(&source, &target, "").unwrap();
    assert_eq!(source.section("paragraph"), initial.section("paragraph"));
}
#[test]
fn retained_endpoint_changes_refuse_preparation_and_partial_ack_never_seals_pair() {
    let source = seed();
    for variant in ["missing", "mutated"] {
        let intent = ReplacementIntent::main_canvas(&source, "neighbor", 100).unwrap();
        assert!(
            intent
                .prepare(&pair::preflight_source(&source, variant), 100)
                .is_err()
        );
    }
    let p = ReplacementIntent::main_canvas(&source, "neighbor", 100)
        .unwrap()
        .prepare(&source, 100)
        .unwrap();
    let (ack, fresh, _) = pair::response(p.document_data(), &source, true);
    assert!(p.validate_ack(&ack).is_err());
    assert_eq!(fresh.sections.len(), 5);
    assert_eq!(fresh.section("paragraph"), source.section("paragraph"));
}
