use super::*;
use serde_json::json;
use super_platinum_core::{
    block_actions::workflow_management::member_hydration::{Flags, RendererReply},
    config::Session,
    state::Workspace,
};
fn shell() -> ShellState {
    let mut s = ShellState::login(crate::media::MediaRegistry::default());
    let make = |id: &str| WorkspaceSession {
        team_id: id.into(),
        enterprise_id: None,
        user_id: "U00000009".into(),
        name: id.into(),
        url: "https://example.invalid".into(),
        token: format!("synthetic-{id}"),
    };
    let auth = Session {
        d_cookie: "synthetic-cookie".into(),
        unified_user_client_enabled: BTreeMap::new(),
        workspaces: BTreeMap::from([("T1".into(), make("T1")), ("T2".into(), make("T2"))]),
    };
    s.core.active_account = Some("A".into());
    s.core.active_team = Some("T1".into());
    s.core.transport = Some(Arc::new(Transport::new("synthetic-cookie").unwrap()));
    s.core.workspaces.clear();
    for (team, owner) in &auth.workspaces {
        let mut w = Workspace::from_session(owner);
        w.directory_members
            .begin("A", owner, &BTreeSet::new())
            .unwrap();
        s.core.workspaces.insert(team.clone(), w);
    }
    s.core.session = Some(auth);
    s
}
fn harness() -> (VirtualDom, Signal<ShellState>) {
    type Slot = std::rc::Rc<std::cell::Cell<Option<Signal<ShellState>>>>;
    let slot = Slot::default();
    let mut dom = VirtualDom::new_with_props(
        |slot: Slot| {
            let state = use_signal(shell);
            slot.set(Some(state));
            rsx! {}
        },
        slot.clone(),
    );
    dom.rebuild_to_vec();
    (dom, slot.get().unwrap())
}
fn pair(mut state: Signal<ShellState>) -> (Subscription, Subscription, Arc<Dispatch>) {
    let mut s = state.write();
    let a = register(&mut s, "T1", &["U00000001".into()])
        .unwrap()
        .unwrap();
    let b = register(&mut s, "T1", &["U00000001".into()])
        .unwrap()
        .unwrap();
    let wake = arm(&mut s, &a, Instant::now()).unwrap().unwrap();
    let dispatch = dispatch(&mut s, &wake, wake.due()).unwrap();
    publish_observed_terminal(&mut s, dispatch.clone(), NetworkTerminal::Complete(vec![
        json!({"id":"U00000001","name":"Observed","profile":{"real_name":"Observed"},"is_stranger":false})
    ]), vec![], vec![]).unwrap();
    (a, b, dispatch)
}
fn elect(mut state: Signal<ShellState>, sub: &Subscription) -> Election {
    let (adoption, outcome) = claim_adoption(&mut state.write(), sub).unwrap().unwrap();
    Election {
        state,
        adoption: Some(adoption),
        outcome,
    }
}
fn stage(state: Signal<ShellState>, elected: &Election) -> Stage {
    elected.stage(&state.peek(),Flags::decode(&json!({"ok":true,"assignments":{}})).unwrap(),
        "query-one".into(),&super_platinum_core::block_actions::directory_endpoint::member_info::unknown::PlaceholderAsset::observed("https://a.slack-edge.com/bv1-13/").unwrap()).unwrap()
}
fn render(state: Signal<ShellState>, elected: &Election, stage: &mut Stage) {
    while let Some(pending) = stage.prepare(&state.peek(), elected).unwrap() {
        assert!(pending.fields().iter().all(|s| s.is_ascii()));
        let reply = RendererReply {
            generation: pending.generation().into(),
            operation: "ordinary-toLowerCase".into(),
            fields: pending.fields().to_vec(),
            lowered: pending
                .fields()
                .iter()
                .map(|s| s.to_ascii_lowercase())
                .collect(),
        };
        stage.apply(&state.peek(), elected, pending, reply).unwrap();
    }
}
#[test]
fn abandoned_private_renderer_stage_preserves_receipt_for_sibling_without_refetch() {
    let (dom, mut state) = harness();
    dom.in_runtime(|| {
        let (a, b, dispatch) = pair(state);
        let before = state.peek().core.workspaces["T1"]
            .directory_members
            .cached();
        let first = elect(state, &a);
        let mut private = stage(state, &first);
        render(state, &first, &mut private);
        assert!(state.peek().core.workspaces["T1"].users.is_empty());
        assert_eq!(
            state.peek().core.workspaces["T1"]
                .directory_members
                .cached(),
            before
        );
        drop(private);
        drop(first);
        retire(&mut state.write(), &a);
        let mut second = elect(state, &b);
        assert!(Arc::ptr_eq(&second.outcome.dispatch, &dispatch));
        let mut private = stage(state, &second);
        render(state, &second, &mut private);
        private
            .install(&mut state.write(), &mut second, |_| true)
            .unwrap();
        let s = state.peek();
        assert_eq!(status(&s, &b), Status::Complete);
        assert_eq!(
            s.core.workspaces["T1"].users["U00000001"].name.as_deref(),
            Some("Observed")
        );
        assert!(
            !s.core.workspaces["T1"]
                .directory_members
                .has_pending("A", &s.core.session.as_ref().unwrap().workspaces["T1"])
                .unwrap()
        );
    });
}
#[test]
fn stale_query_or_native_drift_installs_neither_half_and_cannot_settle() {
    for native in [false, true] {
        let (dom, mut state) = harness();
        dom.in_runtime(|| {
            let (a, _b, _) = pair(state);
            let mut elected = elect(state, &a);
            let mut private = stage(state, &elected);
            render(state, &elected, &mut private);
            if native {
                state
                    .write()
                    .core
                    .workspaces
                    .get_mut("T1")
                    .unwrap()
                    .users
                    .insert(
                        "U00000002".into(),
                        serde_json::from_value(json!({"id":"U00000002"})).unwrap(),
                    );
            }
            let before = state.peek().core.workspaces["T1"]
                .directory_members
                .cached();
            let native_before =
                serde_json::to_value(&state.peek().core.workspaces["T1"].users).unwrap();
            assert!(
                private
                    .install(&mut state.write(), &mut elected, |_| native)
                    .is_err()
            );
            assert_eq!(
                before,
                state.peek().core.workspaces["T1"]
                    .directory_members
                    .cached()
            );
            assert_eq!(
                native_before,
                serde_json::to_value(&state.peek().core.workspaces["T1"].users).unwrap()
            );
            assert_eq!(status(&state.peek(), &a), Status::Pending);
        });
    }
}
#[test]
fn dropped_client_releases_adoption_but_preserves_a_siblings_completed_receipt() {
    let (dom, state) = harness();
    dom.in_runtime(|| {
        let (a, b, _) = pair(state);
        let first = elect(state, &a);
        let client = Client {
            state,
            subscription: Some(a),
        };
        drop(client);
        assert!(!first.current(&state.peek()));
        let sibling = elect(state, &b);
        assert!(sibling.current(&state.peek()));
        drop(first); // old lease cannot release the new sibling lease
        assert!(sibling.current(&state.peek()));
    });
}
#[test]
fn shared_rate_limit_survives_consumer_retirement_and_preserves_original_error() {
    let (dom, mut state) = harness();
    dom.in_runtime(|| {
        let (a, b, dispatch) = pair(state);
        retire(&mut state.write(), &a);
        let error = Arc::new(Error::RateLimited {
            retry_after_secs: Some(17),
        });
        retain_failure(&mut state.write(), &dispatch, error.clone());
        let s = state.peek();
        let slot = s.directory_member_fetches.slot(&dispatch.context).unwrap();
        assert!(slot.retry_at.unwrap().unwrap() > Instant::now());
        assert!(Arc::ptr_eq(slot.last_failure.as_ref().unwrap(), &error));
        assert_eq!(status(&s, &b), Status::Pending);
    });
}

