use super::*;
use serde_json::json;
#[path = "tests/family.rs"]
mod family_tests;
pub(super) const TS: &str = "1700000001.000001";
const ROOT: &str = "1700000000.000001";
fn root(target: &Target) -> Value {
    json!({"type":"message","ts":ROOT,"subtype":"list_record_comment","slack_list":{"list_id":target.list_id(),"list_record_id":target.record_id()}})
}
pub(super) fn raw(users: Vec<String>) -> Value {
    json!({"type":"message","ts":TS,"thread_ts":ROOT,"user":"U_OTHER","text":"Original body",
    "reactions":[{"name":"wave","count":users.len(),"users":users}]})
}
pub(super) fn load(shell: &mut ShellState, row: Value) {
    let (owner, serial, target, cursor) = controller::begin(shell, false).unwrap();
    controller::finish(shell,&owner,serial,cursor,Ok(network::Loaded{
        page:Page::parse(&target,json!({"ok":true,"messages":[root(&target),row],"has_more":true,"response_metadata":{"next_cursor":"next"}})).unwrap(),
        projection:Projection::observed(json!({"ok":true,"assignments":{}})).unwrap(),
    }));
}
pub(super) fn setup() -> ShellState {
    let mut s = super::super::tests::shell();
    s.core.workspaces.get_mut("T1").unwrap().rt_generation = 7;
    load(&mut s, raw(vec!["U_OTHER".into()]));
    s
}
pub(super) fn generation(s: &ShellState) -> u64 {
    s.slack_lists.comments.entry.as_ref().unwrap().owner.serial
}
fn start(s: &mut ShellState) -> Work {
    let generation = generation(s);
    begin(s, generation, TS, "wave").unwrap()
}
fn dispatch(s: &mut ShellState, work: &Work) {
    let entry = s.slack_lists.comments.entry.as_ref().unwrap();
    let page = entry.page.as_ref().unwrap();
    let intent = ReactionIntent::capture(
        &entry.target,
        page.root().unwrap(),
        message(entry, TS).unwrap(),
        json!({"id":entry.target.channel_id()}),
        &work.user,
        "wave",
    )
    .unwrap();
    assert_eq!(intent.record().adding(), work.adding);
    work.update(|a| {
        a.record = Some(intent.record().clone());
        a.phase = Phase::Sent;
    });
    s.slack_lists.comments.entry.as_mut().unwrap().loading = false;
}
fn own(s: &ShellState, user: &str) -> bool {
    member(
        message(s.slack_lists.comments.entry.as_ref().unwrap(), TS).unwrap(),
        user,
        "wave",
    )
}
fn socket(s: &mut ShellState, work: &Work, add: bool) -> bool {
    let event = if add {
        RtEvent::ReactionAdded {
            channel: work.target.channel_id().into(),
            ts: TS.into(),
            user: work.user.clone(),
            reaction: "wave".into(),
        }
    } else {
        RtEvent::ReactionRemoved {
            channel: work.target.channel_id().into(),
            ts: TS.into(),
            user: work.user.clone(),
            reaction: "wave".into(),
        }
    };
    reactions::apply(s, "T1", 7, &event)
}
#[test]
fn invocation_uses_current_membership_and_blocks_duplicate_pending_pill() {
    let mut s = setup();
    let work = start(&mut s);
    assert!(work.adding);
    assert!(!available(&s, generation(&s), TS, "wave"));
    dispatch(&mut s, &work);
    assert!(!available(&s, generation(&s), TS, "wave"));
    settle(&work, Ok(json!({"ok":true})));
    finish(&mut s, &work);
    assert!(own(&s, &work.user));
    assert!(!work.attempt().unwrap().socket_observed);
    let next = start(&mut s);
    assert!(!next.adding);
}
#[test]
fn confirmed_delta_cancels_crossing_page_read_without_changing_raw_body_or_draft() {
    let (mut s, binding, _local) = draft::tests::setup();
    s.core.workspaces.get_mut("T1").unwrap().rt_generation = 7;
    load(&mut s, raw(vec!["U_OTHER".into()]));
    let draft_state = |s: &ShellState| {
        let e = draft::entry(s, binding.id()).unwrap();
        json!({"blocks":e.blocks,"generation":e.generation,"capture":e.capture,"sequence":e.sequence,
        "inventory":e.inventory.as_ref().map(|i|(i.blocks(),i.revision(),i.attempt()))})
    };
    let before = draft_state(&s);
    let work = start(&mut s);
    dispatch(&mut s, &work);
    let page = s
        .slack_lists
        .comments
        .entry
        .as_ref()
        .unwrap()
        .page
        .clone()
        .unwrap();
    let (owner, serial, _, _) = controller::begin(&mut s, false).unwrap();
    settle(&work, Ok(json!({"ok":true})));
    finish(&mut s, &work);
    assert!(!owner.reading(&s, serial));
    assert!(own(&s, &work.user));
    let entry = s.slack_lists.comments.entry.as_ref().unwrap();
    assert_eq!(entry.page.as_ref().unwrap().raw(), page.raw());
    assert_eq!(entry.page.as_ref().unwrap().cursor(), page.cursor());
    assert_eq!(draft_state(&s), before);
}
#[test]
fn duplicate_own_socket_then_opposite_wins_over_late_ack() {
    let mut s = setup();
    let work = start(&mut s);
    dispatch(&mut s, &work);
    assert!(socket(&mut s, &work, true));
    assert!(!socket(&mut s, &work, true));
    assert!(socket(&mut s, &work, false));
    settle(&work, Ok(json!({"ok":true})));
    finish(&mut s, &work);
    assert!(!own(&s, &work.user));
    let a = work.attempt().unwrap();
    assert!(a.socket_observed && !a.published);
    assert_eq!(a.socket_revision, 3);
    assert!(
        status(&s, generation(&s))
            .unwrap()
            .contains("Newer comment updates")
    );
}
#[test]
fn socket_between_ack_receipt_and_native_publication_is_also_respected() {
    let mut s = setup();
    let work = start(&mut s);
    dispatch(&mut s, &work);
    settle(&work, Ok(json!({"ok":true})));
    assert!(socket(&mut s, &work, true));
    assert!(socket(&mut s, &work, false));
    finish(&mut s, &work);
    assert!(!own(&s, &work.user));
    assert!(work.attempt().unwrap().socket_observed);
}
#[test]
fn later_canonical_membership_prevents_earlier_ack_from_repainting() {
    let mut s = setup();
    let work = start(&mut s);
    dispatch(&mut s, &work);
    load(&mut s, raw(vec!["U_OTHER".into()]));
    settle(&work, Ok(json!({"ok":true})));
    finish(&mut s, &work);
    assert!(!own(&s, &work.user));
    assert!(work.attempt().unwrap().canonical_observed);
}
#[test]
fn stale_owner_socket_and_deletion_never_publish_but_keep_raw_receipt() {
    for mode in 0..3 {
        let mut s = setup();
        let work = start(&mut s);
        dispatch(&mut s, &work);
        match mode {
            0 => s.core.account_epoch += 1,
            1 => s.core.workspaces.get_mut("T1").unwrap().rt_generation += 1,
            _ => {
                s.slack_lists
                    .comments
                    .entry
                    .as_mut()
                    .unwrap()
                    .deleted
                    .insert(TS.into());
            }
        }
        let raw = json!({"ok":true,"opaque":[1,2]});
        settle(&work, Ok(raw.clone()));
        finish(&mut s, &work);
        assert!(!own(&s, &work.user));
        assert_eq!(work.attempt().unwrap().raw, Some(raw));
        assert!(!work.attempt().unwrap().published);
    }
}
#[test]
fn unknown_survives_close_reload_and_only_positive_readback_resolves() {
    let mut s = setup();
    let work = start(&mut s);
    dispatch(&mut s, &work);
    settle(&work, Ok(json!({"ok":false,"error":"internal_error"})));
    finish(&mut s, &work);
    assert_eq!(work.attempt().unwrap().phase.label(), "unknown");
    let mut loaded = setup();
    carry_pending(&mut s, &mut loaded);
    assert!(!available(&loaded, generation(&loaded), TS, "wave"));
    load(&mut loaded, raw(vec!["U_OTHER".into()]));
    assert_eq!(work.attempt().unwrap().phase.label(), "unknown");
    load(&mut loaded, raw(vec!["U_OTHER".into(), work.user.clone()]));
    let a = work.attempt().unwrap();
    assert_eq!(a.phase.label(), "observed");
    assert!(a.readback);
    assert_eq!(a.raw.unwrap()["error"], "internal_error");
}
#[test]
fn exact_rejection_and_notrequested_allow_retry_but_unknown_and_lost_do_not() {
    for (result, blocked) in [
        (Ok(json!({"ok":false,"error":"invalid_name"})), false),
        (
            Err(ListCommentReactionFailure::NotRequested(unavailable())),
            false,
        ),
        (Ok(json!({"ok":false,"error":"unknown_error"})), true),
        (
            Err(ListCommentReactionFailure::Unconfirmed(unavailable())),
            true,
        ),
    ] {
        let mut s = setup();
        let work = start(&mut s);
        dispatch(&mut s, &work);
        settle(&work, result);
        finish(&mut s, &work);
        assert_eq!(!available(&s, generation(&s), TS, "wave"), blocked);
        assert!(!own(&s, &work.user));
    }
}
#[test]
fn pending_status_has_priority_over_later_success_without_blocking_unrelated_pill() {
    let mut s = setup();
    let work = start(&mut s);
    dispatch(&mut s, &work);
    settle(
        &work,
        Err(ListCommentReactionFailure::Unconfirmed(unavailable())),
    );
    finish(&mut s, &work);
    let mut newer = work.attempt().unwrap();
    newer.id = next_id();
    newer.name = "other".into();
    newer.phase = Phase::Confirmed;
    s.slack_lists
        .comments
        .reaction_actions
        .0
        .lock()
        .unwrap()
        .push(newer);
    assert!(status(&s, generation(&s)).unwrap().contains("unconfirmed"));
}
#[test]
fn full_readback_preserves_body_raw_and_updates_last_pill_without_ack_claim() {
    let mut s = setup();
    let user = s
        .slack_lists
        .comments
        .entry
        .as_ref()
        .unwrap()
        .owner
        .scope
        .stable_owner()
        .2;
    load(&mut s, raw(vec![user]));
    let work = start(&mut s);
    assert!(!work.adding);
    dispatch(&mut s, &work);
    settle(
        &work,
        Err(ListCommentReactionFailure::Unconfirmed(unavailable())),
    );
    finish(&mut s, &work);
    let before = s
        .slack_lists
        .comments
        .entry
        .as_ref()
        .unwrap()
        .page
        .clone()
        .unwrap();
    let full = json!({"ok":true,"type":"message","channel":"CCOMMENT01","message":{"ts":TS,"thread_ts":ROOT,"text":"Do not replace body","reactions":[]}});
    let observation = recovery::Observation {
        id: work.id,
        revision: 0,
        deadline: 0,
        raw: Some(full.clone()),
    };
    apply_readback(&mut s, &work.owner, &[observation]);
    assert!(!own(&s, &work.user));
    let e = s.slack_lists.comments.entry.as_ref().unwrap();
    assert!(message(e, TS).unwrap().reactions.is_empty());
    assert_eq!(
        message(e, TS).unwrap().text.as_deref(),
        Some("Original body")
    );
    assert_eq!(e.page.as_ref().unwrap().raw(), before.raw());
    let a = work.attempt().unwrap();
    assert!(a.readback);
    assert_eq!(a.phase.label(), "observed");
    assert!(a.raw.is_none());
    assert_eq!(a.full_readback, Some(full));
}
#[test]
fn full_reaction_observation_is_refused_after_any_newer_socket_frame() {
    let mut s = setup();
    let work = start(&mut s);
    dispatch(&mut s, &work);
    settle(
        &work,
        Err(ListCommentReactionFailure::Unconfirmed(unavailable())),
    );
    finish(&mut s, &work);
    let observations = [recovery::Observation {
        id: work.id,
        revision: 0,
        deadline: 0,
        raw: Some(
            json!({"ok":true,"type":"message","channel":"CCOMMENT01","message":{"ts":TS,"reactions":[]}}),
        ),
    }];
    assert!(socket(&mut s, &work, true));
    assert!(!readback_current(&s, &observations));
    apply_readback(&mut s, &work.owner, &observations);
    assert!(own(&s, &work.user));
    assert_eq!(work.attempt().unwrap().phase.label(), "unknown");
}
#[test]
fn failed_release_retires_only_original_preflight_and_preserves_draft() {
    let (mut s, binding, _local) = draft::tests::setup();
    load(&mut s, raw(vec!["U_OTHER".into()]));
    let before = draft::entry(&s, binding.id()).unwrap().blocks.clone();
    let work = start(&mut s);
    assert!(work.owner.reading(&s, work.serial));
    work.update(|a| a.phase = Phase::NotSent);
    retire_preflight(&mut s, &work);
    assert!(!work.owner.reading(&s, work.serial));
    assert!(!s.slack_lists.comments.entry.as_ref().unwrap().loading);
    assert_eq!(draft::entry(&s, binding.id()).unwrap().blocks, before);
    assert!(available(&s, generation(&s), TS, "wave"));
    let (_, successor, _, _) = controller::begin(&mut s, false).unwrap();
    retire_preflight(&mut s, &work);
    assert!(work.owner.reading(&s, successor));
}
#[test]
fn changed_clicked_membership_retires_predispatch_witness_without_new_send() {
    let mut s = setup();
    let work = start(&mut s);
    assert!(work.current(&s));
    assert!(socket(&mut s, &work, true));
    assert!(!work.current(&s));
    settle(
        &work,
        Err(ListCommentReactionFailure::NotRequested(unavailable())),
    );
    finish(&mut s, &work);
    assert!(own(&s, &work.user));
    assert_eq!(work.attempt().unwrap().phase.label(), "not-sent");
    let next = start(&mut s);
    assert!(!next.adding);
}
#[test]
fn full_readback_never_resolves_foreign_or_partial_receipt() {
    for patch in [json!({"channel":"CFOREIGN"}), json!({"message":{"ts":TS}})] {
        let mut s = setup();
        let work = start(&mut s);
        dispatch(&mut s, &work);
        settle(
            &work,
            Err(ListCommentReactionFailure::Unconfirmed(unavailable())),
        );
        finish(&mut s, &work);
        let mut full = json!({"ok":true,"type":"message","channel":"CCOMMENT01","message":{"ts":TS,"thread_ts":ROOT,"reactions":[{"name":"wave","count":1,"users":[work.user]}]}});
        full.as_object_mut()
            .unwrap()
            .extend(patch.as_object().unwrap().clone());
        let before = serde_json::to_value(
            message(s.slack_lists.comments.entry.as_ref().unwrap(), TS).unwrap(),
        )
        .unwrap();
        apply_readback(
            &mut s,
            &work.owner,
            &[recovery::Observation {
                id: work.id,
                revision: 0,
                deadline: 0,
                raw: Some(full),
            }],
        );
        assert_eq!(work.attempt().unwrap().phase.label(), "unknown");
        assert_eq!(
            serde_json::to_value(
                message(s.slack_lists.comments.entry.as_ref().unwrap(), TS).unwrap()
            )
            .unwrap(),
            before
        );
    }
}
#[test]
fn full_read_rate_limit_keeps_unknown_and_blocks_explicit_refresh_until_deadline() {
    for error in [
        Error::RateLimited {
            retry_after_secs: Some(60),
        },
        Error::HttpStatus {
            status: 429,
            retry_after_secs: Some(60),
        },
    ] {
        let mut s = setup();
        let work = start(&mut s);
        dispatch(&mut s, &work);
        settle(
            &work,
            Err(ListCommentReactionFailure::Unconfirmed(unavailable())),
        );
        finish(&mut s, &work);
        let before = now();
        let observation = recovery::Observation::capture(work.id, 0, Err(error));
        assert!(observation.deadline >= before + 60);
        assert!(observation.raw.is_none());
        let deadline = observation.deadline;
        apply_readback(&mut s, &work.owner, &[observation]);
        assert_eq!(cooldown(&s), deadline);
        assert_eq!(
            s.slack_lists.comments.entry.as_ref().unwrap().retry_at,
            deadline
        );
        assert!(controller::begin(&mut s, false).is_none());
        assert_eq!(work.attempt().unwrap().phase.label(), "unknown");
        assert!(!own(&s, &work.user));
    }
}
