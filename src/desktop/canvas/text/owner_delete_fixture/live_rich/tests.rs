use super::*;
fn save(source: &Arc<CanvasCollabSnapshot>, id: &str, rtml: &str) -> Arc<CanvasCollabSnapshot> {
    let intent = TextIntent::capture(source, id, 100).unwrap();
    let mut session = super_platinum_core::canvas::edit::EditSession::new();
    let prepared = intent
        .prepare(
            source,
            &mut session,
            &TextDraft::from_rtml(rtml).unwrap(),
            100,
        )
        .unwrap();
    let (accepted, fresh, _) = completion::text(prepared, source.clone());
    assert!(accepted.confirms_readback(&fresh).unwrap());
    fresh
}
fn actions(source: &CanvasCollabSnapshot, owner: &str, child: &str) {
    let mapped = super_platinum_core::canvas::edit::structure::paragraph_actions(
        source,
        &[owner, child],
        100,
    );
    assert!(mapped.iter().all(|a| {
        a.as_ref()
            .is_some_and(|a| a.insert_before && a.insert_after && a.delete)
    }));
    assert!(TextIntent::capture(source, owner, 100).is_ok());
    assert!(TextIntent::capture(source, child, 100).is_ok());
}
#[test]
fn live_saved_coalesced_child_apply_remove_keeps_owner_exact_and_actions() {
    for corpus in ["ascii", "unicode"] {
        for cadence in ["typed", "revert"] {
            for format in ["bold", "link"] {
                let source = seed::seed_cadence(corpus, "retained-zero", cadence);
                let owner = parent("retained-zero");
                let child = seed::child(&source, owner);
                let tail = if cadence == "typed" {
                    format!("{} typed", texts(corpus).1)
                } else {
                    texts(corpus).1.to_owned()
                };
                let plain = escaped(&tail);
                let rich = if format == "bold" {
                    format!("<b>{plain}</b>")
                } else {
                    format!("<a href=\"https://example.test/\">{plain}</a>")
                };
                let formatted = save(&source, &child, &rich);
                immutable_except(&source, &formatted, &child).unwrap();
                canonical(&formatted, &child, &rich).unwrap();
                actions(&formatted, owner, &child);
                let removed = save(&formatted, &child, &plain);
                immutable_except(&formatted, &removed, &child).unwrap();
                canonical(&removed, &child, &plain).unwrap();
                actions(&removed, owner, &child);
            }
        }
    }
}
#[test]
fn live_rich_child_survives_restored_owner_edit_or_delete() {
    for delete in [false, true] {
        let source = seed::seed_cadence("unicode", "bootstrap", "typed");
        let owner = parent("bootstrap");
        let child = seed::child(&source, owner);
        let rich = format!("<b>{} typed</b>", escaped(texts("unicode").1));
        let formatted = save(&source, &child, &rich);
        actions(&formatted, owner, &child);
        let fresh = if delete {
            let p = super_platinum_core::canvas::edit::structure::StructureIntent::delete(
                &formatted, owner, 100,
            )
            .unwrap()
            .prepare(&formatted, 100)
            .unwrap();
            assert_eq!(
                p.checkpoint()
                    .unwrap()
                    .split_owner_child()
                    .unwrap()
                    .as_ref(),
                formatted.section(&child)
            );
            let (accepted, fresh, _) = completion::delete(p, formatted.clone());
            assert!(accepted.confirms_readback(&fresh).unwrap());
            assert_eq!(fresh.section(owner).unwrap().deleted().unwrap(), Some(true));
            fresh
        } else {
            save(
                &formatted,
                owner,
                &format!("{} edited", escaped(texts("unicode").0)),
            )
        };
        immutable_except(&formatted, &fresh, owner).unwrap();
        assert_eq!(fresh.section(&child), formatted.section(&child));
        canonical(&fresh, &child, &rich).unwrap();
    }
}