#[tokio::test]
async fn actual_root_spawn_survives_subscriber_component_unmount_without_a_second_task() {
    #[derive(Clone)]
    struct Control(
        std::rc::Rc<
            std::cell::RefCell<(
                Option<Signal<bool>>,
                Option<tokio::sync::oneshot::Receiver<()>>,
                Option<tokio::sync::oneshot::Sender<()>>,
            )>,
        >,
    );
    impl PartialEq for Control {
        fn eq(&self, other: &Self) -> bool {
            std::rc::Rc::ptr_eq(&self.0, &other.0)
        }
    }
    #[component]
    fn Child(control: Control) -> Element {
        use_hook(move || {
            let (_, receive, send) = &mut *control.0.borrow_mut();
            let receive = receive.take().unwrap();
            let send = send.take().unwrap();
            spawn_root(async move {
                receive.await.unwrap();
                send.send(()).unwrap();
            });
        });
        rsx! {}
    }
    let (release, receive) = tokio::sync::oneshot::channel();
    let (send, mut completed) = tokio::sync::oneshot::channel();
    let control = Control(std::rc::Rc::new(std::cell::RefCell::new((
        None,
        Some(receive),
        Some(send),
    ))));
    let mut dom = VirtualDom::new_with_props(
        |control: Control| {
            let visible = use_signal(|| true);
            control.0.borrow_mut().0 = Some(visible);
            rsx! { if visible() { Child { control } } }
        },
        control.clone(),
    );
    dom.rebuild_to_vec();
    // Give the root task a poll before its creating component is removed.
    dom.render_immediate_to_vec();
    let mut visible = control.0.borrow().0.unwrap();
    dom.in_runtime(|| visible.set(false));
    dom.render_immediate_to_vec();
    release.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            tokio::select! {
                result=&mut completed => { result.unwrap(); break; }
                _=dom.wait_for_work()=>{dom.render_immediate_to_vec();}
            }
        }
    })
    .await
    .unwrap();
}

