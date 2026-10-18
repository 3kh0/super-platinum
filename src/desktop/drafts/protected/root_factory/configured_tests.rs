//! Actual temporary owned worker + schema + existing desktop Root/Host lanes.
//! Does not read configured paths, accounts, session secrets, or the network.
use super::*;
use super_platinum_core::ComposerState;

fn local(text: &str) -> LocalDraft {
    LocalDraft {
        composer: ComposerState::with_text(text),
        attachments: vec![],
    }
}
fn key() -> DraftKey {
    DraftKey {
        team: "T1".into(),
        channel: "C1".into(),
        thread: None,
    }
}
fn make_shell(account: &Arc<Account>) -> ShellState {
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    shell.core.active_account = Some(account.id().into());
    shell.core.account_epoch = 9;
    shell
}
async fn ready(root: &mut Root) -> (Acquisition, Arc<Account>) {
    let route = tokio::task::spawn_blocking(durable::owned_store::OwnedRoute::fresh_fixture)
        .await
        .unwrap()
        .unwrap();
    let source = Source::configured_fixture(route);
    let first = root.acquire(source.clone()).unwrap();
    let again = root.acquire(source).unwrap();
    assert!(Arc::ptr_eq(&first.0, &again.0));
    first.ready().await;
    let account = first.account().unwrap();
    assert!(account.owned.is_some());
    assert!(Arc::ptr_eq(&account, &again.account().unwrap()));
    assert!(Arc::ptr_eq(
        &account,
        &root.configured_account(account.id()).unwrap()
    ));
    let configured = root.acquire_configured(account.id()).unwrap();
    assert!(Arc::ptr_eq(&account, &configured.account().unwrap()));
    (first, account)
}
async fn load(root: &mut Root, shell: &ShellState, acquisition: &Acquisition) {
    let selected = root.select(shell, acquisition).unwrap();
    let read = root.begin_load(shell, &selected).unwrap();
    read.ready().await;
    root.finish_load(shell, &read).unwrap();
}

#[tokio::test]
async fn actual_cold_schema_root_revisits_and_more_than_64_local_writes_keep_one_owner() {
    let mut root = Root::new();
    let (acquired, account) = ready(&mut root).await;
    let shell = make_shell(&account);
    load(&mut root, &shell, &acquired).await;
    let baseline = account.owned.as_ref().unwrap().operations.retained().len();
    assert!(
        root.submit_current(&shell, &key(), &local("initial"))
            .is_err()
    );
    let opened = root.host.retained_locals(&account).pop().unwrap();
    opened.ready().await;
    let lane = root.host.local(&opened).unwrap();
    lane.writer().flush().await.unwrap();
    for n in 0..128 {
        let expected = local(&format!("ordinary typing {n} \u{65e5}\u{1f680}"));
        root.submit_current(&shell, &key(), &expected).unwrap();
        let observed = lane.writer().flush().await.unwrap().inventory.unwrap();
        assert!(same_local(observed.local(), &expected));
        assert_eq!(
            account.owned.as_ref().unwrap().operations.retained().len(),
            baseline
        );
    }
    let changed = make_shell(&account);
    load(&mut root, &changed, &acquired).await;
    let held = root.host.discovery().entry(&changed, &key()).unwrap();
    assert!(same_local(
        held.local(),
        &local("ordinary typing 127 \u{65e5}\u{1f680}")
    ));
    assert_eq!(root.host.retained_locals(&account).len(), 1);
    assert!(
        root.submit_current(&shell, &key(), &local("stale shell"))
            .is_err()
    );
}

#[tokio::test]
async fn held_open_coalesces_only_unadmitted_text_and_survives_account_switch() {
    let mut root = Root::new();
    let (first, account) = ready(&mut root).await;
    let old = make_shell(&account);
    load(&mut root, &old, &first).await;
    let (entered, release) = account.owned.as_ref().unwrap().hold_next_local_read();
    assert!(root.submit_current(&old, &key(), &local("first")).is_err());
    entered.await.unwrap();
    assert!(
        root.submit_current(&old, &key(), &local("latest \u{65e5}"))
            .is_err()
    );
    let opened = root.host.retained_locals(&account).pop().unwrap();
    let (second, other) = ready(&mut root).await;
    let other_shell = make_shell(&other);
    load(&mut root, &other_shell, &second).await;
    assert!(
        root.submit_current(&old, &key(), &local("retired"))
            .is_err()
    );
    assert!(root.host.local_presentations(&other).unwrap().is_empty());
    release.send(()).unwrap();
    opened.ready().await;
    let lane = root.host.local(&opened).unwrap();
    lane.writer().flush().await.unwrap();
    assert!(same_local(
        &root.host.local_presentations(&account).unwrap()[&key()],
        &local("latest \u{65e5}")
    ));
    let returned = make_shell(&account);
    load(&mut root, &returned, &first).await;
    assert!(same_local(
        root.host
            .discovery()
            .entry(&returned, &key())
            .unwrap()
            .local(),
        &local("latest \u{65e5}")
    ));
}

#[tokio::test]
async fn managed_inventory_is_retained_and_unverified_input_never_becomes_local_lease() {
    let mut root = Root::new();
    let (acquired, account) = ready(&mut root).await;
    let route = account.owned.as_ref().unwrap();
    let scope = Scope {
        account_id: account.id().into(),
        key: key(),
    };
    let inventory = route.local(scope.clone()).await.unwrap();
    let managed = super::super::owned_route::Held::new(route.operations.enroll(inventory))
        .observed_success()
        .await
        .unwrap();
    let shell = make_shell(&account);
    load(&mut root, &shell, &acquired).await;
    assert!(matches!(
        root.host
            .discovery()
            .entry(&shell, &key())
            .unwrap()
            .inventory(),
        Inventory::Managed(_)
    ));
    assert!(
        root.submit_current(&shell, &key(), &local("retain for review"))
            .is_err()
    );
    let opened = root.host.retained_managed(&account).pop().unwrap();
    opened.ready().await;
    let editor = root.host.managed(&opened).unwrap();
    editor.flush().await.unwrap();
    assert!(root.host.retained_locals(&account).is_empty());
    assert!(same_local(
        &root.host.managed_presentations(&account).unwrap()[&key()],
        &local("retain for review")
    ));
    let after = route.review(scope).await.unwrap();
    assert!(after.row().same_enrollment(managed.row()));
    assert_eq!(after.row().lineage().basis(), durable::Basis::Unverified);
    assert!(same_local(after.local(), &local("retain for review")));
    let writer = editor.snapshot().writer_id;
    root.submit_current(&shell, &key(), &local("second actual managed input"))
        .unwrap();
    editor.flush().await.unwrap();
    assert_eq!(editor.snapshot().writer_id, writer);
    assert_eq!(root.host.retained_managed(&account).len(), 1);
    assert!(root.host.retained_locals(&account).is_empty());
    assert_eq!(
        root.host.route_snapshot(&shell, &key()).unwrap().kind,
        "opening-managed"
    );
}
