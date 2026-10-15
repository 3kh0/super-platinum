//! Real owned fixture acquisition, explicit migration and Account/Host revisit.
use super::super::account::wait;
use super::*;
use reservation::{Decision, FixtureOrigin};
use std::time::Duration;

async fn prepared(value: &Reservation) {
    tokio::time::timeout(Duration::from_secs(10), value.ready())
        .await
        .unwrap();
}
async fn ready(value: &Acquisition) {
    tokio::time::timeout(Duration::from_secs(10), value.ready())
        .await
        .unwrap();
}
async fn confirm(value: &Reservation) {
    let Decision::NeedsConsent(review) = value.decision() else {
        panic!("explicit consent required")
    };
    let route = value.route().unwrap();
    tokio::task::spawn_blocking(move || {
        route.commit(&review).unwrap().wait().unwrap();
        route.inspect().unwrap().wait().unwrap().open().unwrap();
    })
    .await
    .unwrap();
}
async fn acquired(root: &mut Root) -> (Reservation, Acquisition) {
    let reserved = root.reserve_fixture(FixtureOrigin::Fresh).unwrap();
    prepared(&reserved).await;
    assert!(matches!(
        root.acquire_reserved(&reserved),
        Err(Error::Pending)
    ));
    Box::pin(confirm(&reserved)).await;
    let acquired = root.acquire_reserved(&reserved).unwrap();
    ready(&acquired).await;
    (reserved, acquired)
}
fn shell(account: &Arc<Account>) -> ShellState {
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    shell.core.active_account = Some(account.id().into());
    shell.core.account_epoch = 7;
    shell
}
async fn enroll(account: &Arc<Account>) {
    let owner = account
        .owner(DraftKey {
            team: "T1".into(),
            channel: "C1".into(),
            thread: None,
        })
        .unwrap();
    let initial = owner.local_inventory().await.unwrap();
    let client = account.client().unwrap();
    let lease = wait(client.renew_local(initial.clone())).await.unwrap();
    let serial = lease.initial_serial();
    let local = LocalDraft {
        composer: super_platinum_core::ComposerState::with_text("retained native input"),
        attachments: vec![],
    };
    let durable::LocalWriteResult::Applied(current) =
        wait(client.write_local(lease, serial, serial + 1, initial.local().clone(), local))
            .await
            .unwrap()
    else {
        panic!("canonical write")
    };
    wait(client.enroll_local(owner.scope().clone(), current))
        .await
        .unwrap();
}

#[tokio::test]
async fn reserved_missing_store_requires_explicit_consent_before_account_readiness() {
    let mut root = Root::new();
    let reserved = root.reserve_fixture(FixtureOrigin::Fresh).unwrap();
    assert_eq!(root.occupied(), 1);
    prepared(&reserved).await;
    let Decision::NeedsConsent(review) = reserved.decision() else {
        panic!("no automatic migration")
    };
    assert_eq!(
        review.metadata().kind,
        durable::owned_store::StoreKind::Missing
    );
    assert!(matches!(
        root.acquire_reserved(&reserved),
        Err(Error::Pending)
    ));
    assert!(root.accounts.is_empty());
    Box::pin(confirm(&reserved)).await;
    let acquired = root.acquire_reserved(&reserved).unwrap();
    ready(&acquired).await;
    let result = acquired.0.initialization.result().unwrap();
    let receipt = result.initialized.as_ref().unwrap();
    assert_eq!(receipt.mode(), durable::LoadMode::AfterRestart);
    assert_eq!(receipt.account(), acquired.account().unwrap().id());
    assert_eq!(receipt.worker_id(), reserved.route().unwrap().worker_id());
    assert!(receipt.inventory().managed().is_empty());
    assert_eq!(root.occupied(), 1);
    assert!(matches!(reserved.decision(), Decision::ProtectedOwned));
}

