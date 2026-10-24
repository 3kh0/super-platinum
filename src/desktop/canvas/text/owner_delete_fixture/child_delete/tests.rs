//! Exercise the same public proofs and synthetic canonical adapter as the native fixture.
use super::*;
use super_platinum_core::canvas::edit::structure::{Side, StructureIntent, paragraph_actions};
fn edit(source: &Arc<CanvasCollabSnapshot>, owner: &str, rtml: &str) -> Arc<CanvasCollabSnapshot> {
    let intent = TextIntent::capture(source, owner, 100).unwrap();
    assert_eq!(intent.associated_peer_count(), 1);
    let mut session = super_platinum_core::canvas::edit::EditSession::new();
    let p = intent
        .prepare(
            source,
            &mut session,
            &TextDraft::from_rtml(rtml).unwrap(),
            100,
        )
        .unwrap();
    let (accepted, fresh, _) = completion::text(p, source.clone());
    assert_eq!(accepted.associated_peer_count(), 1);
    assert!(accepted.confirms_readback(&fresh).unwrap());
    unchanged(source, &fresh, owner).unwrap();
    canonical(&fresh, owner, rtml).unwrap();
    fresh
}
#[test]
fn native_child_delete_seeds_retain_peer_through_plain_and_rich_owner_saves() {
    for (corpus, mode, variant) in [
        ("ascii", "bootstrap", "plain"),
        ("ascii", "retained", "revert"),
        ("unicode", "retained", "rich-gap"),
    ] {
        let source = seed_variant(corpus, mode, variant);
        let owner = parent(mode);
        let child = seed::child(&source, owner);
        let p = StructureIntent::delete(&source, &child, 100)
            .unwrap()
            .prepare(&source, 100)
            .unwrap();
        let checkpoint = p.checkpoint().unwrap();
        assert!(checkpoint.split_owner_child().unwrap().is_none());
        let (accepted, deleted, _) = completion::delete(p, source.clone());
        assert!(accepted.confirms_readback(&deleted).unwrap());
        unchanged(&source, &deleted, &child).unwrap();
        assert_eq!(
            deleted.section(&child).unwrap().deleted().unwrap(),
            Some(true)
        );
        let mut omitted = (*deleted).clone();
        omitted.sections.retain(|s| s.id() != child);
        assert!(!accepted.confirms_readback(&omitted).unwrap_or(false));
        assert!(TextIntent::capture(&omitted, owner, 100).is_err());
        let actions = paragraph_actions(&deleted, &[owner, child.as_str()], 100);
        let owner_actions = actions[0].as_ref().unwrap();
        assert!(owner_actions.insert_before && owner_actions.insert_after && owner_actions.delete);
        assert!(actions[1].is_none());
        let source = if variant == "plain" {
            let p = StructureIntent::insert(
                &deleted,
                owner,
                Side::After,
                &TextDraft::from_rtml("Independent paragraph").unwrap(),
                100,
            )
            .unwrap()
            .prepare(&deleted, 100)
            .unwrap();
            let (accepted, fresh, _) = completion::delete(p, deleted.clone());
            assert!(accepted.confirms_readback(&fresh).unwrap());
            assert_eq!(fresh.sections.len(), deleted.sections.len() + 1);
            assert!(
                deleted
                    .sections
                    .iter()
                    .all(|s| fresh.section(s.id()) == Some(s))
            );
            fresh
        } else {
            deleted.clone()
        };
        let left = if variant == "rich-gap" {
            format!("{} revised", texts(corpus).0)
        } else {
            texts(corpus).0.into()
        };
        let text = escaped(&format!("{left} edited"));
        let plain = edit(&source, owner, &text);
        let rich = if corpus == "ascii" {
            format!("<b>{text}</b>")
        } else {
            format!("<a href=\"https://example.test/\">{text}</a>")
        };
        let formatted = edit(&plain, owner, &rich);
        assert_eq!(formatted.section(&child), deleted.section(&child));
        assert!(StructureIntent::delete(&formatted, owner, 100).is_ok());
    }
}
