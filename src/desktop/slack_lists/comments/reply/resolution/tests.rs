use super::*;
use serde_json::json;
use super_platinum_core::slack_lists::comments::journal::Journal;
fn pending(shell: &mut ShellState, binding: &draft::Binding) -> Terminal {
    let target = shell
        .slack_lists
        .comments
        .entry
        .as_ref()
        .unwrap()
        .target
        .clone();
    let page=Page::parse(&target,json!({"ok":true,"messages":[{"type":"message","subtype":"list_record_comment","ts":target.root_ts(),"slack_list":{"list_id":target.list_id(),"list_record_id":target.record_id()}}]})).unwrap();
    let e = draft::entry_mut(shell, &binding.id).unwrap();
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
    let record = intent.record().clone();
    let terminal = Terminal {
        record: record.clone(),
        resolution: Resolution::Accepted(
            json!({"type":"message","ts":"1700000003.000001","thread_ts":target.root_ts(),"user":record.user(),"client_msg_id":record.client_msg_id(),"blocks":record.blocks()}),
        ),
        status: "Comment sent.".into(),
        sequence: e.sequence,
        owner: binding.owner.clone(),
        reveal: true,
    };
    e.terminal = Some(terminal.clone());
    terminal
}
fn visible_page(shell: &mut ShellState, terminal: &Terminal) {
    let (owner, serial, target, cursor) = controller::begin(shell, false).unwrap();
    let page = Page::parse(
        &target,
        json!({"ok":true,"messages":[terminal.record.root_raw()],
        "has_more":true,"response_metadata":{"next_cursor":"more"}}),
    )
    .unwrap();
    controller::finish(
        shell,
        &owner,
        serial,
        cursor,
        Ok(network::Loaded {
            page,
            projection: Projection::observed(json!({"ok":true,"assignments":{}})).unwrap(),
        }),
    );
}
fn committed(shell: &ShellState, binding: &draft::Binding, terminal: &Terminal) -> Inventory {
    let e = draft::entry(shell, &binding.id).unwrap();
    let path = std::env::temp_dir().join(format!("list-publish-{}.sqlite", uuid::Uuid::new_v4()));
    let mut journal = Journal::open(&path, &e.disk.account).unwrap();
    let i = journal.inventory(&e.disk.scope).unwrap();
    let i = journal
        .save(&i, &e.capture, terminal.record.blocks().to_vec())
        .unwrap();
    journal.begin(&i, terminal.record.clone()).unwrap();
    let inventory = journal
        .resolve(&e.disk.scope, &terminal.record, terminal.resolution.clone())
        .unwrap();
    drop(journal);
    std::fs::remove_file(path).unwrap();
    inventory
}

#[test]
fn confirmed_reply_projects_only_after_real_journal_settlement_and_local_retry() {
    let (mut shell, binding, _local) = draft::tests::setup();
    let terminal = pending(&mut shell, &binding);
    visible_page(&mut shell, &terminal);
    assert!(!apply_result(
        &mut shell,
        &binding.id,
        &terminal,
        Err(unavailable())
    ));
    assert!(
        shell
            .slack_lists
            .comments
            .entry
            .as_ref()
            .unwrap()
            .accepted
            .is_empty()
    );
    assert!(
        draft::entry(&shell, &binding.id)
            .unwrap()
            .terminal
            .is_some()
    );
    let inventory = committed(&shell, &binding, &terminal);
    assert!(apply_result(
        &mut shell,
        &binding.id,
        &terminal,
        Ok(inventory)
    ));
    let view = presentation(&shell).unwrap();
    assert_eq!(view.rows.len(), 1);
    assert_eq!(view.rows[0].ts, "1700000003.000001");
    assert_eq!(view.reveal.as_ref().unwrap().1, "1700000003.000001");
    assert!(view.has_more);
    let e = draft::entry(&shell, &binding.id).unwrap();
    assert!(e.blocks.is_empty() && e.terminal.is_none());
}

#[test]
fn settled_old_receipt_cannot_publish_into_a_reopened_original_item() {
    let (mut shell, binding, _local) = draft::tests::setup();
    let terminal = pending(&mut shell, &binding);
    visible_page(&mut shell, &terminal);
    assert!(!apply_result(
        &mut shell,
        &binding.id,
        &terminal,
        Err(unavailable())
    ));
    controller::close_shell(&mut shell);
    let generation = record_detail::presentation(shell.slack_lists.view.as_ref().unwrap())
        .unwrap()
        .generation;
    assert!(controller::open_shell(&mut shell, generation));
    visible_page(&mut shell, &terminal);
    let inventory = committed(&shell, &binding, &terminal);
    assert!(apply_result(
        &mut shell,
        &binding.id,
        &terminal,
        Ok(inventory)
    ));
    let e = shell.slack_lists.comments.entry.as_ref().unwrap();
    assert!(e.accepted.is_empty() && e.reveal.is_none());
    assert!(e.rows.iter().all(|row| row.ts != "1700000003.000001"));
}

