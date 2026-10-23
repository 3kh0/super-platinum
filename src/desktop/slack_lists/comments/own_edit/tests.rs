use super::*;
mod capture;
mod durable;
use super_platinum_core::{
    slack::events::RtEvent,
    slack_lists::comments::{OwnEditIntent, OwnEditPolicy},
};
const TS: &str = "1700000001.000001";
fn rich(text: &str) -> Vec<Value> {
    vec![
        json!({"type":"rich_text","elements":[{"type":"rich_text_section","elements":[{"type":"text","text":text,"style":{"bold":true},"opaque":7}]}]}),
    ]
}
pub(super) fn setup() -> (ShellState, Binding, OwnEditPolicy) {
    let mut shell = super::super::tests::shell();
    shell.core.workspaces.get_mut("T1").unwrap().rt_generation = 7;
    let user = shell.core.workspaces["T1"].self_user_id.clone();
    let (owner, serial, target, cursor) = controller::begin(&mut shell, false).unwrap();
    let root = target.root_ts().unwrap().to_owned();
    let mut blocks = rich("Original 中🦊");
    blocks.push(json!({"type":"context","elements":[{"type":"plain_text","text":"Retain context"}],"opaque":true}));
    let page=Page::parse(&target,json!({"ok":true,"messages":[
        {"type":"message","subtype":"list_record_comment","ts":root,"slack_list":{"list_id":target.list_id(),"list_record_id":target.record_id()}},
        {"type":"message","ts":TS,"thread_ts":root,"user":user,"text":"Original 中🦊","blocks":blocks,"reactions":[{"name":"wave","count":1,"users":["UOTHER"]}]}],"has_more":false})).unwrap();
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
    let source = Arc::new(Source::capture(&shell, owner.serial, TS).unwrap());
    let policy = OwnEditPolicy::observed(
        "T1",
        &user,
        json!({"id":target.channel_id()}),
        true,
        false,
        vec![-1],
        false,
        false,
    )
    .unwrap();
    let e = shell.slack_lists.comments.entry.as_ref().unwrap();
    let seed = OwnEditSeed::capture(
        "T1",
        &user,
        e.page.as_ref().unwrap(),
        source::message(e, TS).unwrap(),
        &policy,
        1_700_000_100_000,
    )
    .unwrap();
    let binding = Binding {
        source,
        token: next_id(),
    };
    shell.slack_lists.comments.own_edit.active = Some(Session {
        titles: None,
        binding: binding.clone(),
        blocks: seed.editable().to_vec(),
        plain: None,
        plain_mount: None,
        seed: Arc::new(seed),
        revision: 0,
        native: None,
        native_sequence: 0,
        native_ready: false,
        composing: false,
        native_pending: false,
        capture_failed: false,
        busy: false,
        finishing: false,
        durable: None,
        local_retry_at: 0,
        notice: None,
        suspension: Arc::new(Mutex::new(None)),
        attempt: None,
    });
    (shell, binding, policy)
}
fn frame(raw: Value) -> RtEvent {
    super_platinum_core::slack::realtime::parse_event(
        &json!({"type":"message","subtype":"message_changed","channel":"CCOMMENT01","message":raw})
            .to_string(),
    )
    .unwrap()
}
fn ready(shell: &mut ShellState, binding: &Binding, mount: &str) {
    let blocks = session(shell, binding).unwrap().blocks.clone();
    assert!(install_native(
        shell,
        binding,
        bridge::test_mounted(binding, mount)
    ));
    assert!(retain_capture(shell, binding, mount, 1, blocks, false, false).is_ok());
}
fn intent(shell: &ShellState, binding: &Binding, policy: &OwnEditPolicy) -> OwnEditIntent {
    let e = shell.slack_lists.comments.entry.as_ref().unwrap();
    OwnEditIntent::capture(
        &session(shell, binding).unwrap().seed,
        source::message(e, TS).unwrap(),
        policy,
        rich("Edited 日"),
        1_700_000_100_000,
    )
    .unwrap()
}
fn attempt(
    shell: &ShellState,
    binding: &Binding,
    policy: &OwnEditPolicy,
    phase: Phase,
) -> Arc<Mutex<Attempt>> {
    Arc::new(Mutex::new(Attempt {
        id: next_id(),
        disk_id: None,
        account: shell.core.active_account.clone(),
        record: intent(shell, binding, policy).record().clone(),
        phase,
        receipt: None,
        retry_at: 0,
    }))
}
#[test]
fn original_body_allows_reactions_and_reply_counts_but_not_body_or_owner_changes() {
    let (mut shell, binding, _) = setup();
    let mut other = binding.source.original_for_test();
    other.reactions.clear();
    other.reply_count = Some(99);
    other.channel = Some("CCOMMENT01".into());
    assert!(binding.source.same(&other));
    other.blocks = rich("Different");
    assert!(!binding.source.same(&other));
    assert!(binding.current(&shell));
    shell.core.workspaces.get_mut("T1").unwrap().rt_generation += 1;
    assert!(!binding.current(&shell));
    assert!(binding.visible_mount(&shell));
    shell.core.account_epoch += 1;
    assert!(!binding.visible_mount(&shell));
}
#[test]
fn equal_original_body_frame_retires_custody_foreign_replied_and_reaction_frames_do_not() {
    let (mut shell, binding, _) = setup();
    ready(&mut shell, &binding, "source-test");
    let event = frame(
        json!({"ts":TS,"thread_ts":binding.source.target.root_ts(),"blocks":session(&shell,&binding).unwrap().seed.original_raw()["blocks"]}),
    );
    assert!(!observe_event(&mut shell, "TOTHER", 7, &event));
    assert!(!observe_event(&mut shell, "T1", 8, &event));
    assert!(binding.current(&shell));
    assert!(!observe_event(
        &mut shell,
        "T1",
        7,
        &frame(json!({"ts":TS,"reactions":[]}))
    ));
    assert!(binding.current(&shell));
    assert!(observe_event(&mut shell, "T1", 7, &event));
    assert!(!binding.current(&shell));
    assert!(binding.visible_mount(&shell));
    assert!(presentation(&shell).unwrap().can_cancel);
    assert!(!presentation(&shell).unwrap().can_save);
}
#[test]
fn selected_and_root_deletion_only_retire_original_scope() {
    for root in [false, true] {
        let (mut shell, binding, _) = setup();
        let ts = if root {
            binding.source.target.root_ts().unwrap().into()
        } else {
            TS.into()
        };
        assert!(observe_event(
            &mut shell,
            "T1",
            7,
            &RtEvent::MessageDeleted {
                channel: "CCOMMENT01".into(),
                deleted_ts: ts
            }
        ));
        assert!(!binding.current(&shell));
    }
}
#[test]
fn native_capture_is_exact_mount_monotonic_and_never_writes_reply_journal() {
    let (mut shell, binding, _) = setup();
    let mounted = bridge::test_mounted(&binding, "original-edit");
    assert!(install_native(&mut shell, &binding, mounted));
    let draft_count = shell.slack_lists.comments.drafts.entries.len();
    assert!(
        retain_capture(
            &mut shell,
            &binding,
            "foreign",
            1,
            rich("Wrong"),
            false,
            false
        )
        .is_err()
    );
    assert!(
        retain_capture(
            &mut shell,
            &binding,
            "original-edit",
            1,
            rich("Retained 中"),
            false,
            false
        )
        .is_ok()
    );
    assert!(
        retain_capture(
            &mut shell,
            &binding,
            "original-edit",
            1,
            rich("Old"),
            false,
            false
        )
        .is_err()
    );
    let s = session(&shell, &binding).unwrap();
    assert_eq!(s.blocks, rich("Retained 中"));
    assert_eq!(s.revision, 1);
    assert_eq!(shell.slack_lists.comments.drafts.entries.len(), draft_count);
    native_done(&mut shell, &binding, "foreign", true);
    assert!(native(&shell, &binding).is_some());
    native_done(&mut shell, &binding, "original-edit", false);
    assert!(session(&shell, &binding).unwrap().capture_failed);
}
#[test]
fn unknown_blocks_save_cancel_and_departure_and_survives_account_carry() {
    let (mut shell, binding, policy) = setup();
    ready(&mut shell, &binding, "unknown-test");
    let a = attempt(&shell, &binding, &policy, Phase::Unknown);
    shell.slack_lists.comments.own_edit.attempts.push(a.clone());
    session_mut(&mut shell, &binding).unwrap().attempt = Some(a);
    let p = presentation(&shell).unwrap();
    assert!(!p.can_save && !p.can_cancel && p.can_check);
    assert!(blocks_departure(&shell, binding.source.owner.serial));
    assert!(blocked_attempt(&shell, &binding.source));
    let (mut successor, _, _) = setup();
    carry_pending(&shell, &mut successor);
    assert_eq!(successor.slack_lists.comments.own_edit.attempts.len(), 1);
}

