use super::*;
use serde_json::json;

fn prepared(shell: &ShellState, binding: &Binding) -> Departure {
    let e = entry(shell, binding.id()).unwrap();
    Departure {
        generation: binding.owner.serial,
        lease: None,
        saved: Some(Saved {
            binding: binding.clone(),
            blocks: e.blocks.clone(),
            capture: e.capture.clone(),
            sequence: e.sequence,
            mount: None,
            unread_empty: e.inventory.is_none() && e.blocks.is_empty(),
        }),
    }
}

#[test]
fn failed_route_drop_retains_original_binding_saved_bytes_and_generation() {
    let (shell, binding, _local) = super::super::tests::setup();
    let departure = prepared(&shell, &binding);
    assert!(departure.current(&shell));
    let bytes = entry(&shell, binding.id()).unwrap().blocks.clone();
    drop(departure);
    assert!(binding.current(&shell));
    assert_eq!(entry(&shell, binding.id()).unwrap().blocks, bytes);
    assert_eq!(
        entry(&shell, binding.id()).unwrap().generation,
        binding.generation()
    );
}

#[test]
fn final_guard_refuses_newer_draft_root_busy_and_storage_failure() {
    for mutation in 0..6 {
        let (mut changed, binding, _local) = super::super::tests::setup();
        let departure = prepared(&changed, &binding);
        let e = entry_mut(&mut changed, binding.id()).unwrap();
        match mutation {
            0 => e.blocks.push(json!({"opaque":"newer native bytes"})),
            1 => e.sequence += 1,
            2 => e.busy = true,
            3 => e.composing = true,
            4 => e.inventory = None,
            _ => e.capture_failed = true,
        }
        assert!(!departure.current(&changed), "mutation {mutation}");
        drop(departure);
        assert!(binding.visible_mount(&changed));
    }
    let (mut changed, binding, _local) = super::super::tests::setup();
    let departure = prepared(&changed, &binding);
    let old = changed
        .slack_lists
        .comments
        .entry
        .as_ref()
        .unwrap()
        .target
        .clone();
    let mut record = old.record_raw().clone();
    record["thread_ts"] = json!("1800000000.000001");
    changed.slack_lists.comments.entry.as_mut().unwrap().target = old
        .refresh(
            &json!({"ok":true,"file":old.document_raw()}),
            &json!({"ok":true,"record":record}),
            false,
        )
        .unwrap();
    assert!(!departure.current(&changed));
    assert!(binding.visible_mount(&changed));
}

#[test]
fn commit_clears_only_original_binding_and_preserves_retained_journal() {
    let (mut shell, binding, _local) = super::super::tests::setup();
    let departure = prepared(&shell, &binding);
    let blocks = entry(&shell, binding.id()).unwrap().blocks.clone();
    shell.overlay = Some(Overlay::Canvas);
    departure.committed(&mut shell);
    assert!(shell.slack_lists.comments.drafts.active.is_none());
    assert_eq!(
        entry(&shell, binding.id())
            .unwrap()
            .inventory
            .as_ref()
            .unwrap()
            .blocks(),
        blocks
    );
    let (mut shell, binding, _local) = super::super::tests::setup();
    let departure = prepared(&shell, &binding);
    shell
        .slack_lists
        .comments
        .drafts
        .active
        .as_mut()
        .unwrap()
        .generation += 1;
    departure.committed(&mut shell);
    assert!(shell.slack_lists.comments.drafts.active.is_some());
}

#[test]
fn unread_empty_failed_load_can_leave_without_overwriting_journal() {
    let (mut shell, binding, _local) = super::super::tests::setup();
    let e = entry_mut(&mut shell, binding.id()).unwrap();
    e.blocks.clear();
    e.inventory = None;
    e.error = Some("Original SQLite load error".into());
    let departure = prepared(&shell, &binding);
    assert!(departure.current(&shell));
    report_refusal(&mut shell, &binding);
    assert_eq!(
        entry(&shell, binding.id()).unwrap().error.as_deref(),
        Some("Original SQLite load error")
    );
    drop(departure);
    assert!(binding.current(&shell));
}

#[test]
fn composing_and_busy_refusals_are_visible_without_rotating_draft() {
    let (mut shell, binding, _local) = super::super::tests::setup();
    let blocks = entry(&shell, binding.id()).unwrap().blocks.clone();
    entry_mut(&mut shell, binding.id()).unwrap().composing = true;
    report_refusal(&mut shell, &binding);
    assert!(
        entry(&shell, binding.id())
            .unwrap()
            .error
            .as_ref()
            .unwrap()
            .contains("Finish composing")
    );
    let e = entry_mut(&mut shell, binding.id()).unwrap();
    e.composing = false;
    e.error = None;
    e.busy = true;
    report_refusal(&mut shell, &binding);
    assert!(
        entry(&shell, binding.id())
            .unwrap()
            .error
            .as_ref()
            .unwrap()
            .contains("current comment send")
    );
    assert_eq!(entry(&shell, binding.id()).unwrap().blocks, blocks);
    assert_eq!(
        entry(&shell, binding.id()).unwrap().generation,
        binding.generation()
    );
}
