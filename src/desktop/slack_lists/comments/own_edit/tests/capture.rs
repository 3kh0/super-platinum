use super::*;

#[test]
fn a_delta_capture_cannot_replace_a_rich_mount_or_advance_its_sequence() {
    let (mut shell, binding, _) = setup();
    ready(&mut shell, &binding, "original");
    let before = session(&shell, &binding).unwrap().blocks.clone();
    let delta = PlainDelta::capture(vec![json!({"insert":"new plain draft\n"})]).unwrap();
    assert!(
        retain_content(
            &mut shell,
            &binding,
            "original",
            2,
            Content::PlainDelta(delta),
            false,
            false,
            false
        )
        .is_err()
    );
    let s = session(&shell, &binding).unwrap();
    assert_eq!(s.native_sequence, 1);
    assert_eq!(s.blocks, before);
    assert!(s.plain.is_none());
}

#[test]
fn revision_exhaustion_leaves_the_entire_capture_unchanged() {
    let (mut shell, binding, _) = setup();
    ready(&mut shell, &binding, "original");
    session_mut(&mut shell, &binding).unwrap().revision = u64::MAX;
    let before = session(&shell, &binding).unwrap().blocks.clone();
    assert!(
        retain_content(
            &mut shell,
            &binding,
            "original",
            2,
            Content::RichBlocks(rich("changed")),
            true,
            true,
            false
        )
        .is_err()
    );
    let s = session(&shell, &binding).unwrap();
    assert_eq!(s.native_sequence, 1);
    assert_eq!(s.blocks, before);
    assert!(!s.composing && !s.native_pending);
}

#[test]
fn pending_native_work_retains_content_but_blocks_save() {
    let (mut shell, binding, _) = setup();
    ready(&mut shell, &binding, "original");
    let blocks = session(&shell, &binding).unwrap().blocks.clone();
    assert!(
        retain_content(
            &mut shell,
            &binding,
            "original",
            2,
            Content::RichBlocks(blocks.clone()),
            false,
            true,
            false
        )
        .is_ok()
    );
    assert!(!presentation(&shell).unwrap().can_save);
    assert!(
        retain_content(
            &mut shell,
            &binding,
            "original",
            3,
            Content::RichBlocks(blocks),
            false,
            false,
            false
        )
        .is_ok()
    );
    assert!(presentation(&shell).unwrap().can_save);
}