#[test]
fn rejected_and_uncertain_terminal_never_publish_a_confirmed_reply() {
    for resolution in [
        Resolution::NotRequested,
        Resolution::Rejected(json!({"ok":false,"error":"restricted_action"})),
        Resolution::Uncertain(None),
    ] {
        let (mut shell, binding, _local) = draft::tests::setup();
        let mut terminal = pending(&mut shell, &binding);
        terminal.resolution = resolution;
        visible_page(&mut shell, &terminal);
        let original = draft::entry(&shell, &binding.id).unwrap().blocks.clone();
        let inventory = committed(&shell, &binding, &terminal);
        assert!(!apply_result(
            &mut shell,
            &binding.id,
            &terminal,
            Ok(inventory)
        ));
        assert_eq!(draft::entry(&shell, &binding.id).unwrap().blocks, original);
        let e = shell.slack_lists.comments.entry.as_ref().unwrap();
        assert!(e.accepted.is_empty() && e.reveal.is_none());
    }
}

#[test]
fn accepted_projection_does_not_replace_newer_native_draft() {
    let (mut shell, binding, _local) = draft::tests::setup();
    let terminal = pending(&mut shell, &binding);
    visible_page(&mut shell, &terminal);
    let inventory = committed(&shell, &binding, &terminal);
    let e = draft::entry_mut(&mut shell, &binding.id).unwrap();
    e.sequence += 1;
    e.blocks = vec![
        json!({"type":"rich_text","elements":[{"type":"rich_text_section","elements":[{"type":"text","text":"Newer B"}]}]}),
    ];
    let newer = e.blocks.clone();
    let generation = e.generation;
    assert!(!apply_result(
        &mut shell,
        &binding.id,
        &terminal,
        Ok(inventory)
    ));
    let e = draft::entry(&shell, &binding.id).unwrap();
    assert_eq!(e.blocks, newer);
    assert_eq!(e.generation, generation);
    assert_eq!(
        presentation(&shell).unwrap().rows[0].ts,
        "1700000003.000001"
    );
}
#[test]
fn local_settlement_failure_retains_exact_terminal_for_retry_without_resend() {
    let (mut shell, binding, _local) = draft::tests::setup();
    let terminal = pending(&mut shell, &binding);
    let e = draft::entry_mut(&mut shell, &binding.id).unwrap();
    let original = e.blocks.clone();
    assert!(!apply(e, &terminal, Err(unavailable())));
    assert_eq!(e.blocks, original);
    assert_eq!(e.terminal.as_ref().unwrap().record, terminal.record);
    assert!(matches!(
        e.terminal.as_ref().unwrap().resolution,
        Resolution::Accepted(_)
    ));
    assert!(!draft::presentation(&shell).unwrap().can_send);
}
#[test]
fn late_terminal_never_clears_newer_local_native_capture() {
    let (mut shell, binding, _local) = draft::tests::setup();
    let terminal = pending(&mut shell, &binding);
    let e = draft::entry_mut(&mut shell, &binding.id).unwrap();
    // An empty result from the original persisted capture does not authorize
    // clearing a newer in-memory original-node tail awaiting its own disk save.
    let path =
        std::env::temp_dir().join(format!("list-resolution-{}.sqlite", uuid::Uuid::new_v4()));
    let mut journal = Journal::open(&path, &e.disk.account).unwrap();
    let i = journal.inventory(&e.disk.scope).unwrap();
    let i = journal
        .save(
            &i,
            &uuid::Uuid::new_v4().to_string(),
            terminal.record.blocks().to_vec(),
        )
        .unwrap();
    journal.begin(&i, terminal.record.clone()).unwrap();
    let accepted = journal
        .resolve(&e.disk.scope, &terminal.record, terminal.resolution.clone())
        .unwrap();
    e.sequence += 1;
    e.blocks = vec![json!({"type":"future","opaque":"new native content"})];
    let newer = e.blocks.clone();
    assert!(!apply(e, &terminal, Ok(accepted)));
    assert_eq!(e.blocks, newer);
    assert!(e.terminal.is_none());
    drop(journal);
    let _ = std::fs::remove_file(path);
}