#[tokio::test]
async fn three_owned_revisits_keep_exact_account_and_actual_live_stream() {
    let mut root = Root::new();
    let (reserved, acquired) = Box::pin(acquired(&mut root)).await;
    let account = acquired.account().unwrap();
    Box::pin(enroll(&account)).await;
    let client = account.client().unwrap();
    let state = wait(client.stream_state("T1".into())).await.unwrap();
    let stream = wait(client.open_stream("T1".into(), state)).await.unwrap();
    let mut shell = shell(&account);
    for _ in 0..3 {
        let again = root.acquire_reserved(&reserved).unwrap();
        assert!(Arc::ptr_eq(&account, &again.account().unwrap()));
        let selection = root.select(&shell, &again).unwrap();
        let load = root.begin_load(&shell, &selection).unwrap();
        tokio::time::timeout(Duration::from_secs(10), load.ready())
            .await
            .unwrap();
        root.finish_load(&shell, &load).unwrap();
        root.retire_selection();
        shell.draft_writes.view_id += 1;
    }
    // This exact core capability is revoked by AfterRestart, so a second cold
    // load cannot be hidden behind a factory counter or a matching JSON shape.
    assert!(stream.is_live());
    assert_eq!(acquired.0.source.starts(), 1);
    assert!(matches!(
        wait(client.load(durable::LoadMode::AfterRestart)).await,
        Err(durable::Error::Stale)
    ));
    assert!(stream.is_live());
}

#[tokio::test]
async fn canceled_acquisition_waiter_and_lost_reservation_reattach_without_new_source() {
    let mut root = Root::new();
    let reserved = root.reserve_fixture(FixtureOrigin::Fresh).unwrap();
    let ui = reserved.clone();
    let task = tokio::spawn(async move {
        ui.ready().await;
    });
    task.abort();
    let _ = task.await;
    drop(reserved);
    let retained = root.retained_reservations().pop().unwrap();
    prepared(&retained).await;
    Box::pin(confirm(&retained)).await;
    let first = root.acquire_reserved(&retained).unwrap();
    let ui = first.clone();
    let task = tokio::spawn(async move {
        ui.ready().await;
    });
    task.abort();
    let _ = task.await;
    let again = root.acquire_reserved(&retained).unwrap();
    ready(&again).await;
    assert!(Arc::ptr_eq(&first.0, &again.0));
    assert!(Arc::ptr_eq(
        &first.account().unwrap(),
        &again.account().unwrap()
    ));
    assert_eq!(again.0.source.starts(), 1);
    assert_eq!(root.occupied(), 1);
}

#[tokio::test]
async fn sixteen_failed_reservations_remain_counted_and_never_fallback() {
    let mut root = Root::new();
    let mut all = vec![];
    for _ in 0..MAX_ACCOUNTS {
        all.push(
            root.reserve_fixture(FixtureOrigin::Reopen("invalid".into()))
                .unwrap(),
        );
    }
    assert_eq!(root.occupied(), MAX_ACCOUNTS);
    assert!(matches!(
        root.reserve_fixture(FixtureOrigin::Fresh),
        Err(Error::Capacity)
    ));
    for value in all {
        prepared(&value).await;
        assert!(matches!(
            value.decision(),
            Decision::Unavailable(Error::Storage(_))
        ));
        assert!(value.route().is_err());
        assert!(matches!(
            root.acquire_reserved(&value),
            Err(Error::Storage(_))
        ));
    }
    assert!(root.accounts.is_empty());
    assert_eq!(root.occupied(), MAX_ACCOUNTS);
}

#[tokio::test]
async fn foreign_root_reservation_is_not_an_account_alias() {
    let mut first = Root::new();
    let (reserved, acquired) = Box::pin(acquired(&mut first)).await;
    let mut foreign = Root::new();
    assert!(matches!(
        foreign.acquire_reserved(&reserved),
        Err(Error::Identity)
    ));
    assert_eq!(foreign.occupied(), 0);
    assert!(acquired.account().is_ok());
}

