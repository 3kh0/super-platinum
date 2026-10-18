use super::*;
use serde_json::json;

pub(super) fn acquire(s: &mut ShellState, id: u64, channels: Vec<Value>) -> Authority {
    let ctx = context(s, id).unwrap();
    s.core
        .session
        .as_mut()
        .unwrap()
        .unified_user_client_enabled
        .insert(ctx.session.team_id.clone(), false);
    let raw = json!({"ok":true,
        "team":{"id":ctx.session.team_id,"plan":"plus"},
        "self":{"id":ctx.session.user_id,"team_id":ctx.session.team_id,"is_admin":false},
        "workspaces":[{"id":ctx.session.team_id}],
        "read_only_channels":[],"thread_only_channels":[],"channels":channels});
    let read = BootRead::begin(&mut s.core, &ctx.transport, &ctx.session).unwrap();
    assert!(read.apply(&mut s.core, raw, false).unwrap());
    let authority = Authority::capture(&s.core, &ctx.session.team_id).unwrap();
    let generation = active(s, id).unwrap().share_generation;
    let witnesses = authority
        .current_raw_channels(&s.core)
        .unwrap()
        .into_iter()
        .map(|raw| {
            (
                raw.value()["id"].as_str().unwrap().to_owned(),
                Witness {
                    generation,
                    authority: authority.clone(),
                    raw,
                },
            )
        })
        .collect();
    active_mut(s, id).unwrap().posting.channels = witnesses;
    authority
}
fn raw() -> Value {
    json!({"id":"CCAL","is_channel":true,"name":"Selected", "is_member":false})
}
fn selected() -> (ShellState, u64, Authority) {
    let (mut s, id) = super::super::tests::setup();
    let c = active_mut(&mut s, id).unwrap();
    c.share = true;
    c.share_channel = "CCAL".into();
    c.share_selected = Some(sharing::Destination::Channel(
        serde_json::from_value(raw()).unwrap(),
    ));
    c.share_choices.push(cal::Choice {
        id: "CCAL".into(),
        name: "Selected".into(),
    });
    let a = acquire(&mut s, id, vec![raw()]);
    (s, id, a)
}
#[test]
fn cached_or_search_channel_cannot_grant_posting_without_actual_acquisition() {
    let (mut s, id) = super::super::tests::setup();
    let ctx = context(&s, id).unwrap();
    let candidate: Channel = serde_json::from_value(raw()).unwrap();
    assert!(sharing::channel_base(&s, &ctx, &candidate));
    assert!(!sharing::channel(&s, &ctx, &candidate));
    acquire(&mut s, id, vec![raw()]);
    assert!(sharing::channel(&s, &ctx, &candidate));
    // An actual independently retained nonmembership does not substitute for
    // root posting policy in either direction (Calendar's onlyIfMember=false).
    assert_eq!(candidate.extra.get("is_member"), Some(&json!(false)));
}
#[test]
fn actual_effective_flags_and_restrictions_reach_final_native_capture() {
    for (restriction, expected) in [
        (json!({"role":["all"]}), true),
        (json!({"role":["admin"]}), false),
        (json!({}), false),
    ] {
        let (mut s, id, _) = selected();
        let mut value = raw();
        value["properties"] = json!({
            "posting_restricted_to":restriction,"threads_restricted_to":{}});
        acquire(&mut s, id, vec![value]);
        assert_eq!(native::Snapshot::capture(&s, id).is_some(), expected);
    }
    let (mut s, id, a) = selected();
    let before = native::Snapshot::capture(&s, id).unwrap();
    let read = a.begin_channel_read(&mut s.core, "CCAL").unwrap();
    assert!(native::Snapshot::capture(&s, id).is_none());
    let mut changed = raw();
    changed["is_read_only"] = json!(true);
    changed["is_thread_only"] = json!(false);
    read.apply(&mut s.core, changed.clone()).unwrap();
    assert!(native::Snapshot::capture(&s, id).as_ref() != Some(&before));
    acquire(&mut s, id, vec![changed]);
    assert!(native::Snapshot::capture(&s, id).is_none());
    acquire(&mut s, id, vec![raw()]);
    assert!(native::Snapshot::capture(&s, id).is_some());
}
#[test]
fn selected_witness_retires_on_query_config_credentials_owner_and_raw_changes() {
    for mutation in 0..7 {
        let (mut s, id, _) = selected();
        assert!(actions::event(&s, id).is_some());
        match mutation {
            0 => active_mut(&mut s, id).unwrap().share_generation += 1,
            1 => {
                s.core
                    .session
                    .as_mut()
                    .unwrap()
                    .unified_user_client_enabled
                    .insert("T1".into(), true);
            }
            2 => s
                .core
                .session
                .as_mut()
                .unwrap()
                .workspaces
                .get_mut("T1")
                .unwrap()
                .token
                .push('x'),
            3 => s.core.account_epoch += 1,
            4 => s.channel_generation += 1,
            5 => {
                s.core.workspaces.get_mut("T1").unwrap().channels.insert(
                    "CCAL".into(),
                    serde_json::from_value(
                        json!({"id":"CCAL","is_channel":true,"is_archived":true}),
                    )
                    .unwrap(),
                );
            }
            _ => active_mut(&mut s, id).unwrap().posting.invalidate(),
        }
        assert!(actions::event(&s, id).is_none(), "mutation {mutation}");
        assert!(s.message_buttons.shortcuts.calendar_attempts.is_empty());
    }
}
#[test]
fn late_actual_channel_read_cannot_replace_a_newer_reserved_witness() {
    let (mut s, id, a) = selected();
    let old = a.begin_channel_read(&mut s.core, "CCAL").unwrap();
    let new = a.begin_channel_read(&mut s.core, "CCAL").unwrap();
    let witness = new.apply(&mut s.core, raw()).unwrap();
    assert!(old.apply(&mut s.core, raw()).is_err());
    let generation = active(&s, id).unwrap().share_generation;
    active_mut(&mut s, id).unwrap().posting.channels.insert(
        "CCAL".into(),
        Witness {
            generation,
            authority: a,
            raw: witness,
        },
    );
    assert!(actions::event(&s, id).is_some());
}
#[test]
fn candidate_ranking_is_bounded_and_selected_survives_unmatched_query() {
    let (mut s, id, _) = selected();
    let ctx = context(&s, id).unwrap();
    active_mut(&mut s, id).unwrap().share_query = "no-match".into();
    assert_eq!(candidates(&s, id, &ctx, &[]), vec!["CCAL"]);
    active_mut(&mut s, id).unwrap().share_query.clear();
    let remote =
        (0..150)
            .map(|i| {
                sharing::Destination::Channel(serde_json::from_value(
        json!({"id":format!("C{i:04}"),"is_channel":true,"name":format!("A{i:04}")})).unwrap())
            })
            .collect::<Vec<_>>();
    let rows = candidates(&s, id, &ctx, &remote);
    assert_eq!(rows.len(), 100);
    assert_eq!(rows[0], "CCAL");
}
#[test]
fn actual_read_refuses_retired_query_or_exhausted_budget_before_transport() {
    use std::{
        cell::Cell,
        rc::Rc,
        task::{Context as TaskContext, Poll, Wake, Waker},
    };
    struct NoWake;
    impl Wake for NoWake {
        fn wake(self: Arc<Self>) {}
    }
    type Slot = Rc<Cell<Option<Signal<ShellState>>>>;
    let slot = Slot::default();
    let mut dom = VirtualDom::new_with_props(
        |slot: Slot| {
            let state = use_signal(|| super::super::tests::setup().0);
            slot.set(Some(state));
            rsx! {div{}}
        },
        slot.clone(),
    );
    dom.rebuild_in_place();
    let mut state = slot.get().unwrap();
    let id = state
        .peek()
        .message_buttons
        .shortcuts
        .dialog
        .as_ref()
        .unwrap()
        .id;
    let ctx = context(&state.peek(), id).unwrap();
    for retired in [false, true] {
        let mut query = Query {
            id,
            generation: 0,
            ctx: ctx.clone(),
            remaining: if retired { 1 } else { 0 },
        };
        if retired {
            active_mut(&mut state.write(), id).unwrap().share_generation = 1;
        }
        let request = api::conversations_info(&ctx.client, &ctx.session, "CCAL".into());
        let waker = Waker::from(Arc::new(NoWake));
        let mut cx = TaskContext::from_waker(&waker);
        let mut future = std::pin::pin!(query.read(state, request));
        assert!(matches!(
            std::future::Future::poll(future.as_mut(), &mut cx),
            Poll::Ready(Err(_))
        ));
    }
}
