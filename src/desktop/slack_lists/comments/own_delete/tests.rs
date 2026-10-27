use super::*;
use serde_json::json;
use super_platinum_core::{
    slack::events::RtEvent,
    slack_lists::comments::{OwnDeletePermissionQuery, OwnDeletePolicy},
};
pub(super) mod durable;

pub(super) const TS: &str = "1700000001.000001";
pub(super) fn rich(text: &str) -> Vec<Value> {
    vec![
        json!({"type":"rich_text","elements":[{"type":"rich_text_section","elements":[{"type":"text","text":text,"style":{"bold":true},"opaque":7}]}]}),
    ]
}
pub(super) fn load(shell: &mut ShellState) {
    shell.core.workspaces.get_mut("T1").unwrap().rt_generation = 7;
    let user = shell.core.workspaces["T1"].self_user_id.clone();
    let (owner, serial, target, cursor) = controller::begin(shell, false).unwrap();
    let root = target.root_ts().unwrap();
    let page = Page::parse(&target, json!({"ok":true,"messages":[
        {"type":"message","subtype":"list_record_comment","ts":root,"slack_list":{"list_id":target.list_id(),"list_record_id":target.record_id()}},
        {"type":"message","ts":TS,"thread_ts":root,"user":user,"text":"Original 中🦊","blocks":rich("Original 中🦊"),"reactions":[{"name":"wave","count":1,"users":["UOTHER"]}]}],
        "has_more":true,"response_metadata":{"next_cursor":"next"}})).unwrap();
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
pub(super) fn setup() -> (ShellState, Arc<Source>, OwnDeletePolicy) {
    let mut shell = super::super::tests::shell();
    load(&mut shell);
    let generation = shell
        .slack_lists
        .comments
        .entry
        .as_ref()
        .unwrap()
        .owner
        .serial;
    let source = Arc::new(Source::capture(&shell, generation, TS).unwrap());
    let user = &shell.core.workspaces["T1"].self_user_id;
    let permission =
        OwnDeletePermissionQuery::capture("T1", user, source.target.channel_id(), false)
            .unwrap()
            .accept(json!({"ok":true,"allowed":["DELETE_OWN_MESSAGE"]}))
            .unwrap();
    let policy = OwnDeletePolicy::observed(
        "T1",
        user,
        json!({"id":source.target.channel_id(),"is_file":true}),
        true,
        false,
        false,
        &permission,
    )
    .unwrap();
    (shell, source, policy)
}
pub(super) fn intent(
    shell: &ShellState,
    source: &Source,
    policy: &OwnDeletePolicy,
) -> OwnDeleteIntent {
    OwnDeleteIntent::capture(
        "T1",
        &shell.core.workspaces["T1"].self_user_id,
        shell
            .slack_lists
            .comments
            .entry
            .as_ref()
            .unwrap()
            .page
            .as_ref()
            .unwrap(),
        &source.original,
        policy,
    )
    .unwrap()
}
pub(super) fn install(shell: &mut ShellState, source: Arc<Source>, record: OwnDeleteRecord) -> u64 {
    let id = next_id();
    let preview = shell
        .slack_lists
        .comments
        .entry
        .as_ref()
        .unwrap()
        .rows
        .iter()
        .find(|m| m.ts == TS)
        .unwrap()
        .clone();
    shell.slack_lists.comments.own_delete.active = Some(Session {
        id,
        source,
        preview,
        busy: false,
        notice: None,
        record: Some(record),
        phase: None,
        retry_at: 0,
        local_pending: false,
        recovery: None,
        suspension: Arc::new(Mutex::new(None)),
    });
    id
}
pub(super) fn change_page(shell: &mut ShellState, change: impl FnOnce(&mut Value)) {
    let e = shell.slack_lists.comments.entry.as_mut().unwrap();
    let mut raw = e.page.as_ref().unwrap().raw().clone();
    change(&mut raw);
    e.page = Some(Page::parse(&e.target, raw).unwrap());
}
fn changed(raw: Value) -> RtEvent {
    super_platinum_core::slack::realtime::parse_event(
        &json!({"type":"message","subtype":"message_changed","channel":"CCOMMENT01","message":raw})
            .to_string(),
    )
    .unwrap()
}

#[test]
fn candidate_preserves_plain_rich_and_file_scope_but_refuses_foreign_root_and_loading() {
    for kind in 0..3 {
        let (mut shell, source, _) = setup();
        change_page(&mut shell, |raw| {
            if kind == 0 {
                raw["messages"][1]["blocks"] = json!([]);
            }
            if kind == 2 {
                raw["messages"][1]["subtype"] = json!("file_share");
                raw["messages"][1]["files"] = json!([{"id":"FFILE001","mode":"hosted"}]);
            }
        });
        assert!(Source::capture(&shell, source.owner.serial, TS).is_some());
        assert!(
            Source::capture(
                &shell,
                source.owner.serial,
                source.target.root_ts().unwrap()
            )
            .is_none()
        );
        change_page(&mut shell, |raw| {
            raw["messages"][1]["user"] = json!("UOTHER")
        });
        assert!(Source::capture(&shell, source.owner.serial, TS).is_none());
    }
    let (mut shell, source, _) = setup();
    change_page(&mut shell, |raw| {
        raw["messages"][0]["is_locked"] = json!(true)
    });
    assert!(Source::capture(&shell, source.owner.serial, TS).is_none());
    let (mut shell, source, _) = setup();
    shell.slack_lists.comments.entry.as_mut().unwrap().loading = true;
    assert!(Source::capture(&shell, source.owner.serial, TS).is_none());
}

#[test]
fn body_custody_ignores_reactions_counters_but_refuses_changed_content_socket_account_and_visit() {
    for mutation in 0..7 {
        let (mut shell, source, _) = setup();
        change_page(&mut shell, |raw| {
            raw["messages"][1]["reactions"] = json!([]);
            raw["messages"][1]["reply_count"] = json!(99);
        });
        assert!(source.current(&shell));
        match mutation {
            0 => change_page(&mut shell, |raw| {
                raw["messages"][1]["blocks"] = json!(rich("New body"))
            }),
            1 => shell.core.workspaces.get_mut("T1").unwrap().rt_generation += 1,
            2 => shell.core.account_epoch += 1,
            3 => shell.core.session.as_mut().unwrap().d_cookie.push('x'),
            4 => shell.slack_lists.comments.open = false,
            5 => {
                shell
                    .slack_lists
                    .comments
                    .entry
                    .as_mut()
                    .unwrap()
                    .owner
                    .serial = next_id()
            }
            _ => {
                shell
                    .slack_lists
                    .comments
                    .entry
                    .as_mut()
                    .unwrap()
                    .deleted
                    .insert(TS.into());
            }
        }
        assert!(!source.current(&shell), "mutation {mutation}");
    }
}

#[test]
fn actual_equal_body_frame_retires_original_but_foreign_reaction_and_replied_frames_do_not() {
    let (mut shell, source, policy) = setup();
    let record = intent(&shell, &source, &policy).record().clone();
    install(&mut shell, source.clone(), record);
    let event = changed(
        json!({"ts":TS,"thread_ts":source.target.root_ts(),"blocks":source.original.blocks}),
    );
    observe_event(&mut shell, "OTHER", 7, &event);
    observe_event(&mut shell, "T1", 8, &event);
    observe_event(
        &mut shell,
        "T1",
        7,
        &changed(json!({"ts":TS,"reactions":[]})),
    );
    let replied = super_platinum_core::slack::realtime::parse_event(&json!({"type":"message","subtype":"message_replied","channel":"CCOMMENT01","message":{"ts":source.target.root_ts(),"reply_count":7}}).to_string()).unwrap();
    observe_event(&mut shell, "T1", 7, &replied);
    assert!(source.current(&shell));
    observe_event(&mut shell, "T1", 7, &event);
    assert!(!source.current(&shell));
    assert!(!presentation(&shell).unwrap().can_confirm);
    assert_eq!(shell.slack_lists.comments.own_delete.events, 1);
}

#[test]
fn original_selected_or_root_deleted_event_retires_even_before_row_reduction() {
    for root in [false, true] {
        let (mut shell, source, policy) = setup();
        let record = intent(&shell, &source, &policy).record().clone();
        install(&mut shell, source.clone(), record);
        observe_event(
            &mut shell,
            "T1",
            7,
            &RtEvent::MessageDeleted {
                channel: source.target.channel_id().into(),
                deleted_ts: if root {
                    source.target.root_ts().unwrap().into()
                } else {
                    TS.into()
                },
            },
        );
        assert!(!source.current(&shell));
        assert!(source.owner_current(&shell));
    }
}

#[test]
fn presentation_once_only_gates_distinguish_busy_unknown_local_recovery_and_terminal() {
    let (mut shell, source, policy) = setup();
    assert!(available(&shell, source.owner.serial, TS));
    let record = intent(&shell, &source, &policy).record().clone();
    let id = install(&mut shell, source.clone(), record);
    assert!(!available(&shell, source.owner.serial, TS));
    assert!(presentation(&shell).unwrap().can_confirm);
    session_mut(&mut shell, id).unwrap().busy = true;
    let p = presentation(&shell).unwrap();
    assert!(!p.can_confirm && !p.can_check && !p.can_retry_local);
    session_mut(&mut shell, id).unwrap().busy = false;
    for phase in [DeletePhase::Pending, DeletePhase::Uncertain] {
        session_mut(&mut shell, id).unwrap().phase = Some(phase);
        let p = presentation(&shell).unwrap();
        assert!(!p.can_confirm && p.can_check && !p.terminal);
    }
    session_mut(&mut shell, id).unwrap().local_pending = true;
    let p = presentation(&shell).unwrap();
    assert!(p.can_retry_local && !p.can_confirm && !p.can_check);
    session_mut(&mut shell, id).unwrap().local_pending = false;
    session_mut(&mut shell, id).unwrap().phase = Some(DeletePhase::Confirmed);
    let p = presentation(&shell).unwrap();
    assert!(p.terminal && !p.can_confirm && !p.can_check);
    session_mut(&mut shell, id).unwrap().phase = Some(DeletePhase::Rejected);
    session_mut(&mut shell, id).unwrap().retry_at = now().max(0) as u64 + 60;
    assert!(!presentation(&shell).unwrap().can_confirm);
    session_mut(&mut shell, id).unwrap().retry_at = 0;
    assert!(presentation(&shell).unwrap().can_confirm);
    assert!(session(&shell, id.wrapping_add(1)).is_none());
}

#[test]
fn scoped_removal_clears_all_copies_keeps_raw_and_prevents_old_read_resurrection() {
    for preflight in [false, true] {
        let (mut shell, source, _) = setup();
        let before = shell.slack_lists.comments.entry.as_ref().unwrap().clone();
        let e = shell.slack_lists.comments.entry.as_mut().unwrap();
        e.accepted.push(source.original.clone());
        e.live.push(source.original.clone());
        e.reveal = Some((1, TS.into()));
        let (owner, read, _, cursor) = if preflight {
            controller::begin_preflight(&mut shell).unwrap()
        } else {
            controller::begin(&mut shell, false).unwrap()
        };
        assert!(deletions::remove_scoped(
            &mut shell,
            &owner,
            &source.target,
            TS
        ));
        assert_eq!(owner.reading(&shell, read), preflight);
        let e = shell.slack_lists.comments.entry.as_ref().unwrap();
        assert_eq!(
            e.rows.iter().map(|row| row.ts.as_str()).collect::<Vec<_>>(),
            vec![source.target.root_ts().unwrap()]
        );
        assert!(e.accepted.is_empty() && e.live.is_empty() && e.deleted.contains(TS));
        assert_eq!(
            e.page.as_ref().unwrap().raw(),
            before.page.as_ref().unwrap().raw()
        );
        assert_eq!(e.page.as_ref().unwrap().cursor(), Some("next"));
        assert!(e.reveal.is_none());
        let rows = e.rows.clone();
        let retained_read = e.read;
        assert!(!deletions::remove_scoped(
            &mut shell,
            &owner,
            &source.target,
            TS
        ));
        assert!(Arc::ptr_eq(
            &rows,
            &shell.slack_lists.comments.entry.as_ref().unwrap().rows
        ));
        assert_eq!(
            shell.slack_lists.comments.entry.as_ref().unwrap().read,
            retained_read
        );
        if !preflight {
            controller::finish(
                &mut shell,
                &owner,
                read,
                cursor,
                Ok(network::Loaded {
                    page: before.page.unwrap(),
                    projection: before.projection.unwrap(),
                }),
            );
            let retained = shell.slack_lists.comments.entry.as_ref().unwrap();
            assert!(Arc::ptr_eq(&rows, &retained.rows));
            assert_eq!(
                retained
                    .rows
                    .iter()
                    .map(|row| row.ts.as_str())
                    .collect::<Vec<_>>(),
                vec![source.target.root_ts().unwrap()]
            );
            assert!(retained.deleted.contains(TS));
        }
    }
}

#[test]
fn scoped_removal_refuses_root_or_old_owner_and_preserves_independent_reply() {
    let (mut shell, binding, _local) = draft::tests::setup();
    load(&mut shell);
    let e = shell.slack_lists.comments.entry.as_ref().unwrap();
    let owner = e.owner.clone();
    let target = e.target.clone();
    let before = draft::entry(&shell, binding.id()).unwrap().clone();
    assert!(!deletions::remove_scoped(
        &mut shell,
        &owner,
        &target,
        target.root_ts().unwrap()
    ));
    assert!(deletions::remove_scoped(&mut shell, &owner, &target, TS));
    let after = draft::entry(&shell, binding.id()).unwrap();
    assert_eq!(after.blocks, before.blocks);
    assert_eq!(after.capture, before.capture);
    assert_eq!(after.generation, before.generation);
    assert_eq!(
        serde_json::to_value(&after.inventory).unwrap(),
        serde_json::to_value(&before.inventory).unwrap()
    );
    assert!(binding.current(&shell));
    let (mut shell, source, _) = setup();
    shell.core.account_epoch += 1;
    assert!(!deletions::remove_scoped(
        &mut shell,
        &source.owner,
        &source.target,
        TS
    ));
    assert_eq!(
        shell
            .slack_lists
            .comments
            .entry
            .as_ref()
            .unwrap()
            .rows
            .len(),
        1
    );
}

#[test]
fn issued_429_without_retry_after_uses_bounded_default() {
    let before = now();
    let deadline = retry_deadline(&Error::RateLimited {
        retry_after_secs: None,
    });
    assert!(deadline >= before + 30 && deadline <= now() + 30);
    assert_eq!(retry_deadline(&Error::Api("other".into())), 0);
}
