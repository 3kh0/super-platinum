use super::*;
use crate::model::RichNode;

fn loaded(count: usize) -> (ShellState, u64, Vec<Demand>) {
    populate(super::super::tests::shell(), count)
}
fn populate(mut shell: ShellState, count: usize) -> (ShellState, u64, Vec<Demand>) {
    let (owner, serial, target, cursor) = controller::begin(&mut shell, false).unwrap();
    let mut tokens = vec![];
    for i in 0..count {
        tokens.push(json!({"type":"canvas","file_id":format!("FCOLD{i}"),"url":"https://untrusted.invalid/ignored","opaque":{"keep":true}}));
    }
    if let Some(first) = tokens.first().cloned() {
        tokens.push(first);
    }
    let page=Page::parse(&target,json!({"ok":true,"messages":[{"ts":"1700000001.000001","thread_ts":target.root_ts(),"user":"U1",
        "blocks":[{"type":"rich_text","elements":[{"type":"rich_text_section","elements":tokens}]}]}]})).unwrap();
    controller::finish(
        &mut shell,
        &owner,
        serial,
        cursor,
        Ok(network::Loaded {
            page,
            projection: Projection::observed(json!({"ok":true,"assignments":{}})).unwrap(),
        }),
    );
    let generation = owner.serial;
    let demands = pending(&shell, generation);
    (shell, generation, demands)
}
fn queued() -> (ShellState, u64, Attempt, Arc<()>) {
    let (mut shell, generation, demands) = loaded(2);
    assert_eq!(demands.len(), 2);
    queue(&mut shell, generation, demands);
    let worker = Arc::new(());
    shell.slack_lists.comments.file_acquisition.worker = Some(worker.clone());
    let attempt = shell.slack_lists.comments.file_acquisition.attempts[0].clone();
    (shell, generation, attempt, worker)
}

