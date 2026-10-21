use super::*;
use std::cell::Cell;
use super_platinum_core::config::Session;

fn shell() -> ShellState {
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    let session = WorkspaceSession {
        team_id: "T1".into(),
        enterprise_id: None,
        user_id: shell.core.workspaces["T1"].self_user_id.clone(),
        name: "Fixture".into(),
        url: "https://fixture.slack.com".into(),
        token: "synthetic-only".into(),
    };
    shell.core.session = Some(Session {
        unified_user_client_enabled: Default::default(),
        d_cookie: "synthetic-only".into(),
        workspaces: [("T1".into(), session)].into(),
    });
    shell
}

fn guard(current: Rc<Cell<bool>>) -> NavigationGuard {
    NavigationGuard {
        current: Rc::new(move |_| current.get()),
        lock: Rc::new(|| Box::pin(async { None })),
        read: None,
    }
}

fn attempt(shell: &mut ShellState, current: Rc<Cell<bool>>) -> Attempt {
    Attempt::begin(shell, "T1", "CACTMISS01", guard(current))
        .unwrap()
        .unwrap()
}

fn channel() -> Channel {
    Channel {
        id: "CACTMISS01".into(),
        name: Some("activity-target".into()),
        is_channel: true,
        ..Default::default()
    }
}

#[test]
fn refused_source_or_foreign_team_never_reserves_a_link_intent() {
    let mut s = shell();
    let before = (s.slack_link_generation, s.channel_generation);
    assert!(
        Attempt::begin(&mut s, "T1", "CACTMISS01", guard(Rc::new(Cell::new(false))))
            .unwrap()
            .is_none()
    );
    for (team, id) in [("TOTHER", "CACTMISS01"), ("T1", "C1&team=TOTHER")] {
        assert!(Attempt::begin(&mut s, team, id, guard(Rc::new(Cell::new(true)))).is_err());
    }
    assert_eq!((s.slack_link_generation, s.channel_generation), before);
}

#[test]
fn retired_occurrence_cannot_publish_report_or_select_even_when_route_visit_is_current() {
    let mut s = shell();
    let current = Rc::new(Cell::new(true));
    let attempt = attempt(&mut s, current.clone());
    let revision = s.core.workspaces["T1"].channel_lifecycle.revision();
    let active = s.core.active_channel.clone();
    let failures = Rc::new(Cell::new(0));
    let observed = failures.clone();
    let feedback: FailureFeedback = Some(Rc::new(move |_, _| observed.set(observed.get() + 1)));
    s.toast = None;
    current.set(false); // e.g. same execution ID, refreshed raw detail/occurrence.
    assert!(attempt.visit.current(&s));
    assert!(!attempt.resolve(&mut s, "CACTMISS01", channel(), revision));
    attempt.failed(&mut s, &feedback, &Error::Api("private response".into()));
    attempt.failed(&mut s, &None, &Error::Api("retired response".into()));
    assert!(attempt.select(&mut s, "CACTMISS01").is_none());
    assert!(!s.core.workspaces["T1"].channels.contains_key("CACTMISS01"));
    assert_eq!(s.core.active_channel, active);
    assert_eq!(failures.get(), 0);
    assert!(s.toast.is_none());
}

#[test]
fn route_owner_retirement_still_blocks_a_permissive_caller() {
    for change in 0..5 {
        let mut s = shell();
        let attempt = attempt(&mut s, Rc::new(Cell::new(true)));
        match change {
            0 => s.core.account_epoch += 1,
            1 => s.channel_generation += 1,
            2 => s.slack_link_generation += 1,
            3 => s
                .core
                .session
                .as_mut()
                .unwrap()
                .workspaces
                .get_mut("T1")
                .unwrap()
                .token
                .push('x'),
            _ => s.profile_generation += 1,
        }
        let revision = s.core.workspaces["T1"].channel_lifecycle.revision();
        assert!(!attempt.current(&s));
        assert!(!attempt.resolve(&mut s, "CACTMISS01", channel(), revision));
        assert!(attempt.select(&mut s, "CACTMISS01").is_none());
    }
}

#[test]
fn wrong_response_id_never_enters_workspace() {
    let mut s = shell();
    let attempt = attempt(&mut s, Rc::new(Cell::new(true)));
    let revision = s.core.workspaces["T1"].channel_lifecycle.revision();
    let mut wrong = channel();
    wrong.id = "COTHER".into();
    assert!(!attempt.resolve(&mut s, "CACTMISS01", wrong, revision));
    assert!(!s.core.workspaces["T1"].channels.contains_key("COTHER"));
}

