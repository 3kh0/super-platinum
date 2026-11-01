//! Actual mounted preview callbacks, original typed request construction and
//! real fresh-projection/decode/publication; network is mandatory scoped IO.
use super::*;
use pages::preview::{self, Display, test_io::Io};
use serde_json::json;
fn page() -> serde_json::Value {
    json!({"ok":true,"items":[{"id":"F12345678","title":"Launch image","pretty_type":"PNG"},{"id":"F12345678","title":"Same file, another occurrence","pretty_type":"PNG"}],"pagination":{"page":1,"page_count":2,"total_count":3}})
}
fn metadata() -> serde_json::Value {
    json!({"ok":true,"file":{"id":"F12345678","mode":"hosted","filetype":"png","mimetype":"image/png","thumb_360":"https://files.slack.com/preview.png","file_access":null,"is_hidden_by_limit":false,"is_tombstoned":false,"restriction_type":0,"is_external":false,"external_type":null,"app_id":null,"created":1}})
}
#[tokio::test]
async fn isolated_image_mounted_preview_retry_duplicate_close_and_owner_retirement_preserve_search()
{
    static EVENTS: std::sync::Once = std::sync::Once::new();
    EVENTS.call_once(|| {
        dioxus::html::set_event_converter(Box::new(dioxus::html::SerializedHtmlEventConverter))
    });
    let h = pages::tests::setup(true).await;
    let mut old = h.h.state;
    let shell = std::mem::replace(
        &mut *old.write(),
        ShellState::fixture(crate::media::MediaRegistry::default()),
    );
    type Input = Rc<RefCell<Option<ShellState>>>;
    type Slot = Rc<Cell<Option<Signal<ShellState>>>>;
    let slot = Slot::default();
    let mut dom = VirtualDom::new_with_props(
        |(input, slot): (Input, Slot)| {
            let state = use_signal(|| input.borrow_mut().take().unwrap());
            slot.set(Some(state));
            rsx! {{results(state,&state.read())}{controls(state,&state.read())}}
        },
        (Rc::new(RefCell::new(Some(shell))), slot.clone()),
    );
    let mut attributes = Attributes::new();
    record(&mut attributes, dom.rebuild_to_vec().edits);
    let mut state = slot.get().unwrap();
    let pages_io = Rc::new(pages::test_io::Io::default());
    let io = Rc::new(Io::default());
    dom.in_scope(ScopeId::ROOT, || {
        provide_context(pages_io.clone());
        provide_context(io.clone());
    });
    let reply = pages_io.expect(1);
    let source = pages::EntrySource::capture(&state.peek()).unwrap();
    dom.in_scope(ScopeId::ROOT, || {
        pages::change_sort(
            state,
            source,
            super_platinum_core::slack::api::file_pages::Sort::Newest,
        )
    });
    pump(&mut dom, &mut attributes, || {
        pages_io.pages.borrow().len() == 1
    })
    .await;
    reply.send(Ok(page())).unwrap();
    pump(&mut dom, &mut attributes, || {
        pages::status(&state.peek()).is_some_and(|v| !v.4)
    })
    .await;
    let query = state.peek().search_query.clone();
    let channel = state.peek().core.active_channel.clone();
    let before_sources = state
        .peek()
        .media
        .registered_source_count("https://files.slack.com/preview.png");
    let first = mounted_element(&dom, "id", "file-image-preview-0");
    let reply = io.expect(false);
    click(&dom, first);
    click(&dom, first);
    pump(&mut dom, &mut attributes, || io.calls.borrow().len() == 1).await;
    reply
        .send(Err(super_platinum_core::slack::Error::Offline(
            "fixture outage".into(),
        )))
        .unwrap();
    pump(&mut dom, &mut attributes, || {
        matches!(preview::display(&state.peek(), 0), Some(Display::Failed))
    })
    .await;
    assert_eq!(pages::rows(&state.peek()).len(), 2);
    let reply = io.expect(false);
    let ip = io.expect(true);
    let bytes = io.expect_bytes();
    click(&dom, mounted_element(&dom, "id", "file-image-retry-0"));
    pump(&mut dom, &mut attributes, || io.calls.borrow().len() == 2).await;
    reply.send(Ok(metadata())).unwrap();
    pump(&mut dom, &mut attributes, || io.calls.borrow().len() == 3).await;
    ip.send(Ok(json!({"ok":true,"ip_allowed":true}))).unwrap();
    pump(&mut dom, &mut attributes, || {
        io.byte_urls.borrow().len() == 1
    })
    .await;
    bytes
        .send(Ok(include_bytes!(
            "../../../../../../assets/icons/icon-512.png"
        )
        .to_vec()))
        .unwrap();
    pump(&mut dom, &mut attributes, || {
        matches!(preview::display(&state.peek(), 0), Some(Display::Image(_)))
    })
    .await;
    let Display::Image(id) = preview::display(&state.peek(), 0).unwrap() else {
        panic!()
    };
    assert!(state.peek().media.is_ready(&id));
    // BEGIN local zoom mount assertions (existing acquisition journey).
    let zoom_key = preview::dom_key(&state.peek(), 0).unwrap();
    assert!(zoom_key.starts_with("image-zoom-"));
    let _zoom_root = mounted_element(&dom, "id", &zoom_key);
    assert!(preview::dom_key(&state.peek(), 1).is_none());
    // END local zoom mount assertions.
    // BEGIN local zoom step markup (same original acquisition and ready PNG).
    let _zoom_in = mounted_element(&dom, "data-image-zoom-plus", "true");
    let _zoom_out = mounted_element(&dom, "data-image-zoom-minus", "true");
    // END local zoom step markup.
    assert!(state.peek().viewer.is_none());
    assert_eq!(state.peek().overlay, Some(crate::state::Overlay::Search));
    assert_eq!(state.peek().search_query, query);
    assert_eq!(state.peek().core.active_channel, channel);
    assert_eq!(
        state
            .peek()
            .media
            .registered_source_count("https://files.slack.com/preview.png"),
        before_sources
    );
    assert!(preview::display(&state.peek(), 1).is_none());
    // A duplicate-id row is a separate occurrence; its in-flight response must
    // not inherit the first image or survive the real Close callback.
    let reply = io.expect(false);
    click(&dom, mounted_element(&dom, "id", "file-image-preview-1"));
    pump(&mut dom, &mut attributes, || io.calls.borrow().len() == 4).await;
    assert!(preview::display(&state.peek(), 0).is_none());
    click(&dom, mounted_element(&dom, "id", "file-image-close-1"));
    reply.send(Ok(metadata())).unwrap();
    pump(&mut dom, &mut attributes, || io.completed.get() == 4).await;
    assert!(preview::display(&state.peek(), 1).is_none());
    assert_eq!(io.calls.borrow().len(), 4);
    // Current metadata must refuse an IP denial, without fetching image bytes.
    let reply = io.expect(false);
    let ip = io.expect(true);
    click(&dom, mounted_element(&dom, "id", "file-image-preview-0"));
    pump(&mut dom, &mut attributes, || io.calls.borrow().len() == 5).await;
    reply.send(Ok(metadata())).unwrap();
    pump(&mut dom, &mut attributes, || io.calls.borrow().len() == 6).await;
    ip.send(Ok(json!({"ok":true,"ip_allowed":false}))).unwrap();
    pump(&mut dom, &mut attributes, || {
        matches!(
            preview::display(&state.peek(), 0),
            Some(Display::Unavailable)
        )
    })
    .await;
    assert_eq!(io.byte_urls.borrow().len(), 1);
    click(&dom, mounted_element(&dom, "id", "file-image-close-0"));
    assert!(preview::display(&state.peek(), 0).is_none());
    record(&mut attributes, dom.render_immediate_to_vec().edits);
    let reply = io.expect(false);
    let ip = io.expect(true);
    let bytes = io.expect_bytes();
    click(&dom, mounted_element(&dom, "id", "file-image-preview-0"));
    pump(&mut dom, &mut attributes, || io.calls.borrow().len() == 7).await;
    reply.send(Ok(metadata())).unwrap();
    pump(&mut dom, &mut attributes, || io.calls.borrow().len() == 8).await;
    ip.send(Ok(json!({"ok":true,"ip_allowed":true}))).unwrap();
    pump(&mut dom, &mut attributes, || {
        io.byte_urls.borrow().len() == 2
    })
    .await;
    state.write().core.account_epoch += 1;
    bytes
        .send(Ok(include_bytes!(
            "../../../../../../assets/icons/icon-512.png"
        )
        .to_vec()))
        .unwrap();
    pump(&mut dom, &mut attributes, || io.byte_completed.get() == 2).await;
    assert!(preview::display(&state.peek(), 0).is_none());
    assert_eq!(io.calls.borrow().len(), 8);
}

