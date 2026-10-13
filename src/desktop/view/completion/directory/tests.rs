use super::*;
fn account(epoch: u64) -> Account {
    Account {
        id: Some(format!("account-{epoch}")),
        epoch,
    }
}
fn owner(team: &str) -> Owner {
    Owner {
        team: team.into(),
        user: "U1".into(),
    }
}
fn start(gate: &mut Admission, account: Account, owner: Owner, now: Instant) -> Ticket {
    let Decision::Start(ticket) = gate.begin(account, owner, now) else {
        panic!("request was not admitted")
    };
    ticket
}
#[test]
fn changed_query_cannot_cancel_429_deadline_or_inflight_admission() {
    let now = Instant::now();
    let mut gate = Admission::default();
    let old = start(&mut gate, account(1), owner("T1"), now);
    // Dropping/replacing the query UI does not finish its dispatched read.
    assert!(matches!(
        gate.begin(account(1), owner("T1"), now + Duration::from_millis(250)),
        Decision::Busy
    ));
    gate.finish(
        &old,
        now + Duration::from_secs(1),
        Some(&Error::RateLimited {
            retry_after_secs: Some(30),
        }),
    );
    for elapsed in [1, 2, 15, 30] {
        assert!(matches!(
            gate.begin(account(1), owner("T1"), now + Duration::from_secs(elapsed)),
            Decision::Cooling
        ));
    }
    let next = start(
        &mut gate,
        account(1),
        owner("T1"),
        now + Duration::from_secs(31),
    );
    // A duplicate old result cannot release or extend the new request.
    gate.finish(
        &old,
        now + Duration::from_secs(32),
        Some(&Error::RateLimited {
            retry_after_secs: Some(90),
        }),
    );
    assert!(matches!(
        gate.begin(account(1), owner("T1"), now + Duration::from_secs(33)),
        Decision::Busy
    ));
    gate.finish(&next, now + Duration::from_secs(33), None);
    start(
        &mut gate,
        account(1),
        owner("T1"),
        now + Duration::from_secs(33),
    );
}
#[test]
fn old_account_response_cannot_block_replacement_and_workspaces_are_independent() {
    let now = Instant::now();
    let mut gate = Admission::default();
    let old = start(&mut gate, account(1), owner("T1"), now);
    let new = start(&mut gate, account(2), owner("T1"), now);
    gate.finish(
        &old,
        now,
        Some(&Error::RateLimited {
            retry_after_secs: Some(600),
        }),
    );
    gate.finish(&new, now, None);
    let new = start(&mut gate, account(2), owner("T1"), now);
    gate.finish(
        &new,
        now,
        Some(&Error::RateLimited {
            retry_after_secs: Some(30),
        }),
    );
    start(&mut gate, account(2), owner("T2"), now);
    assert!(matches!(
        gate.begin(account(2), owner("T1"), now),
        Decision::Cooling
    ));
}
#[test]
fn cache_is_bounded_without_evicting_unexpired_deadlines() {
    let now = Instant::now();
    let mut gate = Admission::default();
    for n in 0..MAX_OWNERS {
        let t = start(&mut gate, account(1), owner(&format!("T{n}")), now);
        gate.finish(
            &t,
            now,
            Some(&Error::RateLimited {
                retry_after_secs: Some(30),
            }),
        );
    }
    assert!(matches!(
        gate.begin(account(1), owner("overflow"), now),
        Decision::Full
    ));
    assert_eq!(gate.entries.len(), MAX_OWNERS);
    start(
        &mut gate,
        account(1),
        owner("overflow"),
        now + Duration::from_secs(30),
    );
    assert_eq!(gate.entries.len(), 1);
}