#[test]
fn known_selection_transfers_refresh_to_successor_without_old_caller_authority() {
    let mut s = shell();
    s.core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .channels
        .insert("CACTMISS01".into(), channel());
    let current = Rc::new(Cell::new(true));
    let attempt = attempt(&mut s, current.clone());
    let next = attempt.select(&mut s, "CACTMISS01").unwrap();
    assert_eq!(s.core.active_channel.as_deref(), Some("CACTMISS01"));
    assert_eq!(s.main_view, MainView::Home);
    assert_eq!(s.channel_tab, ChannelTab::Messages);
    current.set(false);
    assert!(!attempt.current(&s));
    assert!(next.current(&s));
    assert!(attempt.select(&mut s, "CACTMISS01").is_none());
    s.channel_generation += 1;
    assert!(!next.current(&s));
}

#[test]
fn current_error_reports_safe_feedback_without_selection() {
    let mut s = shell();
    s.toast = None;
    let attempt = attempt(&mut s, Rc::new(Cell::new(true)));
    let active = s.core.active_channel.clone();
    let failures = Rc::new(Cell::new(0));
    let observed = failures.clone();
    let feedback: FailureFeedback = Some(Rc::new(move |_, message| {
        assert!(!message.contains("secret"));
        observed.set(observed.get() + 1);
    }));
    attempt.failed(&mut s, &feedback, &Error::Api("secret".into()));
    assert_eq!(failures.get(), 1);
    assert_eq!(s.core.active_channel, active);
    assert!(s.toast.is_none());
}

#[test]
fn scoped_offline_and_local_failures_use_callback_without_overwriting_existing_toast() {
    let mut s = shell();
    let attempt = attempt(&mut s, Rc::new(Cell::new(true)));
    s.show_toast("Unrelated existing feedback");
    let received = Rc::new(std::cell::RefCell::new(Vec::new()));
    let observed = received.clone();
    let feedback: FailureFeedback = Some(Rc::new(move |_, message| {
        observed.borrow_mut().push(message.to_owned());
    }));
    attempt.failed(
        &mut s,
        &feedback,
        &Error::Offline("private connection detail".into()),
    );
    scoped_failure(&mut s, &feedback, "This conversation link is invalid.");
    scoped_failure(&mut s, &feedback, "This conversation is unavailable.");
    assert_eq!(
        &*received.borrow(),
        &[
            "Connect to Slack to open this conversation.",
            "This conversation link is invalid.",
            "This conversation is unavailable.",
        ]
    );
    assert_eq!(
        s.toast.as_ref().unwrap().text,
        "Unrelated existing feedback"
    );
}

#[test]
fn no_scoped_callback_preserves_existing_shared_failure_feedback() {
    let mut s = shell();
    let attempt = attempt(&mut s, Rc::new(Cell::new(true)));
    s.toast = None;
    attempt.failed(&mut s, &None, &Error::Api("private response".into()));
    assert_eq!(
        s.toast.as_ref().unwrap().text,
        "Could not open this conversation. It may no longer be accessible."
    );
    scoped_failure(&mut s, &None, "This conversation is unavailable.");
    assert_eq!(
        s.toast.as_ref().unwrap().text,
        "This conversation is unavailable."
    );
}

#[tokio::test]
async fn retired_read_is_never_polled_and_successful_result_is_retained() {
    let polls = Cell::new(0);
    let refused = std::future::poll_fn(|_| {
        polls.set(polls.get() + 1);
        Poll::Ready(7)
    });
    assert_eq!(while_current(refused, || false).await, None);
    assert_eq!(polls.get(), 0);
    assert_eq!(while_current(async { 7 }, || true).await, Some(7));
}

#[tokio::test]
async fn resuming_future_checks_owner_before_another_dispatch_poll() {
    let current = Cell::new(true);
    let polls = Cell::new(0);
    let read = std::future::poll_fn(|cx| {
        polls.set(polls.get() + 1);
        current.set(false);
        cx.waker().wake_by_ref();
        Poll::<()>::Pending
    });
    assert_eq!(while_current(read, || current.get()).await, None);
    assert_eq!(polls.get(), 1);
}