#[test]
fn isolated_image_named_fixtures_are_inert_and_action_free() {
    for name in [
        "search-files-image-preview",
        "search-files-image-preview-narrow",
    ] {
        let (_dom, state, captured) = crate::resource_reads::render_test::mount(
            name,
            |state| rsx! {{results(state,&state.read())}},
        );
        let s = state.peek();
        assert!(s.search_file_image_fixture.is_some());
        assert!(s.viewer.is_none());
        assert!(preview::Source::capture(&s, 1).is_none());
        let text = crate::resource_reads::render_test::text(captured.borrow().as_ref().unwrap());
        assert!(text.contains("Close preview"));
        for excluded in [
            "Download",
            "Share",
            "Rename",
            "Copy link",
            "Open in browser",
        ] {
            assert!(!text.contains(excluded), "{excluded}");
        }
    }
}

fn mount_shell(shell: ShellState) -> (VirtualDom, Signal<ShellState>, Attributes) {
    type Input = Rc<RefCell<Option<ShellState>>>;
    type Slot = Rc<Cell<Option<Signal<ShellState>>>>;
    let slot = Slot::default();
    let mut dom = VirtualDom::new_with_props(
        |(input, slot): (Input, Slot)| {
            let state = use_signal(|| input.borrow_mut().take().unwrap());
            slot.set(Some(state));
            rsx! {{results(state,&state.read())}{controls(state,&state.read())}}
        },
        (Rc::new(RefCell::new(Some(shell))), slot.clone()),
    );
    let mut attributes = Attributes::new();
    record(&mut attributes, dom.rebuild_to_vec().edits);
    (dom, slot.get().unwrap(), attributes)
}
/// The same silent poll + explicit notification as runtime::ticks, without a
/// native window/device probe or a substitute permission/readiness snapshot.
#[allow(deprecated)]
fn poll_cooldown(mut state: Signal<ShellState>) -> bool {
    let changed = pages::poll(&mut state.write_silent());
    if changed {
        drop(state.write());
    }
    changed
}
#[tokio::test]
async fn isolated_image_retry_after_expiry_repaints_disabled_control_without_replaying_request() {
    static EVENTS: std::sync::Once = std::sync::Once::new();
    EVENTS.call_once(|| {
        dioxus::html::set_event_converter(Box::new(dioxus::html::SerializedHtmlEventConverter))
    });
    let h = pages::tests::setup(true).await;
    let mut old = h.h.state;
    let shell = std::mem::replace(
        &mut *old.write(),
        ShellState::fixture(crate::media::MediaRegistry::default()),
    );
    let (mut dom, mut state, mut attributes) = mount_shell(shell);
    let pages_io = Rc::new(pages::test_io::Io::default());
    let io = Rc::new(Io::default());
    dom.in_scope(ScopeId::ROOT, || {
        provide_context(pages_io.clone());
        provide_context(io.clone());
    });
    let reply = pages_io.expect(1);
    let source = pages::EntrySource::capture(&state.peek()).unwrap();
    dom.in_scope(ScopeId::ROOT, || {
        pages::change_sort(
            state,
            source,
            super_platinum_core::slack::api::file_pages::Sort::Newest,
        )
    });
    pump(&mut dom, &mut attributes, || {
        pages_io.pages.borrow().len() == 1
    })
    .await;
    reply.send(Ok(page())).unwrap();
    pump(&mut dom, &mut attributes, || {
        pages::status(&state.peek()).is_some_and(|v| !v.4)
    })
    .await;
    let reply = io.expect(false);
    click(&dom, mounted_element(&dom, "id", "file-image-preview-0"));
    pump(&mut dom, &mut attributes, || io.calls.borrow().len() == 1).await;
    reply
        .send(Err(super_platinum_core::slack::Error::RateLimited {
            retry_after_secs: Some(1),
        }))
        .unwrap();
    pump(&mut dom, &mut attributes, || {
        matches!(preview::display(&state.peek(), 0), Some(Display::Failed))
    })
    .await;
    let retry = mounted_element(&dom, "id", "file-image-retry-0");
    assert_eq!(attributes[&(retry, "disabled".into())], "true");
    assert!(
        poll_cooldown(state),
        "original visible cooling transition is recorded"
    );
    record(&mut attributes, dom.render_immediate_to_vec().edits);
    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
    assert!(!preview::cooling(&state.peek()));
    assert_eq!(
        attributes[&(retry, "disabled".into())],
        "true",
        "time alone did not repaint this real mounted control"
    );
    assert!(
        poll_cooldown(state),
        "expiry needs exactly one normal runtime notification"
    );
    record(&mut attributes, dom.render_immediate_to_vec().edits);
    assert_eq!(attributes[&(retry, "disabled".into())], "false");
    assert!(!poll_cooldown(state));
    assert_eq!(
        io.calls.borrow().len(),
        1,
        "expiry never automatically replays a request"
    );
    let reply = io.expect(false);
    let ip = io.expect(true);
    let bytes = io.expect_bytes();
    click(&dom, mounted_element(&dom, "id", "file-image-retry-0"));
    pump(&mut dom, &mut attributes, || io.calls.borrow().len() == 2).await;
    reply.send(Ok(metadata())).unwrap();
    pump(&mut dom, &mut attributes, || io.calls.borrow().len() == 3).await;
    ip.send(Ok(json!({"ok":true,"ip_allowed":true}))).unwrap();
    pump(&mut dom, &mut attributes, || {
        io.byte_urls.borrow().len() == 1
    })
    .await;
    bytes
        .send(Ok(include_bytes!(
            "../../../../../../assets/icons/icon-512.png"
        )
        .to_vec()))
        .unwrap();
    pump(&mut dom, &mut attributes, || {
        matches!(preview::display(&state.peek(), 0), Some(Display::Image(_)))
    })
    .await;
    assert_eq!(pages::rows(&state.peek()).len(), 2);
    assert!(state.peek().viewer.is_none());
    state.write().core.account_epoch += 1;
    assert!(
        !poll_cooldown(state),
        "old account cannot notify its successor"
    );
}