fn mixed_cooldown(pending_round: bool) {
    let (dom, mut state) = harness();
    dom.in_runtime(|| {
        let ids: Vec<_> = (1..=if pending_round { 101 } else { 201 })
            .map(|n| format!("U{n:08}"))
            .collect();
        let (a, b, dispatched) = {
            let mut shell = state.write();
            let a = register(&mut shell, "T1", &ids).unwrap().unwrap();
            let b = register(&mut shell, "T1", &ids).unwrap().unwrap();
            let wake = arm(&mut shell, &a, Instant::now()).unwrap().unwrap();
            let dispatched = dispatch(&mut shell, &wake, wake.due()).unwrap();
            (a, b, dispatched)
        };
        let deferred = ids[if pending_round { 99 } else { 200 }].clone();
        let (received, original) = read::mixed_cooldown_fixture(state, &dispatched, pending_round);
        let completed = received.completed.clone();
        publish_observed_completed(
            &mut state.write(),
            dispatched,
            received.completed,
            received.terminal,
            received.pages,
            received.errors,
        )
        .unwrap();
        let first = elect(state, &a);
        assert_eq!(first.outcome.dispatch.token.ids(), completed);
        assert!(Arc::ptr_eq(&first.outcome.errors[0], &original));
        let mut private = stage(state, &first);
        render(state, &first, &mut private);
        // Retirement during rendering leaves the exact completed subset for a
        // sibling. Canonical source/native remain untouched until installation.
        drop(private);
        drop(first);
        retire(&mut state.write(), &a);
        assert!(state.peek().core.workspaces["T1"].users.is_empty());
        let mut second = elect(state, &b);
        assert!(Arc::ptr_eq(&second.outcome.errors[0], &original));
        let mut private = stage(state, &second);
        render(state, &second, &mut private);
        private
            .install(&mut state.write(), &mut second, |_| true)
            .unwrap();
        {
            let shell = state.peek();
            assert_eq!(status(&shell, &b), Status::Pending);
            assert!(!shell.core.workspaces["T1"].users.contains_key(&deferred));
            assert_eq!(shell.core.workspaces["T1"].users.len(), completed.len());
            let positive = &ids[if pending_round { 0 } else { 100 }];
            assert_eq!(
                shell.core.workspaces["T1"].users[positive].name.as_deref(),
                Some("Observed")
            );
            let slot = shell.directory_member_fetches.slot(&b.context).unwrap();
            assert!(Arc::ptr_eq(slot.last_failure.as_ref().unwrap(), &original));
        }
        // Root's one wake will wait for this deadline; inspect exact requeue
        // without a real timer/network request or removing the cooldown.
        let mut shell = state.write();
        let retry = shell
            .directory_member_fetches
            .slot(&b.context)
            .unwrap()
            .retry_at
            .unwrap()
            .unwrap();
        let wake = arm(&mut shell, &b, retry).unwrap().unwrap();
        let next = dispatch(&mut shell, &wake, wake.due()).unwrap();
        assert_eq!(next.capture().ids(), [deferred.clone()]);
        assert_eq!(next.token.ids(), [deferred]);
    });
}
#[test]
fn two_hundred_one_ids_keep_successes_original_429_and_unsent_id_for_sibling() {
    mixed_cooldown(false);
}
#[test]
fn pending_round_keeps_successes_original_429_and_pending_id_for_sibling() {
    mixed_cooldown(true);
}