#[tokio::test]
async fn quiet_retirement_drops_pending_read_and_native_lease_releases_on_refusal() {
    struct Dropped(Rc<Cell<bool>>);
    impl Drop for Dropped {
        fn drop(&mut self) {
            self.0.set(true);
        }
    }
    let dropped = Rc::new(Cell::new(false));
    let owner = Dropped(dropped.clone());
    let current = Cell::new(true);
    let read = async {
        let _owner = owner;
        current.set(false);
        std::future::pending::<()>().await;
    };
    assert_eq!(while_current(read, || current.get()).await, None);
    assert!(dropped.get());
    let (release, receiver) = crate::app_home::inputs::native::test_pair();
    let lease = NativeLease(release);
    drop(lease);
    assert_eq!(receiver.await, Ok(()));
}

#[tokio::test]
async fn known_commit_reuses_initial_lease_without_racing_a_second_native_lock() {
    let (release, mut receiver) = crate::app_home::inputs::native::test_pair();
    let calls = Rc::new(Cell::new(0));
    let observed = calls.clone();
    let guard = NavigationGuard {
        current: Rc::new(|_| true),
        lock: Rc::new(move || {
            observed.set(observed.get() + 1);
            Box::pin(async { None })
        }),
        read: None,
    };
    let commit = commit_lease(Some(NativeLease(release)), &guard)
        .await
        .unwrap();
    assert_eq!(calls.get(), 0);
    assert!(matches!(
        receiver.try_recv(),
        Err(tokio::sync::oneshot::error::TryRecvError::Empty)
    ));
    drop(commit);
    assert_eq!(receiver.await, Ok(()));
    assert!(commit_lease(None, &guard).await.is_none());
    assert_eq!(calls.get(), 1);
}

#[tokio::test]
async fn missing_read_requires_current_source_before_and_after_pending_acknowledgment() {
    for mode in 0..3 {
        let current = Rc::new(Cell::new(mode != 0));
        let started = Rc::new(Cell::new(0));
        let settled = Rc::new(Cell::new(0));
        let lifecycle = ReadLifecycle {
            started: {
                let current = current.clone();
                let started = started.clone();
                Rc::new(move || {
                    started.set(started.get() + 1);
                    let current = current.clone();
                    Box::pin(async move {
                        if mode == 2 {
                            current.set(false);
                        }
                        mode != 1
                    })
                })
            },
            settled: {
                let settled = settled.clone();
                Rc::new(move || settled.set(settled.get() + 1))
            },
        };
        assert!(
            PendingRead::begin(Some(&lifecycle), || current.get())
                .await
                .is_none()
        );
        assert_eq!(started.get(), u32::from(mode != 0));
        assert_eq!(settled.get(), u32::from(mode != 0));
    }
}

#[tokio::test]
async fn admitted_pending_ack_holds_original_lease_but_read_does_not() {
    let (release, receiver) = crate::app_home::inputs::native::test_pair();
    let receiver = Rc::new(std::cell::RefCell::new(receiver));
    let lease = NativeLease(release);
    let settled = Rc::new(Cell::new(0));
    let lifecycle = ReadLifecycle {
        started: {
            let receiver = receiver.clone();
            Rc::new(move || {
                assert!(matches!(
                    receiver.borrow_mut().try_recv(),
                    Err(tokio::sync::oneshot::error::TryRecvError::Empty)
                ));
                Box::pin(async { true })
            })
        },
        settled: {
            let settled = settled.clone();
            Rc::new(move || settled.set(settled.get() + 1))
        },
    };
    let pending = PendingRead::begin(Some(&lifecycle), || true).await.unwrap();
    assert_eq!(settled.get(), 0);
    drop(lease);
    let result = while_current(
        async {
            assert_eq!(receiver.borrow_mut().try_recv(), Ok(()));
            Err::<(), _>(Error::Offline("closed fixture".into()))
        },
        || true,
    )
    .await;
    assert!(matches!(result, Some(Err(Error::Offline(_)))));
    assert_eq!(settled.get(), 0);
    drop(pending);
    assert_eq!(settled.get(), 1);
}

#[tokio::test]
async fn dropping_unfinished_pending_ack_settles_only_its_own_lifecycle() {
    let settled = Rc::new(Cell::new(0));
    let lifecycle = ReadLifecycle {
        started: Rc::new(|| Box::pin(std::future::pending())),
        settled: {
            let settled = settled.clone();
            Rc::new(move || settled.set(settled.get() + 1))
        },
    };
    let mut begin = Box::pin(PendingRead::begin(Some(&lifecycle), || true));
    std::future::poll_fn(|cx| {
        assert!(begin.as_mut().poll(cx).is_pending());
        Poll::Ready(())
    })
    .await;
    assert_eq!(settled.get(), 0);
    drop(begin);
    assert_eq!(settled.get(), 1);
    drop(lifecycle);
    assert_eq!(settled.get(), 1);
}