#[test]
fn exact_unchanged_body_does_not_bypass_native_source_or_uncertain_save_guards() {
    let (mut shell, binding, policy) = setup();
    assert!(
        session(&shell, &binding)
            .unwrap()
            .seed
            .unchanged(&session(&shell, &binding).unwrap().blocks)
    );
    assert!(!presentation(&shell).unwrap().can_save); // Native mount not ready.
    ready(&mut shell, &binding, "unchanged-original");
    assert!(presentation(&shell).unwrap().can_save);
    let a = attempt(&shell, &binding, &policy, Phase::Unknown);
    session_mut(&mut shell, &binding).unwrap().attempt = Some(a);
    assert!(!presentation(&shell).unwrap().can_save);
    session_mut(&mut shell, &binding).unwrap().attempt = None;
    shell.slack_lists.comments.own_edit.events += 1;
    assert!(!presentation(&shell).unwrap().can_save);
    assert!(
        session(&shell, &binding)
            .unwrap()
            .seed
            .unchanged(&session(&shell, &binding).unwrap().blocks)
    ); // Equality does not grant original-source authority.
}
#[test]
fn confirmed_projection_preserves_live_reactions_original_receipt_and_cancels_only_owned_page_read()
{
    let (mut shell, binding, policy) = setup();
    let intent = intent(&shell, &binding, &policy);
    let raw = json!({"ok":true,"channel":"CCOMMENT01","ts":TS,"message":{"blocks":intent.record().blocks(),"text":"Edited 日","edited":{"ts":"1700000100.000001"}}});
    let ack = intent.accept(raw).unwrap();
    let before = shell
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
    let (owner, serial, _, _) = controller::begin(&mut shell, false).unwrap();
    assert!(publish::accepted(&mut shell, &binding, &ack));
    assert!(!owner.reading(&shell, serial));
    let e = shell.slack_lists.comments.entry.as_ref().unwrap();
    assert_eq!(e.page.as_ref().unwrap().raw(), &before);
    let message = source::message(e, TS).unwrap();
    assert_eq!(message.reactions[0].name, "wave");
    assert_eq!(message.blocks, ack.record().blocks());
    assert!(!publish::accepted(&mut shell, &binding, &ack));
}
#[test]
fn failed_preflight_releases_only_its_read_and_preserves_current_rows() {
    let (mut shell, binding, _) = setup();
    let (owner, serial, _, _) = controller::begin_preflight(&mut shell).unwrap();
    let rows = shell
        .slack_lists
        .comments
        .entry
        .as_ref()
        .unwrap()
        .rows
        .clone();
    publish::finish_read(
        &mut shell,
        &owner,
        serial,
        Some(&Error::RateLimited {
            retry_after_secs: Some(2),
        }),
    );
    assert!(!owner.reading(&shell, serial));
    assert!(Arc::ptr_eq(
        &rows,
        &shell.slack_lists.comments.entry.as_ref().unwrap().rows
    ));
    assert!(binding.current(&shell));
    let e = shell.slack_lists.comments.entry.as_mut().unwrap();
    e.read = next_id();
    e.loading = true;
    publish::finish_read(&mut shell, &owner, serial, None);
    assert!(shell.slack_lists.comments.entry.as_ref().unwrap().loading);
}
#[test]
fn preference_observer_uses_original_team_socket_even_while_native_composing() {
    let (mut shell, binding, _) = setup();
    session_mut(&mut shell, &binding).unwrap().composing = true;
    assert!(observes_preferences(&shell, "T1", 7));
    assert!(!observes_preferences(&shell, "T1", 8));
    assert!(!observes_preferences(&shell, "TOTHER", 7));
    shell.core.account_epoch += 1;
    assert!(!observes_preferences(&shell, "T1", 7));
}
#[test]
fn uncertain_result_is_not_replayable_and_issued_429_without_header_has_bounded_cooldown() {
    use super_platinum_core::slack::transport::ListCommentUpdateFailure as Failure;
    let (shell, binding, policy) = setup();
    let a = attempt(&shell, &binding, &policy, Phase::Sent);
    let raw = json!({"ok":"true","opaque":"Original ambiguous receipt"});
    run::settle(
        &a,
        &Err(Failure::Unconfirmed {
            error: Error::Transport("lost".into()),
            receipt: Some(raw.clone()),
        }),
    );
    assert!(a.lock().unwrap().phase == Phase::Unknown);
    assert_eq!(a.lock().unwrap().receipt, Some(raw));
    let b = attempt(&shell, &binding, &policy, Phase::Sent);
    let before = now();
    run::settle(
        &b,
        &Err(Failure::Unconfirmed {
            error: Error::RateLimited {
                retry_after_secs: None,
            },
            receipt: None,
        }),
    );
    assert!(b.lock().unwrap().phase == Phase::Rejected);
    assert!(b.lock().unwrap().retry_at >= before + 30);
    let c = attempt(&shell, &binding, &policy, Phase::Sent);
    run::settle(
        &c,
        &Err(Failure::Rejected(json!({"ok":false,"error":"ratelimited"}))),
    );
    assert!(c.lock().unwrap().retry_at >= before + 30);
    assert!(
        retry_deadline(&Error::RateLimited {
            retry_after_secs: None
        }) >= before + 30
    );
}
#[test]
fn forced_retirement_retains_exact_final_native_buffer_without_authorizing_successor() {
    let (mut shell, binding, _) = setup();
    assert!(install_native(
        &mut shell,
        &binding,
        bridge::test_mounted(&binding, "retired-node")
    ));
    shell.core.account_epoch += 1;
    retire(&mut shell);
    assert!(shell.slack_lists.comments.own_edit.active.is_none());
    assert_eq!(shell.slack_lists.comments.own_edit.retired.len(), 1);
    assert!(!binding.visible_mount(&shell));
    assert!(binding.retained_mount(&shell));
    assert!(
        retain_capture(
            &mut shell,
            &binding,
            "retired-node",
            1,
            rich("Final detached 日🦊"),
            false,
            true
        )
        .is_ok()
    );
    assert_eq!(
        session(&shell, &binding).unwrap().blocks,
        rich("Final detached 日🦊")
    );
    native_done(&mut shell, &binding, "retired-node", true);
    assert!(!binding.retained_mount(&shell));
}
#[test]
fn native_readiness_requires_fresh_capture_after_original_holder_recovery() {
    let (mut shell, binding, policy) = setup();
    let blocks = session(&shell, &binding).unwrap().blocks.clone();
    let draft_count = shell.slack_lists.comments.drafts.entries.len();
    let view = presentation(&shell).unwrap();
    assert!(!view.can_save && !view.can_cancel && !view.can_check && !view.can_retry_capture);
    assert!(install_native(
        &mut shell,
        &binding,
        bridge::test_mounted(&binding, "recover-original")
    ));
    assert!(!presentation(&shell).unwrap().can_save);
    assert!(
        retain_capture(
            &mut shell,
            &binding,
            "recover-original",
            1,
            blocks.clone(),
            false,
            false
        )
        .is_ok()
    );
    assert!(presentation(&shell).unwrap().can_save);
    let a = attempt(&shell, &binding, &policy, Phase::Unknown);
    session_mut(&mut shell, &binding).unwrap().attempt = Some(a.clone());
    // The bridge records failure on its exact holder before native_done.
    session_mut(&mut shell, &binding).unwrap().native =
        Some(bridge::test_failed_mounted(&binding, "recover-original"));
    native_done(&mut shell, &binding, "recover-original", false);
    let view = presentation(&shell).unwrap();
    assert!(!view.can_save && !view.can_cancel && !view.can_check && view.can_retry_capture);
    let mut foreign = binding.clone();
    foreign.token += 1;
    assert!(!native::can_retry_capture(&shell, &foreign));
    assert!(!install_native(
        &mut shell,
        &foreign,
        bridge::test_mounted(&foreign, "recover-original")
    ));
    assert!(install_native(
        &mut shell,
        &binding,
        bridge::test_mounted(&binding, "recover-original")
    ));
    let view = presentation(&shell).unwrap();
    assert!(!view.can_save && !view.can_cancel && !view.can_check && !view.can_retry_capture);
    assert!(
        retain_capture(
            &mut shell,
            &binding,
            "recover-original",
            1,
            blocks.clone(),
            false,
            false
        )
        .is_err()
    );
    assert!(
        retain_capture(
            &mut shell,
            &binding,
            "recover-original",
            2,
            blocks.clone(),
            false,
            false
        )
        .is_ok()
    );
    let view = presentation(&shell).unwrap();
    assert!(!view.can_save && !view.can_cancel && view.can_check && !view.can_retry_capture);
    assert!(a.lock().unwrap().phase == Phase::Unknown);
    assert_eq!(session(&shell, &binding).unwrap().blocks, blocks);
    assert_eq!(shell.slack_lists.comments.drafts.entries.len(), draft_count);
    assert!(!session(&shell, &binding).unwrap().capture_failed);
}
#[test]
fn stale_comment_can_recover_native_then_cancel_without_regaining_save_authority() {
    let (mut shell, binding, _) = setup();
    ready(&mut shell, &binding, "stale-original");
    let blocks = session(&shell, &binding).unwrap().blocks.clone();
    session_mut(&mut shell, &binding).unwrap().native =
        Some(bridge::test_failed_mounted(&binding, "stale-original"));
    native_done(&mut shell, &binding, "stale-original", false);
    assert!(observe_event(
        &mut shell,
        "T1",
        7,
        &frame(json!({"ts":TS,"text":"Newer body"}))
    ));
    assert!(native::can_retry_capture(&shell, &binding));
    assert!(install_native(
        &mut shell,
        &binding,
        bridge::test_mounted(&binding, "stale-original")
    ));
    assert!(
        retain_capture(
            &mut shell,
            &binding,
            "stale-original",
            2,
            blocks,
            false,
            false
        )
        .is_ok()
    );
    let view = presentation(&shell).unwrap();
    assert!(!view.can_save && view.can_cancel && !view.can_retry_capture);
}
