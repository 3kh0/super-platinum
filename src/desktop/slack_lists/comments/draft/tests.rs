use super::*;
use serde_json::json;
use super_platinum_core::slack_lists::comments::{
    ReplyIntent, ReplyPolicy,
    journal::{Journal, Scope as DiskScope},
};
pub(in crate::slack_lists::comments) struct Local(std::path::PathBuf);
impl Local {
    fn new() -> Self {
        Self(std::env::temp_dir().join(format!(
            "list-comment-draft-test-{}.sqlite",
            uuid::Uuid::new_v4()
        )))
    }
}
impl Drop for Local {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
        let _ = std::fs::remove_file(self.0.with_extension("sqlite-journal"));
    }
}
pub(in crate::slack_lists::comments) fn setup() -> (ShellState, Binding, Local) {
    let mut shell = super::super::tests::shell();
    let owner = shell
        .slack_lists
        .comments
        .entry
        .as_ref()
        .unwrap()
        .owner
        .clone();
    let target = shell
        .slack_lists
        .comments
        .entry
        .as_ref()
        .unwrap()
        .target
        .clone();
    let local = Local::new();
    let disk = storage::Disk::test(
        DiskScope::capture(&owner.scope.team, &owner.scope.stable_owner().2, &target).unwrap(),
        local.0.clone(),
    );
    let mut journal = Journal::open(&local.0, &disk.account).unwrap();
    let inventory = journal.inventory(&disk.scope).unwrap();
    let capture = uuid::Uuid::new_v4().to_string();
    let blocks = vec![
        json!({"type":"rich_text","elements":[{"type":"rich_text_section","elements":[{"type":"text","text":"Preserved","style":{"bold":true}}]}]}),
    ];
    let inventory = journal.save(&inventory, &capture, blocks.clone()).unwrap();
    let binding = Binding {
        id: uuid::Uuid::new_v4().to_string(),
        owner,
        generation: 7,
        root: target.root_ts().unwrap().into(),
    };
    shell.slack_lists.comments.drafts.entries.push(Draft {
        id: binding.id.clone(),
        disk,
        blocks,
        sequence: 1,
        capture,
        inventory: Some(inventory),
        native: None,
        native_binding: None,
        native_sequence: 0,
        generation: 7,
        composing: false,
        capture_failed: false,
        loading: false,
        saving: false,
        busy: false,
        error: None,
        status: None,
        retired_blocks: None,
        terminal: None,
    });
    shell.slack_lists.comments.drafts.active = Some(binding.clone());
    (shell, binding, local)
}
#[test]
fn permission_error_allows_departure_only_after_exact_draft_is_persisted() {
    let (mut shell, binding, _local) = setup();
    let e = entry_mut(&mut shell, &binding.id).unwrap();
    e.error = Some("The permissions could not be checked. Nothing was sent.".into());
    assert!(flushed(e, &e.blocks).unwrap());
    // Capture schedules a no-op save before departure checks persistence.
    e.saving = true;
    assert!(flushed(e, &e.blocks).unwrap());
    e.capture = uuid::Uuid::new_v4().to_string();
    assert!(flushed(e, &e.blocks).is_err());
}
#[test]
fn changed_root_retains_original_visible_mount_but_cannot_send() {
    let (mut shell, binding, _local) = setup();
    assert!(binding.current(&shell));
    assert!(presentation(&shell).unwrap().can_send);
    let old = shell
        .slack_lists
        .comments
        .entry
        .as_ref()
        .unwrap()
        .target
        .clone();
    let mut row = old.record_raw().clone();
    row["thread_ts"] = json!("1800000000.000001");
    let target = old
        .refresh(
            &json!({"ok":true,"file":old.document_raw()}),
            &json!({"ok":true,"record":row}),
            false,
        )
        .unwrap();
    shell.slack_lists.comments.entry.as_mut().unwrap().target = target;
    assert!(binding.visible_mount(&shell));
    assert!(!binding.current(&shell));
    let view = presentation(&shell).unwrap();
    assert!(!view.can_send);
    assert!(view.ready);
    assert!(view.error.unwrap().contains("root changed"));
    assert!(controller::callback_current(&shell, binding.owner.serial));
}
#[test]
fn journal_load_failure_never_authorizes_editable_mount() {
    let (mut shell, binding, _local) = setup();
    let e = entry_mut(&mut shell, &binding.id).unwrap();
    e.inventory = None;
    e.error = Some("unavailable".into());
    let view = presentation(&shell).unwrap();
    assert!(!view.ready && !view.can_send);
    assert!(view.error.is_some());
}
#[test]
fn original_draft_is_carried_inertly_across_account_replacement() {
    let (mut previous, binding, _local) = setup();
    let old_blocks = entry(&previous, &binding.id).unwrap().blocks.clone();
    let mut next = super::super::tests::shell();
    next.core.active_account = Some("foreign".into());
    super::super::carry_pending(&mut previous, &mut next);
    assert!(!binding.visible_mount(&next));
    assert!(presentation(&next).is_none());
    assert_eq!(entry(&next, &binding.id).unwrap().blocks, old_blocks);
    assert!(previous.slack_lists.comments.drafts.entries.is_empty());
}
#[test]
fn unresolved_attempt_allows_retained_draft_edits_but_never_new_send() {
    let (mut shell, binding, local) = setup();
    let target = shell
        .slack_lists
        .comments
        .entry
        .as_ref()
        .unwrap()
        .target
        .clone();
    let page=Page::parse(&target,json!({"ok":true,"messages":[{"type":"message","subtype":"list_record_comment","ts":target.root_ts(),"slack_list":{"list_id":target.list_id(),"list_record_id":target.record_id()}}]})).unwrap();
    let e = entry_mut(&mut shell, &binding.id).unwrap();
    let intent = ReplyIntent::capture_rich(
        &e.disk.scope.workspace,
        &e.disk.scope.user,
        &target,
        page.root().unwrap(),
        &ReplyPolicy::observed(json!({"id":target.channel_id()}), false, true),
        e.blocks.clone(),
        &uuid::Uuid::new_v4().to_string(),
        "1700000002.000001",
    )
    .unwrap();
    let mut journal = Journal::open(&local.0, &e.disk.account).unwrap();
    let i = journal
        .begin(e.inventory.as_ref().unwrap(), intent.record().clone())
        .unwrap();
    e.inventory = Some(i);
    let view = presentation(&shell).unwrap();
    assert!(view.locked && view.ready && !view.busy && !view.can_send);
    let e = entry_mut(&mut shell, &binding.id).unwrap();
    e.blocks = vec![
        json!({"type":"rich_text","elements":[{"type":"rich_text_section","elements":[{"type":"text","text":"Newer draft"}]}]}),
    ];
    e.capture = uuid::Uuid::new_v4().to_string();
    let i = journal
        .save(e.inventory.as_ref().unwrap(), &e.capture, e.blocks.clone())
        .unwrap();
    adopt(e, i);
    assert!(presentation(&shell).unwrap().locked);
}
#[test]
fn stale_generation_and_closed_panel_cannot_retain_visible_authority() {
    let (mut shell, binding, _local) = setup();
    shell
        .slack_lists
        .comments
        .drafts
        .active
        .as_mut()
        .unwrap()
        .generation += 1;
    assert!(!binding.visible_mount(&shell));
    shell.slack_lists.comments.drafts.active = Some(binding.clone());
    controller::close_shell(&mut shell);
    assert!(!binding.visible_mount(&shell));
    assert!(entry(&shell, &binding.id).is_some());
}