#[tokio::test(flavor = "current_thread")]
async fn canceled_query_receiver_still_records_controlled_http_429() {
    use super_platinum_core::slack::{PreparedRequest, Transport, client::RequestBody};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    tokio::task::LocalSet::new().run_until(async {
        let shared = Rc::new(Shared {
            admission: RefCell::new(Admission::default()),
            changed: Notify::new(),
        });
        let ticket = start(&mut shared.admission.borrow_mut(), account(1), owner("T1"), Instant::now());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (arrived, arrival) = oneshot::channel();
        let (release, released) = oneshot::channel();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut bytes = [0;4096];
            assert!(stream.read(&mut bytes).await.unwrap() > 0);
            arrived.send(()).unwrap();
            released.await.unwrap();
            stream.write_all(b"HTTP/1.1 429 Too Many Requests\r\nRetry-After: 30\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await.unwrap();
            drop(stream);
            assert!(tokio::time::timeout(Duration::from_millis(100), listener.accept()).await.is_err());
        });
        let (sender, receiver) = oneshot::channel();
        let task_shared = shared.clone();
        let task = tokio::task::spawn_local(async move {
            let transport = Transport::new("synthetic-cookie").unwrap();
            settle(task_shared, ticket, sender, async {
                transport.execute_mention_search(PreparedRequest {
                    method: "POST", url: format!("http://{address}/cache/T1/users/search"),
                    headers: vec![], body: RequestBody::Json(serde_json::json!({"query":"old"})),
                }).await.map(|_| Vec::new())
            }, || true).await;
        });
        tokio::time::timeout(Duration::from_secs(2), arrival).await.unwrap().unwrap();
        drop(receiver); // The user types another query while the HTTP read is in flight.
        assert!(matches!(shared.admission.borrow_mut().begin(account(1), owner("T1"), Instant::now()), Decision::Busy));
        release.send(()).unwrap();
        tokio::time::timeout(Duration::from_secs(2), task).await.unwrap().unwrap();
        let now = Instant::now();
        assert!(matches!(shared.admission.borrow_mut().begin(account(1), owner("T1"), now + Duration::from_millis(250)), Decision::Cooling));
        // A genuinely different account is not blocked by the old deadline.
        start(&mut shared.admission.borrow_mut(), account(2), owner("T1"), now);
        server.await.unwrap();
    }).await;
}

#[test]
fn credential_rotation_rejects_dispatch_and_late_adoption_but_navigation_retains_deadline() {
    use std::sync::Arc;
    use super_platinum_core::{
        config::{Session, WorkspaceSession},
        slack::Transport,
    };
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    shell.core.active_team = Some("T1".into());
    shell.core.session = Some(Session {
        d_cookie: "synthetic".into(),
        workspaces: [(
            "T1".into(),
            WorkspaceSession {
                team_id: "T1".into(),
                enterprise_id: None,
                user_id: "U1".into(),
                name: "Fixture".into(),
                url: "https://example.invalid".into(),
                token: "synthetic".into(),
            },
        )]
        .into(),
    });
    shell.core.transport = Some(Arc::new(Transport::new("synthetic").unwrap()));
    let ctx = Context::capture(&shell).unwrap();
    assert!(owner_current(&shell, &ctx));
    shell.core.active_team = Some("T2".into());
    assert!(!owner_current(&shell, &ctx));
    assert!(credentials_current(&shell, &ctx)); // Late 429 still belongs to T1.
    shell.core.active_team = Some("T1".into());
    shell
        .core
        .session
        .as_mut()
        .unwrap()
        .workspaces
        .get_mut("T1")
        .unwrap()
        .token = "replacement".into();
    assert!(!credentials_current(&shell, &ctx));
    shell
        .core
        .session
        .as_mut()
        .unwrap()
        .workspaces
        .get_mut("T1")
        .unwrap()
        .token = "synthetic".into();
    shell.core.transport = Some(Arc::new(Transport::new("synthetic").unwrap()));
    assert!(!credentials_current(&shell, &ctx)); // Same cookie, different transport owner.
    shell.core.transport = Some(ctx.transport.clone());
    shell.core.account_epoch += 1;
    assert!(!credentials_current(&shell, &ctx));
}
