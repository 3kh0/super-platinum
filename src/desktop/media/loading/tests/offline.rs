//! Observable offline picture recovery through the actual captured media job.
use super::*;
fn lookup_counter(count: &Arc<AtomicUsize>) -> Hooks {
    let count = count.clone();
    Hooks {
        on_disk_lookup: Some(Arc::new(move || {
            count.fetch_add(1, Ordering::SeqCst);
        })),
        ..Default::default()
    }
}
async fn local_load(
    state: Signal<ShellState>,
    lookups: &Arc<AtomicUsize>,
    calls: &Arc<AtomicUsize>,
) {
    let calls = calls.clone();
    crate::media::load_with(
        state,
        transport(state),
        move |_| {
            let calls = calls.clone();
            async move {
                calls.fetch_add(1, Ordering::SeqCst);
                Ok(TRANSPARENT_PNG.to_vec())
            }
        },
        lookup_counter(lookups),
    )
    .await;
}
#[tokio::test(flavor = "current_thread")]
async fn offline_persistent_pictures_load_once_without_http_or_backoff_spend_and_recover_online() {
    let persistent = store();
    for (slot, url) in [
        ("avatar/U_EXACT", "https://files.slack.com/exact"),
        ("avatar/U_OLD", "https://files.slack.com/old"),
        ("emoji/visible", "https://emoji.slack-edge.com/visible"),
        ("emoji/unused", "https://emoji.slack-edge.com/unused"),
    ] {
        persistent.store(slot, url, PLACEHOLDER_PNG).unwrap();
    }
    let base = MediaRegistry {
        store: Some(persistent),
        ..Default::default()
    };
    let h = setup(&base);
    let state = h.state;
    let media = state.peek().media.clone();
    let link = transport(state);
    link.set_health(Health::Offline);
    let exact = media.register_avatar("U_EXACT", "https://files.slack.com/exact");
    let rotated = media.register_avatar("U_OLD", "https://files.slack.com/new");
    let missing = media.register_avatar("U_MISSING", "https://files.slack.com/missing");
    let private = media.register_image(
        MediaAssetKind::Attachment,
        "https://files.slack.com/private",
        "image/png",
    );
    let visible =
        media.register_emoji_on_display("visible", "https://emoji.slack-edge.com/visible");
    let unused = media.register_emoji_on_display("unused", "https://emoji.slack-edge.com/unused");
    let deadline = Instant::now() + Duration::from_secs(3600);
    {
        let mut backoff = media.backoff.write().unwrap();
        backoff.insert(exact.clone(), Backoff::Abandoned);
        backoff.insert(
            rotated.clone(),
            Backoff::Retry {
                failures: 4,
                at: deadline,
            },
        );
    }
    let lookups = Arc::new(AtomicUsize::new(0));
    let calls = Arc::new(AtomicUsize::new(0));
    assert!(media.has_pending_for(Health::Offline));
    local_load(state, &lookups, &calls).await;
    assert_eq!(lookups.load(Ordering::SeqCst), 3);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(media.ready_bytes(&exact).unwrap().as_ref(), PLACEHOLDER_PNG);
    assert!(media.is_ready(&rotated));
    assert!(media.ready_bytes(&rotated).is_none());
    let painted = get(&base, &rotated);
    assert_eq!(painted.body().as_ref(), PLACEHOLDER_PNG);
    assert_eq!(painted.headers()["Cache-Control"], "no-store");
    assert!(!media.is_ready(&missing));
    assert!(!media.is_ready(&private));
    assert!(!media.is_ready(&visible));
    assert!(!media.is_ready(&unused));
    assert!(matches!(
        media.backoff.read().unwrap().get(&exact),
        Some(Backoff::Abandoned)
    ));
    assert!(
        matches!(media.backoff.read().unwrap().get(&rotated), Some(Backoff::Retry { failures: 4, at }) if *at==deadline)
    );
    for _ in 0..3 {
        let _ = get(&base, &missing);
        let _ = get(&base, &rotated);
        assert!(!media.has_pending_for(Health::Offline));
        local_load(state, &lookups, &calls).await;
    }
    assert_eq!(lookups.load(Ordering::SeqCst), 3);
    let _ = get(&base, &visible);
    assert!(media.has_pending_for(Health::Offline));
    local_load(state, &lookups, &calls).await;
    assert_eq!(lookups.load(Ordering::SeqCst), 4);
    assert_eq!(
        media.ready_bytes(&visible).unwrap().as_ref(),
        PLACEHOLDER_PNG
    );
    assert!(!media.is_ready(&unused));
    assert!(!media.has_pending_for(Health::Offline));
    // Capture while offline, then reconnect before the future's first poll.
    // It must remain cache-only despite outstanding uncached HTTP candidates.
    let observed = calls.clone();
    let captured = crate::media::load_with(
        state,
        link.clone(),
        move |_| {
            let observed = observed.clone();
            async move {
                observed.fetch_add(1, Ordering::SeqCst);
                Ok(TRANSPARENT_PNG.to_vec())
            }
        },
        lookup_counter(&lookups),
    );
    link.set_health(Health::Online);
    captured.await;
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(lookups.load(Ordering::SeqCst), 4);
    // Explicit renewed demand retains the established backoff-clearing contract.
    media.request(&rotated);
    let requests = Arc::new(std::sync::Mutex::new(Vec::new()));
    let seen = requests.clone();
    crate::media::load_with(
        state,
        link,
        move |request| {
            let seen = seen.clone();
            async move {
                seen.lock().unwrap().push((
                    request.url,
                    request.authenticated,
                    request.transport.d_cookie().to_owned(),
                ));
                Ok(TRANSPARENT_PNG.to_vec())
            }
        },
        lookup_counter(&lookups),
    )
    .await;
    let mut observed = requests.lock().unwrap().clone();
    observed.sort();
    assert_eq!(
        observed,
        ["missing", "new", "private"].map(|name| (
            format!("https://files.slack.com/{name}"),
            true,
            "synthetic-A".to_owned()
        ))
    );
    for id in [&rotated, &missing, &private] {
        assert_eq!(media.ready_bytes(id).unwrap().as_ref(), TRANSPARENT_PNG);
    }
    assert_eq!(media.ready_bytes(&exact).unwrap().as_ref(), PLACEHOLDER_PNG);
    assert!(!media.is_ready(&unused));
}
#[tokio::test(flavor = "current_thread")]
async fn new_demand_rejects_old_disk_completion_and_evicted_pictures_recover_once_per_renewed_demand()
 {
    let persistent = store();
    let url = "https://files.slack.com/demand";
    persistent
        .store("avatar/U_DEMAND", url, PLACEHOLDER_PNG)
        .unwrap();
    let base = MediaRegistry {
        store: Some(persistent.clone()),
        ..Default::default()
    };
    let h = setup(&base);
    let state = h.state;
    let media = state.peek().media.clone();
    let link = transport(state);
    let id = media.register_avatar("U_DEMAND", url);
    let blocked_url = "https://files.slack.com/backoff-cached";
    persistent
        .store("avatar/U_BLOCKED", blocked_url, PLACEHOLDER_PNG)
        .unwrap();
    let blocked = media.register_avatar("U_BLOCKED", blocked_url);
    media
        .backoff
        .write()
        .unwrap()
        .insert(blocked.clone(), Backoff::Abandoned);
    let lookups = Arc::new(AtomicUsize::new(0));
    let calls = Arc::new(AtomicUsize::new(0));
    let (entered, release, hook) = held_hook();
    let observed = calls.clone();
    let mut hooks = lookup_counter(&lookups);
    hooks.after_disk_read = Some(hook);
    let mut old = Box::pin(crate::media::load_with(
        state,
        transport(state),
        move |_| {
            let observed = observed.clone();
            async move {
                observed.fetch_add(1, Ordering::SeqCst);
                Ok(TRANSPARENT_PNG.to_vec())
            }
        },
        hooks,
    ));
    until(&mut old, || entered.load(Ordering::SeqCst)).await;
    assert_eq!(lookups.load(Ordering::SeqCst), 1);
    media.request(&id);
    persistent
        .store("avatar/U_DEMAND", url, TRANSPARENT_PNG)
        .unwrap();
    // This job was captured Network. Its pending list excluded the backoff
    // slot. Becoming Offline across disk hydration must refuse all HTTP, and
    // the following local sweep must still hydrate that excluded slot.
    link.set_health(Health::Offline);
    release.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(5), old)
        .await
        .expect("held disk completion settles");
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert!(!media.is_ready(&blocked));
    assert!(
        !media.is_ready(&id),
        "old read cannot publish into newer demand"
    );
    assert!(
        media.has_pending_for(Health::Offline),
        "old completion cannot consume newer demand"
    );
    local_load(state, &lookups, &calls).await;
    assert_eq!(lookups.load(Ordering::SeqCst), 3);
    assert_eq!(
        media.ready_bytes(&blocked).unwrap().as_ref(),
        PLACEHOLDER_PNG
    );
    assert!(matches!(
        media.backoff.read().unwrap().get(&blocked),
        Some(Backoff::Abandoned)
    ));
    assert_eq!(media.ready_bytes(&id).unwrap().as_ref(), TRANSPARENT_PNG);
    for readiness_path in [false, true] {
        // Exercise the real bounded asset-cache eviction, not a source reset.
        for _ in 0..=MAX_RESIDENT_ASSETS {
            media.insert(
                MediaAssetId::new(MediaAssetKind::Attachment),
                "image/png",
                TRANSPARENT_PNG,
            );
        }
        assert!(media.assets.lock().unwrap().evicted.contains(&id));
        assert!(!media.has_pending_for(Health::Offline));
        if readiness_path {
            assert!(!media.is_ready(&id));
        } else {
            let response = get(&base, &id);
            assert_eq!(response.status(), StatusCode::OK);
        }
        assert!(media.has_pending_for(Health::Offline));
        let previous = lookups.load(Ordering::SeqCst);
        local_load(state, &lookups, &calls).await;
        assert_eq!(lookups.load(Ordering::SeqCst), previous + 1);
        assert_eq!(get(&base, &id).body().as_ref(), TRANSPARENT_PNG);
        assert!(!media.has_pending_for(Health::Offline));
        local_load(state, &lookups, &calls).await;
        assert_eq!(lookups.load(Ordering::SeqCst), previous + 1);
    }
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}
#[tokio::test(flavor = "current_thread")]
async fn queued_network_request_refuses_known_offline_admission_without_spending_backoff_and_can_resume()
 {
    let base = MediaRegistry::default();
    let h = setup(&base);
    let state = h.state;
    let media = state.peek().media.clone();
    let link = transport(state);
    let ids = (0..13)
        .map(|index| {
            media.register_image(
                MediaAssetKind::Attachment,
                &format!("https://files.slack.com/queued-{index}"),
                "image/png",
            )
        })
        .collect::<Vec<_>>();
    let release = Arc::new(tokio::sync::Notify::new());
    let released = release.clone();
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = calls.clone();
    let mut work = Box::pin(crate::media::load_with(
        state,
        link.clone(),
        move |_| {
            let released = released.clone();
            let observed = observed.clone();
            async move {
                observed.fetch_add(1, Ordering::SeqCst);
                released.notified().await;
                Ok(TRANSPARENT_PNG.to_vec())
            }
        },
        Hooks::default(),
    ));
    until(&mut work, || calls.load(Ordering::SeqCst) == 12).await;
    link.set_health(Health::Offline);
    release.notify_waiters();
    tokio::time::timeout(Duration::from_secs(5), work)
        .await
        .expect("queued request must settle after offline admission refusal");
    assert_eq!(
        calls.load(Ordering::SeqCst),
        12,
        "thirteenth request must refuse after permit acquisition"
    );
    assert_eq!(
        ids.iter()
            .filter(|id| media.ready_bytes(id).is_some())
            .count(),
        12
    );
    assert!(media.backoff.read().unwrap().is_empty());
    assert!(
        !media.has_pending_for(Health::Offline),
        "unslotted network-only work must not spin offline"
    );
    link.set_health(Health::Online);
    assert!(media.has_pending_for(Health::Online));
    let resumed = Arc::new(AtomicUsize::new(0));
    let observed = resumed.clone();
    crate::media::load_with(
        state,
        link,
        move |_| {
            let observed = observed.clone();
            async move {
                observed.fetch_add(1, Ordering::SeqCst);
                Ok(TRANSPARENT_PNG.to_vec())
            }
        },
        Hooks::default(),
    )
    .await;
    assert_eq!(resumed.load(Ordering::SeqCst), 1);
    assert!(ids.iter().all(|id| media.ready_bytes(id).is_some()));
}