#[test]
fn accepted_direct_source_is_no_longer_absent_even_if_invalidated() {
    let (dom, mut state) = harness();
    dom.in_runtime(|| {
        let (a, _, _) = pair(state);
        let mut elected = elect(state, &a);
        let mut private = stage(state, &elected);
        render(state, &elected, &mut private);
        private
            .install(&mut state.write(), &mut elected, |_| true)
            .unwrap();
        state
            .write()
            .core
            .workspaces
            .get_mut("T1")
            .unwrap()
            .directory_members
            .invalidate("U00000001");
        // Known/stale recovery is a separate unconditional producer. The
        // missing-member selector must never delete a record to fetch it.
        assert!(
            register(&mut state.write(), "T1", &["U00000001".into()])
                .unwrap()
                .is_none()
        );
        assert_eq!(
            state.peek().core.workspaces["T1"].users["U00000001"]
                .name
                .as_deref(),
            Some("Observed")
        );
    });
}

#[test]
fn closed_file_reopen_no_work_requires_actual_settled_member_source() {
    let (dom, mut state) = harness();
    dom.in_runtime(|| {
        let ids = vec!["U00000001".into(), "U00000001".into()];
        assert!(settled_without_member_io(&state.peek(), "T1", &ids).is_err());
        let (a, _, _) = pair(state);
        let mut elected = elect(state, &a);
        let mut private = stage(state, &elected);
        render(state, &elected, &mut private);
        // A completed HTTP receipt/private renderer is not canonical settlement.
        assert!(settled_without_member_io(&state.peek(), "T1", &ids).is_err());
        private
            .install(&mut state.write(), &mut elected, |_| true)
            .unwrap();
        let shell = state.peek();
        let source_before = shell.core.workspaces["T1"].directory_members.cached();
        let native_before = serde_json::to_value(&shell.core.workspaces["T1"].users).unwrap();
        let slots = shell.directory_member_fetches.slots.len();
        assert!(settled_without_member_io(&shell, "T1", &ids).is_ok());
        assert!(
            settled_without_member_io(&shell, "T1", &["U00000001".into(), "UABSENT01".into()])
                .is_err()
        );
        assert_eq!(shell.directory_member_fetches.slots.len(), slots);
        assert_eq!(
            shell
                .directory_member_fetches
                .activity
                .started
                .load(Ordering::Relaxed),
            0
        );
        assert_eq!(
            shell
                .directory_member_fetches
                .activity
                .finished
                .load(Ordering::Relaxed),
            0
        );
        assert_eq!(
            shell.core.workspaces["T1"].directory_members.cached(),
            source_before
        );
        assert_eq!(
            serde_json::to_value(&shell.core.workspaces["T1"].users).unwrap(),
            native_before
        );
        drop(shell);
        assert!(register(&mut state.write(), "T1", &ids).is_ok_and(|v| v.is_none()));
    });
}

#[test]
fn closed_file_reopen_refuses_pending_invalidated_native_drift_and_foreign_source() {
    for mutation in 0..5 {
        let (dom, mut state) = harness();
        dom.in_runtime(|| {
            let (a, _, _) = pair(state);
            let mut elected = elect(state, &a);
            let mut private = stage(state, &elected);
            render(state, &elected, &mut private);
            private
                .install(&mut state.write(), &mut elected, |_| true)
                .unwrap();
            assert!(settled_without_member_io(&state.peek(), "T1", &["U00000001".into()]).is_ok());
            {
                let mut shell = state.write();
                let owner = shell.core.session.as_ref().unwrap().workspaces["T1"].clone();
                match mutation {
                    0 => shell
                        .core
                        .workspaces
                        .get_mut("T1")
                        .unwrap()
                        .directory_members
                        .invalidate("U00000001"),
                    1 => {
                        shell
                            .core
                            .workspaces
                            .get_mut("T1")
                            .unwrap()
                            .users
                            .get_mut("U00000001")
                            .unwrap()
                            .name = Some("Unaccepted drift".into())
                    }
                    2 => shell.core.active_account = Some("foreign".into()),
                    3 => shell
                        .core
                        .workspaces
                        .get_mut("T1")
                        .unwrap()
                        .directory_members
                        .enqueue(
                            "A",
                            &owner,
                            super_platinum_core::state::directory_members::Origin::DirectUpsert,
                            vec![json!({"id":"U00000001","name":"Pending","profile":{}})],
                            json!({}),
                        )
                        .unwrap(),
                    _ => {
                        shell
                            .core
                            .workspaces
                            .get_mut("T1")
                            .unwrap()
                            .users
                            .remove("U00000001");
                    }
                }
            }
            assert!(
                settled_without_member_io(&state.peek(), "T1", &["U00000001".into()]).is_err(),
                "mutation {mutation}"
            );
            assert_eq!(
                state
                    .peek()
                    .directory_member_fetches
                    .activity
                    .started
                    .load(Ordering::Relaxed),
                0
            );
        });
    }
}
