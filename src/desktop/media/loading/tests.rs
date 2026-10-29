use super::*;
mod offline;
use crate::{
    bootstrap::test_support::{Harness, harness, pending},
    state::ShellState,
};
use dioxus::prelude::{ReadableExt, Signal, WritableExt};
use std::{
    sync::{
        atomic::{AtomicBool, AtomicUsize},
        mpsc,
    },
    time::Duration,
};
use super_platinum_core::{
    config::{Session, WorkspaceSession},
    state::Workspace,
};
fn shell(
    base: &MediaRegistry,
    account: &str,
    cookie: &str,
    transport: Option<Arc<Transport>>,
) -> ShellState {
    let mut shell = ShellState::opening(base.clone());
    let session = WorkspaceSession {
        team_id: "T1".into(),
        user_id: "U_MEDIA".into(),
        enterprise_id: None,
        name: "Synthetic".into(),
        url: "https://fixture.invalid".into(),
        token: "synthetic".into(),
    };
    shell.core.active_account = Some(account.into());
    shell.core.active_team = Some("T1".into());
    shell
        .core
        .workspaces
        .insert("T1".into(), Workspace::from_session(&session));
    let session = Session {
        d_cookie: cookie.into(),
        unified_user_client_enabled: Default::default(),
        workspaces: [("T1".into(), session)].into(),
    };
    shell.core.accounts.insert(account.into(), session.clone());
    shell.core.session = Some(session);
    shell.core.transport =
        Some(transport.unwrap_or_else(|| Arc::new(Transport::new(cookie).unwrap())));
    // Synthetic opening has no projection; bind before registering any test sources.
    shell.media = base.for_session(Some(account.into()));
    shell
}
fn setup(base: &MediaRegistry) -> Harness {
    let h = harness();
    crate::bootstrap::install_loaded(h.state, shell(base, "A", "synthetic-A", None));
    h
}
fn transport(state: Signal<ShellState>) -> Arc<Transport> {
    state.peek().core.transport.clone().unwrap()
}
fn store() -> Arc<MediaStore> {
    // Isolated fixture storage is retained; never open the user's shared cache.
    let root = std::env::current_dir()
        .unwrap()
        .join("tmp/parity-search-recovery-20261027/media-account-store-tests")
        .join(uuid::Uuid::new_v4().to_string());
    Arc::new(MediaStore::open(root).unwrap())
}
fn held_hook() -> (
    Arc<AtomicBool>,
    mpsc::Sender<()>,
    Arc<dyn Fn() + Send + Sync>,
) {
    let entered = Arc::new(AtomicBool::new(false));
    let (release, receive) = mpsc::channel();
    let receive = Arc::new(std::sync::Mutex::new(receive));
    let once = Arc::new(AtomicBool::new(false));
    let seen = entered.clone();
    (
        entered,
        release,
        Arc::new(move || {
            if !once.swap(true, Ordering::SeqCst) {
                seen.store(true, Ordering::SeqCst);
                receive
                    .lock()
                    .unwrap()
                    .recv_timeout(Duration::from_secs(10))
                    .expect("controlled cache worker released");
            }
        }),
    )
}
async fn until<F: Future<Output = ()>>(
    work: &mut std::pin::Pin<Box<F>>,
    condition: impl Fn() -> bool,
) {
    tokio::time::timeout(Duration::from_secs(5), async {
        while !condition() {
            pending(work.as_mut()).await;
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("controlled production loader reaches boundary");
}
fn get(registry: &MediaRegistry, id: &MediaAssetId) -> Response<Cow<'static, [u8]>> {
    registry.respond(
        Request::builder()
            .method("GET")
            .uri(id.uri())
            .body(Vec::new())
            .unwrap(),
    )
}
#[tokio::test(flavor = "current_thread")]
async fn queued_old_job_and_current_job_use_only_owned_auth_sources_through_real_install_and_account_return()
 {
    let base = MediaRegistry::default();
    let h = setup(&base);
    let state = h.state;
    let a = state.peek().media.clone();
    let original = transport(state);
    let old = a.register_image(
        MediaAssetKind::Attachment,
        "https://files.slack.com/shared",
        "image/png",
    );
    // A permanent failure is real state, independent of the later retirement fence.
    crate::media::load_with(
        state,
        original.clone(),
        |request| async move {
            assert_eq!(request.transport.d_cookie(), "synthetic-A");
            Err(Error::HttpStatus {
                status: 403,
                retry_after_secs: None,
            })
        },
        Hooks::default(),
    )
    .await;
    assert!(matches!(
        a.backoff.read().unwrap().get(&old),
        Some(Backoff::Abandoned)
    ));
    let shared_a = old.clone();
    let only_a = a.register_image(
        MediaAssetKind::Attachment,
        "https://files.slack.com/only-A",
        "image/png",
    );
    let public_a = a.register_image(
        MediaAssetKind::Attachment,
        "https://cdn.example.test/public.png",
        "image/png",
    );
    let gate = a.load_lock.clone().lock_owned().await;
    let calls = Arc::new(AtomicUsize::new(0));
    let captured = calls.clone();
    let mut old_work = Box::pin(crate::media::load_with(
        state,
        original.clone(),
        move |_| {
            let calls = captured.clone();
            async move {
                calls.fetch_add(1, Ordering::SeqCst);
                panic!("retired old job must not fetch either owner")
            }
        },
        Hooks::default(),
    ));
    pending(old_work.as_mut()).await;
    // Keep another synchronously captured A future unpolled until A -> B -> A.
    let late_calls = Arc::new(AtomicUsize::new(0));
    let late_record = late_calls.clone();
    let late = crate::media::load_with(
        state,
        original.clone(),
        move |_| {
            let calls = late_record.clone();
            async move {
                calls.fetch_add(1, Ordering::SeqCst);
                Ok(PLACEHOLDER_PNG.to_vec())
            }
        },
        Hooks::default(),
    );
    let loaded = shell(&base, "B", "synthetic-B", None);
    let b = loaded.media.clone();
    assert!(a.scope.active());
    let shared_b = b.register_deferred(
        MediaAssetKind::Attachment,
        "https://files.slack.com/shared",
        "image/png",
    );
    assert_ne!(shared_a, shared_b);
    assert!(b.sources.read().unwrap()[&shared_b].deferred);
    assert!(!a.sources.read().unwrap()[&shared_a].deferred);
    assert_eq!(
        b.register_image(
            MediaAssetKind::Attachment,
            "https://cdn.example.test/public.png",
            "image/png"
        ),
        public_a
    );
    crate::bootstrap::install_loaded(state, loaded);
    assert!(!a.scope.active());
    // The original protocol registry promotes B's opaque ID; this grants no A fetch permission.
    base.request(&shared_b);
    drop(gate);
    old_work.await;
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    let requested = Arc::new(std::sync::Mutex::new(Vec::new()));
    let requests = requested.clone();
    crate::media::load_with(
        state,
        transport(state),
        move |request| {
            let requests = requests.clone();
            async move {
                requests.lock().unwrap().push((
                    request.url.clone(),
                    request.authenticated,
                    request.transport.d_cookie().to_owned(),
                ));
                Ok(TRANSPARENT_PNG.to_vec())
            }
        },
        Hooks::default(),
    )
    .await;
    let requests = requested.lock().unwrap();
    assert_eq!(requests.len(), 2);
    assert!(requests.contains(&(
        "https://files.slack.com/shared".into(),
        true,
        "synthetic-B".into()
    )));
    assert!(requests.contains(&(
        "https://cdn.example.test/public.png".into(),
        false,
        "synthetic-B".into()
    )));
    drop(requests);
    assert_eq!(get(&base, &shared_b).body().as_ref(), TRANSPARENT_PNG);
    assert!(!a.sources.read().unwrap()[&shared_a].deferred);
    assert!(!a.is_ready(&only_a));
    assert!(matches!(
        a.backoff.read().unwrap().get(&old),
        Some(Backoff::Abandoned)
    ));
    let returned = shell(&base, "A", "synthetic-A", Some(original));
    let c = returned.media.clone();
    let shared_c = c.register_image(
        MediaAssetKind::Attachment,
        "https://files.slack.com/shared",
        "image/png",
    );
    assert_ne!(shared_c, shared_a);
    crate::bootstrap::install_loaded(state, returned);
    late.await;
    assert_eq!(late_calls.load(Ordering::SeqCst), 0);
    assert!(!a.scope.active());
    assert!(c.scope.active());
}
#[tokio::test(flavor = "current_thread")]
async fn retirement_during_network_permit_and_disk_waits_rejects_late_data_and_allows_current_owner_progress()
 {
    let persistent = store();
    persistent
        .store(
            "avatar/U_DISK",
            "https://files.slack.com/disk-A",
            PLACEHOLDER_PNG,
        )
        .unwrap();
    let base = MediaRegistry {
        store: Some(persistent),
        ..Default::default()
    };
    let h = setup(&base);
    let state = h.state;
    let a = state.peek().media.clone();
    let disk = a.register_avatar("U_DISK", "https://files.slack.com/disk-A");
    let (entered, release, hook) = held_hook();
    let disk_calls = Arc::new(AtomicUsize::new(0));
    let disk_requests = disk_calls.clone();
    let mut work = Box::pin(crate::media::load_with(
        state,
        transport(state),
        move |_| {
            let calls = disk_requests.clone();
            async move {
                calls.fetch_add(1, Ordering::SeqCst);
                Ok(TRANSPARENT_PNG.to_vec())
            }
        },
        Hooks {
            after_disk_read: Some(hook),
            ..Default::default()
        },
    ));
    until(&mut work, || entered.load(Ordering::SeqCst)).await;
    let loaded = shell(&base, "B", "synthetic-B", None);
    let b = loaded.media.clone();
    crate::bootstrap::install_loaded(state, loaded);
    work.await;
    assert!(!a.is_loading());
    assert!(!a.is_ready(&disk));
    assert_eq!(disk_calls.load(Ordering::SeqCst), 0);
    release.send(()).unwrap();
    // Thirteen real queued sources exercise the unchanged twelve-request permit bound.
    let ids = (0..13)
        .map(|i| {
            b.register_image(
                MediaAssetKind::Attachment,
                &format!("https://files.slack.com/permit-{i}"),
                "image/png",
            )
        })
        .collect::<Vec<_>>();
    let started = Arc::new(AtomicUsize::new(0));
    let calls = started.clone();
    let mut network = Box::pin(crate::media::load_with(
        state,
        transport(state),
        move |request| {
            let calls = calls.clone();
            async move {
                assert_eq!(request.transport.d_cookie(), "synthetic-B");
                calls.fetch_add(1, Ordering::SeqCst);
                std::future::pending::<Result<Vec<u8>, Error>>().await
            }
        },
        Hooks::default(),
    ));
    until(&mut network, || started.load(Ordering::SeqCst) == 12).await;
    let loaded = shell(&base, "C", "synthetic-C", None);
    let c = loaded.media.clone();
    let current = c.register_image(
        MediaAssetKind::Attachment,
        "https://files.slack.com/current-C",
        "image/png",
    );
    crate::bootstrap::install_loaded(state, loaded);
    network.await;
    tokio::task::yield_now().await;
    assert_eq!(started.load(Ordering::SeqCst), 12);
    assert!(ids.iter().all(|id| !b.is_ready(id)));
    assert!(b.backoff.read().unwrap().is_empty());
    assert!(!a.is_ready(&disk));
    let current_requests = Arc::new(std::sync::Mutex::new(Vec::new()));
    let observed = current_requests.clone();
    crate::media::load_with(
        state,
        transport(state),
        move |request| {
            let observed = observed.clone();
            async move {
                observed.lock().unwrap().push((
                    request.url,
                    request.authenticated,
                    request.transport.d_cookie().to_owned(),
                ));
                Ok(TRANSPARENT_PNG.to_vec())
            }
        },
        Hooks::default(),
    )
    .await;
    assert_eq!(
        *current_requests.lock().unwrap(),
        [(
            "https://files.slack.com/current-C".into(),
            true,
            "synthetic-C".into()
        )]
    );
    assert!(c.is_ready(&current));
    assert_eq!(get(&base, &current).body().as_ref(), TRANSPARENT_PNG);
}
#[tokio::test(flavor = "current_thread")]
async fn scoped_sources_preserve_resident_and_disk_picture_reuse_without_sharing_unslotted_private_bytes_or_unbounded_memo()
 {
    let persistent = store();
    persistent
        .store(
            "avatar/U_EXACT",
            "https://files.slack.com/exact",
            PLACEHOLDER_PNG,
        )
        .unwrap();
    persistent
        .store(
            "avatar/U_OLD",
            "https://files.slack.com/old",
            PLACEHOLDER_PNG,
        )
        .unwrap();
    let base = MediaRegistry {
        store: Some(persistent),
        ..Default::default()
    };
    let h = setup(&base);
    let state = h.state;
    let a = state.peek().media.clone();
    let picture = a.register_avatar("U_RAM", "https://files.slack.com/ram");
    a.insert_asset(
        picture.clone(),
        "image/png".into(),
        Arc::from(PLACEHOLDER_PNG),
        true,
    );
    let revision = a.image_state(&picture).0.unwrap();
    assert!(a.reject_image(&picture, revision));
    let private = a.register_image(
        MediaAssetKind::Attachment,
        "https://files.slack.com/private",
        "image/png",
    );
    a.insert(private.clone(), "image/png", PLACEHOLDER_PNG);
    let loaded = shell(&base, "B", "synthetic-B", None);
    let b = loaded.media.clone();
    let reused = b.register_avatar("U_RAM", "https://files.slack.com/ram");
    assert_ne!(picture, reused);
    {
        let mut cache = b.assets.lock().unwrap();
        let current = cache.get(&reused).unwrap();
        assert!(current.provisional && current.image_failed);
        assert_ne!(current.revision, revision);
        assert_eq!(current.bytes.as_ref(), PLACEHOLDER_PNG);
    }
    let isolated = b.register_image(
        MediaAssetKind::Attachment,
        "https://files.slack.com/private",
        "image/png",
    );
    assert_ne!(private, isolated);
    assert!(!b.is_ready(&isolated));
    let exact = b.register_avatar("U_EXACT", "https://files.slack.com/exact");
    let provisional = b.register_avatar("U_OLD", "https://files.slack.com/new");
    crate::bootstrap::install_loaded(state, loaded);
    let (send, receive) = tokio::sync::oneshot::channel();
    let held = Arc::new(std::sync::Mutex::new(Some(receive)));
    let entered = Arc::new(AtomicBool::new(false));
    let started = entered.clone();
    let requested = Arc::new(std::sync::Mutex::new(Vec::new()));
    let requests = requested.clone();
    let mut work = Box::pin(crate::media::load_with(
        state,
        transport(state),
        move |request| {
            let held = held.clone();
            let started = started.clone();
            let requests = requests.clone();
            async move {
                requests.lock().unwrap().push((
                    request.url.clone(),
                    request.authenticated,
                    request.transport.d_cookie().to_owned(),
                ));
                if request.url == "https://files.slack.com/new" {
                    let receive = held.lock().unwrap().take().unwrap();
                    started.store(true, Ordering::SeqCst);
                    receive.await.unwrap();
                }
                Ok(TRANSPARENT_PNG.to_vec())
            }
        },
        Hooks::default(),
    ));
    until(&mut work, || entered.load(Ordering::SeqCst)).await;
    assert_eq!(b.ready_bytes(&exact).unwrap().as_ref(), PLACEHOLDER_PNG);
    assert!(!b.assets.lock().unwrap().get(&exact).unwrap().provisional);
    // Provisional pictures may paint but never qualify as original downloads.
    assert!(b.is_ready(&provisional));
    assert!(b.ready_bytes(&provisional).is_none());
    let painted = get(&base, &provisional);
    assert_eq!(painted.body().as_ref(), PLACEHOLDER_PNG);
    assert_eq!(painted.headers()["Cache-Control"], "no-store");
    assert!(
        b.assets
            .lock()
            .unwrap()
            .get(&provisional)
            .unwrap()
            .provisional
    );
    send.send(()).unwrap();
    work.await;
    let requests = requested.lock().unwrap();
    assert_eq!(requests.len(), 3);
    assert!(
        requests
            .iter()
            .all(|(_, authenticated, cookie)| *authenticated && cookie == "synthetic-B")
    );
    for url in [
        "https://files.slack.com/new",
        "https://files.slack.com/private",
        "https://files.slack.com/ram",
    ] {
        assert_eq!(
            requests
                .iter()
                .filter(|(requested, _, _)| requested == url)
                .count(),
            1
        );
    }
    drop(requests);
    assert_eq!(b.ready_bytes(&isolated).unwrap().as_ref(), TRANSPARENT_PNG);
    assert!(!b.assets.lock().unwrap().get(&reused).unwrap().image_failed);
    for i in 0..(MAX_RESIDENT_ASSETS + 2) {
        let id = b.register_avatar(
            &format!("U_BOUND_{i}"),
            &format!("https://files.slack.com/bound-{i}"),
        );
        b.insert(id, "image/png", TRANSPARENT_PNG);
    }
    let cache = b.assets.lock().unwrap();
    assert!(cache.pictures.len() <= cache.entries.len());
    assert!(cache.pictures.len() <= MAX_RESIDENT_ASSETS);
    assert!(
        cache
            .pictures
            .values()
            .all(|id| cache.entries.contains_key(id))
    );
}
#[tokio::test(flavor = "current_thread")]
async fn admitted_cache_writer_keeps_owned_serialization_after_outer_cancellation_and_successor_write_wins()
 {
    let persistent = store();
    let base = MediaRegistry {
        store: Some(persistent.clone()),
        ..Default::default()
    };
    let h = setup(&base);
    let state = h.state;
    let a = state.peek().media.clone();
    let old = a.register_avatar("U_COMMIT", "https://files.slack.com/commit-A");
    let (entered, release, hook) = held_hook();
    let mut work = Box::pin(crate::media::load_with(
        state,
        transport(state),
        |_| async { Ok(PLACEHOLDER_PNG.to_vec()) },
        Hooks {
            after_write_admitted: Some(hook),
            ..Default::default()
        },
    ));
    until(&mut work, || entered.load(Ordering::SeqCst)).await;
    assert!(a.is_loading());
    assert!(a.is_ready(&old));
    let loaded = shell(&base, "B", "synthetic-B", None);
    let b = loaded.media.clone();
    let current = b.register_avatar("U_COMMIT", "https://files.slack.com/commit-B");
    assert!(a.scope.active());
    crate::bootstrap::install_loaded(state, loaded);
    drop(work);
    assert!(
        b.is_loading(),
        "worker retains guard after awaiter cancellation"
    );
    let requests = Arc::new(AtomicUsize::new(0));
    let calls = requests.clone();
    let mut successor = Box::pin(crate::media::load_with(
        state,
        transport(state),
        move |request| {
            let calls = calls.clone();
            async move {
                assert_eq!(request.url, "https://files.slack.com/commit-B");
                assert_eq!(request.transport.d_cookie(), "synthetic-B");
                calls.fetch_add(1, Ordering::SeqCst);
                Ok(TRANSPARENT_PNG.to_vec())
            }
        },
        Hooks::default(),
    ));
    pending(successor.as_mut()).await;
    assert_eq!(requests.load(Ordering::SeqCst), 0);
    release.send(()).unwrap();
    successor.await;
    let stored = persistent
        .load("avatar/U_COMMIT", "https://files.slack.com/commit-B")
        .unwrap();
    assert!(stored.current);
    assert_eq!(stored.bytes, TRANSPARENT_PNG);
    assert_eq!(b.ready_bytes(&current).unwrap().as_ref(), TRANSPARENT_PNG);
    assert_eq!(a.ready_bytes(&old).unwrap().as_ref(), PLACEHOLDER_PNG);
    assert!(!b.is_loading());
}
