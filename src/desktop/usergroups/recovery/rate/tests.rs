use super::*;
use super_platinum_core::{
    config::{Session, WorkspaceSession},
    slack::Transport,
    state::Workspace,
};

fn key() -> Key {
    Key {
        account: "account-a".into(),
        team: "TGROUP001".into(),
        user: "USELF0001".into(),
    }
}
fn shell() -> ShellState {
    let mut shell = ShellState::login(crate::media::MediaRegistry::default());
    let k = key();
    let session = WorkspaceSession {
        team_id: k.team.clone(),
        user_id: k.user.clone(),
        enterprise_id: None,
        name: "Synthetic".into(),
        url: "https://example.invalid".into(),
        token: "synthetic-token".into(),
    };
    shell.core.active_account = Some(k.account);
    shell.core.active_team = Some(k.team.clone());
    shell.core.session = Some(Session {
        d_cookie: "synthetic-cookie".into(),
        unified_user_client_enabled: BTreeMap::new(),
        workspaces: BTreeMap::from([(k.team.clone(), session.clone())]),
    });
    shell.core.transport = Some(Arc::new(Transport::new("synthetic-cookie").unwrap()));
    shell
        .core
        .workspaces
        .insert(k.team, Workspace::from_session(&session));
    shell
}
fn blocked(store: &Store, key: &Key, endpoint: Endpoint, now: Instant) -> bool {
    store.debt(key, endpoint).is_some_and(|d| d.blocked(now))
}
#[test]
fn independent_directory_and_direct_expiry_preserve_original_receipts() {
    let now = Instant::now();
    let mut store = Store::default();
    store.record(key(), Endpoint::Directory, Some(8), now);
    assert!(!store.status(&key(), now).blocked);
    assert!(blocked(&store, &key(), Endpoint::Directory, now));
    assert!(!blocked(&store, &key(), Endpoint::Direct, now));
    store.record(key(), Endpoint::Direct, Some(15), now);
    assert!(store.status(&key(), now).blocked);
    let dir = store
        .debt(&key(), Endpoint::Directory)
        .unwrap()
        .snapshot(now);
    let direct = store.debt(&key(), Endpoint::Direct).unwrap().snapshot(now);
    let first = now + Duration::from_secs(8);
    assert!(!store.status(&key(), first).blocked);
    assert!(!blocked(&store, &key(), Endpoint::Directory, first));
    assert!(blocked(&store, &key(), Endpoint::Direct, first));
    assert!(store.poll(&key(), first));
    assert!(!store.poll(&key(), first));
    assert!(store.poll(&key(), now + Duration::from_secs(15)));
    assert!(!store.poll(&key(), now + Duration::from_secs(16)));
    // Expiry changes admission, never clears the receipt or starts a new debt.
    assert_eq!(store.next, 2);
    assert_eq!(
        store
            .debt(&key(), Endpoint::Directory)
            .unwrap()
            .snapshot(first),
        serde_json::json!({"revision":dir["revision"],"retryAfterSecs":8,"blocked":false})
    );
    assert_eq!(
        store
            .debt(&key(), Endpoint::Direct)
            .unwrap()
            .snapshot(first),
        direct
    );
}
#[test]
fn direct_cooling_does_not_disable_a_healthy_directory() {
    let now = Instant::now();
    let mut store = Store::default();
    store.record(key(), Endpoint::Direct, Some(15), now);
    assert!(store.status(&key(), now).observed);
    assert!(!store.status(&key(), now).blocked);
    assert!(!blocked(&store, &key(), Endpoint::Directory, now));
    assert!(blocked(&store, &key(), Endpoint::Direct, now));
}
#[test]
fn missing_header_uses_sixty_seconds_zero_is_immediate_and_large_is_unclamped() {
    let now = Instant::now();
    let mut store = Store::default();
    store.record(key(), Endpoint::Directory, None, now);
    let debt = store.debt(&key(), Endpoint::Directory).unwrap();
    assert_eq!(debt.header, None);
    assert!(debt.blocked(now + Duration::from_secs(59)));
    assert!(!debt.blocked(now + Duration::from_secs(60)));
    store.record(key(), Endpoint::Direct, Some(0), now);
    assert!(!blocked(&store, &key(), Endpoint::Direct, now));
    store.record(key(), Endpoint::Direct, Some(86_400), now);
    let debt = store.debt(&key(), Endpoint::Direct).unwrap();
    assert_eq!(debt.header, Some(86_400));
    assert!(debt.blocked(now + Duration::from_secs(86_399)));
    assert!(!debt.blocked(now + Duration::from_secs(86_400)));
}
#[test]
fn deadline_overflow_is_scoped_and_cannot_be_shortened_by_later_response() {
    let now = Instant::now();
    let mut store = Store::default();
    store.record(key(), Endpoint::Directory, Some(u64::MAX), now);
    assert!(
        store
            .debt(&key(), Endpoint::Directory)
            .unwrap()
            .deadline
            .is_none()
    );
    store.record(key(), Endpoint::Directory, Some(0), now);
    assert!(blocked(
        &store,
        &key(),
        Endpoint::Directory,
        now + Duration::from_secs(500)
    ));
    assert!(!store.status(&key(), now).blocked); // Independent direct path is usable.
    let mut other = key();
    other.account = "other".into();
    assert!(!store.status(&other, now).observed);
}
#[test]
fn repeated_shorter_response_never_shortens_original_deadline() {
    let now = Instant::now();
    let mut store = Store::default();
    store.record(key(), Endpoint::Directory, Some(100), now);
    store.record(
        key(),
        Endpoint::Directory,
        Some(2),
        now + Duration::from_secs(1),
    );
    let debt = store.debt(&key(), Endpoint::Directory).unwrap();
    assert_eq!(debt.header, Some(2)); // Last actual response remains unmodified.
    assert_eq!(debt.revision, 2);
    assert_eq!(debt.deadline, Some(now + Duration::from_secs(100)));
    store.record(
        key(),
        Endpoint::Directory,
        Some(200),
        now + Duration::from_secs(2),
    );
    assert_eq!(
        store.debt(&key(), Endpoint::Directory).unwrap().deadline,
        Some(now + Duration::from_secs(202))
    );
}
#[test]
fn every_stable_identity_component_isolated() {
    let now = Instant::now();
    let mut store = Store::default();
    store.record(key(), Endpoint::Directory, Some(15), now);
    store.record(key(), Endpoint::Direct, Some(15), now);
    for field in 0..3 {
        let mut other = key();
        match field {
            0 => other.account.push('x'),
            1 => other.team.push('x'),
            _ => other.user.push('x'),
        }
        assert!(!store.status(&other, now).observed);
        assert!(!store.status(&other, now).blocked);
        assert!(!store.poll(&other, now + Duration::from_secs(20)));
    }
}
#[test]
fn credentials_rotation_cannot_bypass_debt_or_admit_old_response() {
    let mut shell = shell();
    let owner = Context::capture(&shell, &key().team).unwrap();
    let now = Instant::now();
    let mut store = Store::default();
    let error = Error::RateLimited {
        retry_after_secs: Some(15),
    };
    store.receive(&shell, &owner, Endpoint::Directory, &error, now);
    shell
        .core
        .session
        .as_mut()
        .unwrap()
        .workspaces
        .get_mut(&key().team)
        .unwrap()
        .token
        .push('x');
    let successor = Context::capture(&shell, &key().team).unwrap();
    assert!(blocked(
        &store,
        &Key::capture(&successor),
        Endpoint::Directory,
        now
    ));
    store.receive(&shell, &owner, Endpoint::Direct, &error, now);
    assert!(store.debt(&key(), Endpoint::Direct).is_none());
    store.receive(&shell, &successor, Endpoint::Direct, &error, now);
    assert!(store.status(&key(), now).blocked);
}
#[test]
fn retired_transport_epoch_or_account_cannot_install_debt() {
    let error = Error::RateLimited {
        retry_after_secs: Some(15),
    };
    for change in 0..3 {
        let mut shell = shell();
        let owner = Context::capture(&shell, &key().team).unwrap();
        match change {
            0 => shell.core.transport = Some(Arc::new(Transport::new("synthetic-cookie").unwrap())),
            1 => shell.core.account_epoch += 1,
            _ => shell.core.active_account = Some("successor".into()),
        }
        let mut store = Store::default();
        store.receive(&shell, &owner, Endpoint::Direct, &error, Instant::now());
        assert!(store.entries.is_empty());
    }
}
#[test]
fn source_and_socket_retirement_do_not_erase_a_legitimate_received_limit() {
    let mut shell = shell();
    let owner = Context::capture(&shell, &key().team).unwrap();
    shell
        .core
        .workspaces
        .get_mut(&key().team)
        .unwrap()
        .rt_generation += 1;
    assert!(!owner.current(&shell));
    assert!(owner.account_current(&shell));
    let now = Instant::now();
    let mut store = Store::default();
    store.receive(
        &shell,
        &owner,
        Endpoint::Directory,
        &Error::RateLimited {
            retry_after_secs: Some(15),
        },
        now,
    );
    assert!(blocked(&store, &key(), Endpoint::Directory, now));
    // A new opening has no authority to clear endpoint debt.
    let reopened = Context::capture(&shell, &key().team).unwrap();
    assert_eq!(Key::capture(&owner), Key::capture(&reopened));
    assert!(blocked(
        &store,
        &Key::capture(&reopened),
        Endpoint::Directory,
        now
    ));
}
#[test]
fn other_errors_create_no_debt_and_subsecond_remaining_delay_rounds_up() {
    let shell = shell();
    let owner = Context::capture(&shell, &key().team).unwrap();
    let now = Instant::now();
    let mut store = Store::default();
    store.receive(
        &shell,
        &owner,
        Endpoint::Directory,
        &Error::Offline("fixture".into()),
        now,
    );
    assert!(store.entries.is_empty());
    store.receive(
        &shell,
        &owner,
        Endpoint::Directory,
        &Error::RateLimited {
            retry_after_secs: Some(1),
        },
        now,
    );
    assert!(matches!(
        store
            .debt(&key(), Endpoint::Directory)
            .unwrap()
            .error(now + Duration::from_millis(500)),
        Error::RateLimited {
            retry_after_secs: Some(1)
        }
    ));
}