#[tokio::test]
async fn isolated_image_owned_close_reopen_requires_fresh_ip_without_refetch_and_missed_start_cooldown_wakes()
 {
    static EVENTS: std::sync::Once = std::sync::Once::new();
    EVENTS.call_once(|| {
        dioxus::html::set_event_converter(Box::new(dioxus::html::SerializedHtmlEventConverter))
    });
    let h = pages::tests::setup(true).await;
    let mut old = h.h.state;
    let shell = std::mem::replace(
        &mut *old.write(),
        ShellState::fixture(crate::media::MediaRegistry::default()),
    );
    let (mut dom, mut state, mut attributes) = mount_shell(shell);
    let pages_io = Rc::new(pages::test_io::Io::default());
    let io = Rc::new(Io::default());
    dom.in_scope(ScopeId::ROOT, || {
        provide_context(pages_io.clone());
        provide_context(io.clone());
    });
    let reply = pages_io.expect(1);
    let entry = pages::EntrySource::capture(&state.peek()).unwrap();
    dom.in_scope(ScopeId::ROOT, || {
        pages::change_sort(
            state,
            entry,
            super_platinum_core::slack::api::file_pages::Sort::Newest,
        )
    });
    pump(&mut dom, &mut attributes, || {
        pages_io.pages.borrow().len() == 1
    })
    .await;
    reply.send(Ok(page())).unwrap();
    pump(&mut dom, &mut attributes, || {
        pages::status(&state.peek()).is_some_and(|v| !v.4)
    })
    .await;
    let query = state.peek().search_query.clone();
    let reply = io.expect(false);
    let ip = io.expect(true);
    let bytes = io.expect_bytes();
    click(&dom, mounted_element(&dom, "id", "file-image-preview-0"));
    pump(&mut dom, &mut attributes, || io.calls.borrow().len() == 1).await;
    reply.send(Ok(metadata())).unwrap();
    pump(&mut dom, &mut attributes, || io.calls.borrow().len() == 2).await;
    ip.send(Ok(json!({"ok":true,"ip_allowed":true}))).unwrap();
    pump(&mut dom, &mut attributes, || {
        io.byte_urls.borrow().len() == 1
    })
    .await;
    bytes
        .send(Ok(include_bytes!(
            "../../../../../../assets/icons/icon-512.png"
        )
        .to_vec()))
        .unwrap();
    pump(&mut dom, &mut attributes, || {
        matches!(preview::display(&state.peek(), 0), Some(Display::Image(_)))
    })
    .await;
    let Some(Display::Image(original)) = preview::display(&state.peek(), 0) else {
        panic!()
    };
    // BEGIN local zoom original lifetime.
    let original_zoom_key = preview::dom_key(&state.peek(), 0).unwrap();
    let _zoom_root = mounted_element(&dom, "id", &original_zoom_key);
    // END local zoom original lifetime.
    let stale_close = preview::Source::capture(&state.peek(), 0).unwrap();
    let wrong_occurrence = preview::Source::capture(&state.peek(), 1).unwrap();
    preview::close(state, wrong_occurrence);
    assert!(
        matches!(preview::display(&state.peek(),0),Some(Display::Image(ref id))if id==&original),
        "a fresh peer control cannot close or reseal another occurrence's image"
    );
    record(&mut attributes, dom.render_immediate_to_vec().edits);
    click(&dom, mounted_element(&dom, "id", "file-image-close-0"));
    record(&mut attributes, dom.render_immediate_to_vec().edits);
    assert!(preview::display(&state.peek(), 0).is_none());
    assert!(state.peek().media.is_ready(&original));

    // Only fresh IP is planned: an unexpected metadata/byte request panics in
    // mandatory scoped IO. A prior success never paints through a new denial.
    let ip = io.expect(true);
    click(&dom, mounted_element(&dom, "id", "file-image-preview-0"));
    pump(&mut dom, &mut attributes, || io.calls.borrow().len() == 3).await;
    assert!(matches!(
        preview::display(&state.peek(), 0),
        Some(Display::Loading)
    ));
    ip.send(Err(super_platinum_core::slack::Error::RateLimited {
        retry_after_secs: Some(1),
    }))
    .unwrap();
    pump(&mut dom, &mut attributes, || {
        matches!(preview::display(&state.peek(), 0), Some(Display::Failed))
    })
    .await;
    let retry = mounted_element(&dom, "id", "file-image-retry-0");
    assert_eq!(attributes[&(retry, "disabled".into())], "true");
    // Intentionally miss the entire cooling=true interval, as a delayed tick
    // can. The real expired deadline must still notify once without replay.
    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
    assert!(!preview::cooling(&state.peek()));
    assert_eq!(attributes[&(retry, "disabled".into())], "true");
    assert!(poll_cooldown(state));
    record(&mut attributes, dom.render_immediate_to_vec().edits);
    assert_eq!(attributes[&(retry, "disabled".into())], "false");
    assert!(!poll_cooldown(state));
    assert_eq!(io.calls.borrow().len(), 3);
    assert_eq!(io.byte_urls.borrow().len(), 1);
    let ip = io.expect(true);
    click(&dom, mounted_element(&dom, "id", "file-image-retry-0"));
    pump(&mut dom, &mut attributes, || io.calls.borrow().len() == 4).await;
    ip.send(Ok(json!({"ok":true,"ip_allowed":true}))).unwrap();
    pump(&mut dom, &mut attributes, || {
        matches!(preview::display(&state.peek(), 0), Some(Display::Image(_)))
    })
    .await;
    let Some(Display::Image(reused)) = preview::display(&state.peek(), 0) else {
        panic!()
    };
    assert_eq!(reused, original);
    // BEGIN local zoom reopened lifetime.
    let reopened_zoom_key = preview::dom_key(&state.peek(), 0).unwrap();
    assert_ne!(reopened_zoom_key, original_zoom_key);
    let _zoom_root = mounted_element(&dom, "id", &reopened_zoom_key);
    // END local zoom reopened lifetime.
    assert_eq!(io.byte_urls.borrow().len(), 1);
    assert_eq!(state.peek().search_query, query);
    assert_eq!(pages::rows(&state.peek()).len(), 2);
    assert!(state.peek().viewer.is_none());

    click(&dom, mounted_element(&dom, "id", "file-image-close-0"));
    record(&mut attributes, dom.render_immediate_to_vec().edits);
    let reply = io.expect(false);
    let ip = io.expect(true);
    click(&dom, mounted_element(&dom, "id", "file-image-preview-1"));
    pump(&mut dom, &mut attributes, || io.calls.borrow().len() == 5).await;
    preview::close(state, stale_close);
    let wrong_close = preview::Source::capture(&state.peek(), 0).unwrap();
    preview::close(state, wrong_close);
    assert!(
        matches!(preview::display(&state.peek(), 1), Some(Display::Loading)),
        "old or wrong-index Close cannot own a queued peer"
    );
    reply.send(Ok(metadata())).unwrap();
    pump(&mut dom, &mut attributes, || io.calls.borrow().len() == 6).await;
    ip.send(Ok(json!({"ok":true,"ip_allowed":false}))).unwrap();
    pump(&mut dom, &mut attributes, || {
        matches!(
            preview::display(&state.peek(), 1),
            Some(Display::Unavailable)
        )
    })
    .await;
    assert_eq!(io.byte_urls.borrow().len(), 1);
    click(&dom, mounted_element(&dom, "id", "file-image-close-1"));
    record(&mut attributes, dom.render_immediate_to_vec().edits);
    let reply = io.expect(false);
    click(&dom, mounted_element(&dom, "id", "file-image-preview-0"));
    pump(&mut dom, &mut attributes, || io.calls.borrow().len() == 7).await;
    let stale = preview::Source::capture(&state.peek(), 0).unwrap();
    let team = state.peek().core.active_team.clone().unwrap();
    crate::file_events::invalidate(&mut state.write(), &team, "F12345678");
    reply.send(Ok(metadata())).unwrap();
    pump(&mut dom, &mut attributes, || io.completed.get() == 7).await;
    preview::open(state, stale);
    record(&mut attributes, dom.render_immediate_to_vec().edits);
    assert!(preview::display(&state.peek(), 0).is_none());
    assert!(preview::display(&state.peek(), 1).is_none());
    assert_eq!(io.calls.borrow().len(), 7);
    assert_eq!(io.byte_urls.borrow().len(), 1);
}
