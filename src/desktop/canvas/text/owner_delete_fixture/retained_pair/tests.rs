//! Actual public deletion/checkpoint/ACK/readback and unrelated-survivor continuation.
use super::*;
use super_platinum_core::canvas::edit::structure::{StructureIntent, paragraph_actions};
#[test]
fn retained_pair_fixture_keeps_both_histories_and_maps_unrelated_survivor() {
    for (corpus, mode, variant) in [
        ("ascii", "bootstrap", "plain"),
        ("unicode", "retained", "rich-gap"),
    ] {
        let source = seed_variant(corpus, mode, variant);
        let owner = parent(mode);
        let child = seed::child(&source, owner);
        assert_eq!(
            source.section(&child).unwrap().deleted().unwrap(),
            Some(true)
        );
        let p = StructureIntent::delete(&source, owner, 100)
            .unwrap()
            .prepare(&source, 100)
            .unwrap();
        let checkpoint = p.checkpoint().unwrap();
        assert_eq!(
            checkpoint.split_owner_child().unwrap().as_ref(),
            source.section(&child)
        );
        let (accepted, fresh, _) = completion::delete(p, source.clone());
        assert!(accepted.confirms_readback(&fresh).unwrap());
        unchanged(&source, &fresh, owner).unwrap();
        assert_eq!(fresh.section(owner).unwrap().deleted().unwrap(), Some(true));
        assert_eq!(fresh.section(&child), source.section(&child));
        for missing in [owner, child.as_str()] {
            let mut omitted = (*fresh).clone();
            omitted.sections.retain(|s| s.id() != missing);
            assert!(!accepted.confirms_readback(&omitted).unwrap_or(false));
        }
        let actions = paragraph_actions(&fresh, &[owner, child.as_str(), "neighbor"], 100);
        assert!(actions[0].is_none() && actions[1].is_none());
        let remaining = actions[2].as_ref().unwrap();
        assert!(remaining.insert_before && remaining.insert_after);
        assert_eq!(remaining.delete, variant == "rich-gap");
        assert!(StructureIntent::delete(&fresh, owner, 100).is_err());
        assert!(StructureIntent::delete(&fresh, &child, 100).is_err());
        let intent = TextIntent::capture(&fresh, "neighbor", 100).unwrap();
        assert_eq!(intent.associated_peer_count(), 0);
        let mut session = super_platinum_core::canvas::edit::EditSession::new();
        let p = intent
            .prepare(
                &fresh,
                &mut session,
                &TextDraft::from_rtml("Another paragraph edited").unwrap(),
                100,
            )
            .unwrap();
        let (accepted, edited, _) = completion::text(p, fresh.clone());
        assert!(accepted.confirms_readback(&edited).unwrap());
        unchanged(&fresh, &edited, "neighbor").unwrap();
        canonical(&edited, "neighbor", "Another paragraph edited").unwrap();
        assert_eq!(edited.section(owner), fresh.section(owner));
        assert_eq!(edited.section(&child), fresh.section(&child));
    }
}