#[test]
fn duplicate_occurrences_dedup_and_bounded_queue_has_no_render_retry_loop() {
    let (mut shell, generation, demands) = loaded(MAX_FILES + 10);
    assert_eq!(demands.len(), MAX_FILES);
    queue(&mut shell, generation, demands);
    assert_eq!(
        shell.slack_lists.comments.file_acquisition.attempts.len(),
        MAX_FILES
    );
    assert!(pending(&shell, generation).is_empty());
    let attempt = shell.slack_lists.comments.file_acquisition.attempts[0].clone();
    assert_eq!(attempt.demand.presentation(&shell).state, "loading");
    run::finish(
        &mut shell,
        &attempt,
        Err(Error::Offline("synthetic".into())),
    );
    assert!(attempt.demand.presentation(&shell).retryable);
    assert!(
        pending(&shell, generation).is_empty(),
        "failure must wait for a real retry"
    );
}
#[test]
fn retry_is_original_occurrence_owned_and_body_socket_account_retirement_cancels() {
    for mutation in 0..5 {
        let (mut shell, _, attempt, worker) = queued();
        assert!(attempt_current(&shell, &attempt, &worker));
        match mutation {
            0 => Arc::make_mut(&mut shell.slack_lists.comments.entry.as_mut().unwrap().rows)[0]
                .body
                .push(RichNode::Text("changed".into())),
            1 => shell.core.account_epoch += 1,
            2 => {
                shell
                    .core
                    .workspaces
                    .get_mut(&attempt.demand.team)
                    .unwrap()
                    .rt_generation += 1
            }
            3 => {
                shell
                    .slack_lists
                    .comments
                    .entry
                    .as_mut()
                    .unwrap()
                    .deleted
                    .insert("1700000001.000001".into());
            }
            _ => controller::close_shell(&mut shell),
        }
        assert!(!attempt_current(&shell, &attempt, &worker));
        assert!(!retry_allowed(&shell, &attempt.demand));
    }
}
#[test]
fn unrelated_projection_change_does_not_retire_or_requeue_original() {
    let (mut shell, generation, attempt, worker) = queued();
    Arc::make_mut(&mut shell.slack_lists.comments.entry.as_mut().unwrap().rows)[0].author =
        "Changed author display".into();
    assert!(attempt_current(&shell, &attempt, &worker));
    assert!(pending(&shell, generation).is_empty());
}
#[test]
fn retry_after_stops_remaining_ids_and_survives_close_without_auto_dispatch() {
    let (mut shell, _, attempt, _) = queued();
    let before = now();
    let deadline = run::finish(
        &mut shell,
        &attempt,
        Err(Error::RateLimited {
            retry_after_secs: Some(30),
        }),
    );
    assert!(deadline >= before + 30);
    for row in &shell.slack_lists.comments.file_acquisition.attempts {
        assert!(row.phase == Phase::Unavailable);
        assert_eq!(row.retry_at, deadline);
        assert!(!row.demand.presentation(&shell).retryable);
    }
    controller::close_shell(&mut shell);
    assert_eq!(cooldown(&shell), deadline);
    assert!(
        shell
            .slack_lists
            .comments
            .file_acquisition
            .attempts
            .is_empty()
    );
    assert!(shell.slack_lists.comments.file_acquisition.worker.is_none());
    let (fallback, _) = failure(&Error::HttpStatus {
        status: 429,
        retry_after_secs: None,
    });
    assert!(fallback >= before + 30);
}
#[test]
fn unsupported_subobject_wrong_team_and_changed_raw_cannot_create_demand() {
    let (shell, _, demands) = loaded(1);
    let original = &demands[0];
    assert!(
        original
            .source
            .cold_occurrence(&shell, &original.team, &original.raw, original.ordinal)
            .is_some()
    );
    assert!(
        original
            .source
            .cold_occurrence(&shell, "TFOREIGN", &original.raw, original.ordinal)
            .is_none()
    );
    for key in ["section_id", "record_id", "is_skill_invocation"] {
        let mut raw = original.raw.clone();
        raw[key] = json!(true);
        assert!(
            Demand::capture(&shell, original.source.clone(), &original.team, &raw, 0).is_none()
        );
        assert!(
            original
                .source
                .cold_occurrence(&shell, &original.team, &raw, 0)
                .is_none()
        );
    }
}
#[test]
fn acquisition_queue_and_failure_preserve_original_raw_page_and_draft_state() {
    let (shell, binding, _local) = draft::tests::setup();
    let (mut shell, generation, demands) = populate(shell, 1);
    let draft_before = draft::entry(&shell, binding.id()).unwrap();
    let blocks = draft_before.blocks.clone();
    let sequence = draft_before.sequence;
    let capture = draft_before.capture.clone();
    let inventory = draft_before.inventory.as_ref().unwrap().blocks().to_vec();
    let page = shell
        .slack_lists
        .comments
        .entry
        .as_ref()
        .unwrap()
        .page
        .as_ref()
        .unwrap()
        .raw()
        .clone();
    let rows = shell
        .slack_lists
        .comments
        .entry
        .as_ref()
        .unwrap()
        .rows
        .clone();
    let route = shell.slack_lists.view.as_ref().unwrap().generation;
    queue(&mut shell, generation, demands);
    let attempt = shell.slack_lists.comments.file_acquisition.attempts[0].clone();
    run::finish(&mut shell, &attempt, Err(Error::Api("malformed".into())));
    assert_eq!(
        shell
            .slack_lists
            .comments
            .entry
            .as_ref()
            .unwrap()
            .page
            .as_ref()
            .unwrap()
            .raw(),
        &page
    );
    assert!(Arc::ptr_eq(
        &rows,
        &shell.slack_lists.comments.entry.as_ref().unwrap().rows
    ));
    assert_eq!(shell.slack_lists.view.as_ref().unwrap().generation, route);
    assert!(binding.current(&shell));
    let after = draft::entry(&shell, binding.id()).unwrap();
    assert_eq!(after.blocks, blocks);
    assert_eq!(after.sequence, sequence);
    assert_eq!(after.capture, capture);
    assert_eq!(after.inventory.as_ref().unwrap().blocks(), inventory);
}
