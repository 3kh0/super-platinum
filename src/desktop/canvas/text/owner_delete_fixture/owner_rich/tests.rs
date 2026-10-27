//! Exercise the actual native fixture's synthetic canonical adapter, not a parallel seed.
use super::*;
fn edit(
    source: &Arc<CanvasCollabSnapshot>,
    id: &str,
    rtml: &str,
    owns_peer: bool,
) -> Arc<CanvasCollabSnapshot> {
    let mut session = super_platinum_core::canvas::edit::EditSession::new();
    let prepared = TextIntent::capture(source, id, 100)
        .unwrap()
        .prepare(
            source,
            &mut session,
            &TextDraft::from_rtml(rtml).unwrap(),
            100,
        )
        .unwrap();
    let (accepted, fresh, _) = completion::text(prepared, source.clone());
    assert_eq!(accepted.associated_peer_count() > 0, owns_peer, "{id}");
    assert!(accepted.confirms_readback(&fresh).unwrap());
    unchanged(source, &fresh, id).unwrap();
    canonical(&fresh, id, rtml).unwrap();
    fresh
}
#[test]
fn actual_parent_seed_rich_apply_remove_and_continuation_preserve_child_and_gap() {
    for (corpus, mode, cadence, gap) in [
        ("ascii", "bootstrap", "correction", false),
        ("unicode", "retained", "revert", true),
        ("ascii", "retained-zero", "erase", true),
    ] {
        let owner = parent(mode);
        let initial = seed::seed_parent(corpus, mode, cadence);
        let child = seed::child(&initial, owner);
        let (left, tail) = texts(corpus);
        let plain = match cadence {
            "correction" => format!("{left} revised"),
            "erase" => String::new(),
            _ => left.into(),
        };
        canonical(&initial, owner, &escaped(&plain)).unwrap();
        canonical(&initial, &child, &escaped(tail)).unwrap();
        let source = if gap {
            add_gap(initial.clone(), owner, "Independent gap")
        } else {
            initial.clone()
        };
        assert_eq!(source.section(owner), initial.section(owner));
        assert_eq!(source.section(&child), initial.section(&child));
        let plain = if plain.is_empty() {
            "Restored owner".to_owned()
        } else {
            plain
        };
        let rtml = escaped(&plain);
        let rich = if corpus == "ascii" {
            format!("<b>{rtml}</b>")
        } else {
            format!("<a href=\"https://example.test/\">{rtml}</a>")
        };
        let formatted = edit(&source, owner, &rich, true);
        let actions = super_platinum_core::canvas::edit::structure::paragraph_actions(
            &formatted,
            &[owner, &child],
            100,
        );
        assert!(actions.iter().all(|a| {
            a.as_ref()
                .is_some_and(|a| a.insert_before && a.insert_after && a.delete)
        }));
        let removed = edit(&formatted, owner, &rtml, true);
        let owner_later = edit(&removed, owner, &format!("{rtml} edited"), true);
        let child_later = edit(
            &owner_later,
            &child,
            &format!("{} edited", escaped(tail)),
            false,
        );
        assert_eq!(child_later.section(owner), owner_later.section(owner));
        assert_eq!(formatted.section(&child), initial.section(&child));
    }
}
#[test]
fn actual_parent_coalescence_rich_delete_retains_checkpoint_child_and_gap() {
    let owner = parent("retained");
    let initial = seed::seed_parent("ascii", "retained", "revert");
    let child = seed::child(&initial, owner);
    let source = add_gap(initial, owner, "Independent gap");
    let rich = edit(&source, owner, "<b>Keep this</b>", true);
    let prepared =
        super_platinum_core::canvas::edit::structure::StructureIntent::delete(&rich, owner, 100)
            .unwrap()
            .prepare(&rich, 100)
            .unwrap();
    assert_eq!(
        prepared
            .checkpoint()
            .unwrap()
            .split_owner_child()
            .unwrap()
            .as_ref(),
        rich.section(&child)
    );
    let (accepted, fresh, _) = completion::delete(prepared, rich.clone());
    assert!(accepted.confirms_readback(&fresh).unwrap());
    unchanged(&rich, &fresh, owner).unwrap();
    assert_eq!(fresh.section(owner).unwrap().deleted().unwrap(), Some(true));
}