#[tokio::test]
async fn another_factory_binding_is_terminal_and_never_exposes_an_account() {
    let mut root = Root::new();
    let reserved = root.reserve_fixture(FixtureOrigin::Fresh).unwrap();
    prepared(&reserved).await;
    Box::pin(confirm(&reserved)).await;
    let opened = reserved.route().unwrap().opened().unwrap();
    tokio::task::spawn_blocking(move || {
        opened
            .initialize_cold(&durable::owned_store::FactoryIdentity::default())
            .unwrap()
            .wait()
            .unwrap();
    })
    .await
    .unwrap();
    let acquired = root.acquire_reserved(&reserved).unwrap();
    ready(&acquired).await;
    assert!(matches!(
        acquired.account(),
        Err(Error::Storage(durable::Error::Stale))
    ));
    let again = root.acquire_reserved(&reserved).unwrap();
    assert!(Arc::ptr_eq(&acquired.0, &again.0));
    assert!(matches!(
        again.account(),
        Err(Error::Storage(durable::Error::Stale))
    ));
    assert_eq!(again.0.source.starts(), 1);
    assert_eq!(root.occupied(), 1);
}

#[tokio::test]
async fn retired_selection_cannot_be_revived_by_owned_initialization_or_revisit() {
    let mut root = Root::new();
    let (reserved, acquired) = Box::pin(acquired(&mut root)).await;
    let account = acquired.account().unwrap();
    let mut shell = shell(&account);
    let old = root.select(&shell, &acquired).unwrap();
    root.retire_selection();
    shell.core.account_epoch += 1;
    let again = root.acquire_reserved(&reserved).unwrap();
    let current = root.select(&shell, &again).unwrap();
    assert!(matches!(root.begin_load(&shell, &old), Err(Error::Stale)));
    shell.core.account_epoch -= 1;
    assert!(matches!(root.begin_load(&shell, &old), Err(Error::Stale)));
    assert!(matches!(
        root.begin_load(&shell, &current),
        Err(Error::Stale)
    ));
}

#[tokio::test]
async fn stop_root_admission_retains_opening_outcomes_without_claiming_drain() {
    let mut root = Root::new();
    let reserved = root.reserve_fixture(FixtureOrigin::Fresh).unwrap();
    root.stop_admission();
    prepared(&reserved).await;
    assert!(matches!(reserved.decision(), Decision::NeedsConsent(_)));
    assert!(matches!(
        root.acquire_reserved(&reserved),
        Err(Error::Closed)
    ));
    assert_eq!(root.retained_reservations().len(), 1);
    assert_eq!(root.occupied(), 1);
}

#[tokio::test]
async fn completed_fixture_reopen_has_same_account_new_worker_no_reimport() {
    let mut root = Root::new();
    let (reserved, acquired) = Box::pin(acquired(&mut root)).await;
    let account = acquired.account().unwrap();
    let id = account.id().to_owned();
    let route = reserved.route().unwrap();
    let worker = route.worker_id().to_owned();
    let opened = route.opened().unwrap();
    let receipt = opened.migration_id().unwrap().to_owned();
    let ticket = opened.fixture_ticket().unwrap();
    drop((opened, route, account, acquired, reserved, root));
    let mut next = Root::new();
    let reserved = next.reserve_fixture(FixtureOrigin::Reopen(ticket)).unwrap();
    prepared(&reserved).await;
    assert!(matches!(reserved.decision(), Decision::ProtectedOwned));
    let route = reserved.route().unwrap();
    assert_ne!(route.worker_id(), worker);
    assert_eq!(route.account(), id);
    assert_eq!(
        route.opened().unwrap().migration_id(),
        Some(receipt.as_str())
    );
    let acquired = next.acquire_reserved(&reserved).unwrap();
    ready(&acquired).await;
    assert_eq!(acquired.account().unwrap().id(), id);
}

#[test]
fn missing_runtime_refuses_before_reserving_or_starting_fixture_io() {
    let mut root = Root::new();
    assert!(matches!(
        root.reserve_fixture(FixtureOrigin::Fresh),
        Err(Error::Storage(_))
    ));
    assert_eq!(root.occupied(), 0);
    assert!(root.retained_reservations().is_empty());
}
