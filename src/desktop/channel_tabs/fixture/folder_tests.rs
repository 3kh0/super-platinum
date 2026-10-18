use super::super::folder;
use super::*;
use std::{cell::Cell, rc::Rc};
fn harness() -> (VirtualDom, Signal<ShellState>) {
    type Slot = Rc<Cell<Option<Signal<ShellState>>>>;
    let slot = Slot::default();
    let mut dom = VirtualDom::new_with_props(
        |slot: Slot| {
            let state = use_signal(|| {
                let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
                install_folder(&mut shell);
                shell
            });
            slot.set(Some(state));
            rsx! {}
        },
        slot.clone(),
    );
    dom.rebuild_in_place();
    (dom, slot.get().unwrap())
}
async fn read(state: Signal<ShellState>) {
    let (owner, folder, serial) = folder::prepare(state).unwrap();
    folder::read(state, owner, folder, serial).await;
}
fn endpoints(cap: &Capability) -> Vec<String> {
    cap.0
        .lock()
        .unwrap()
        .requests
        .iter()
        .map(|r| {
            r.url
                .split('?')
                .next()
                .unwrap()
                .rsplit('/')
                .next()
                .unwrap()
                .into()
        })
        .collect()
}
#[tokio::test(flavor = "current_thread")]
async fn actual_reader_uses_canonical_membership_and_exact_bookmark_request_without_transport() {
    let (_dom, state) = harness();
    let cap = state.peek().channel_tabs.fixture.clone().unwrap();
    read(state).await;
    assert_eq!(
        endpoints(&cap),
        ["conversations.info", "bookmarks.list", "conversations.info"]
    );
    let shell = state.peek();
    let contents = super::super::entry(&shell)
        .unwrap()
        .folder
        .contents
        .as_ref()
        .unwrap();
    assert_eq!(
        contents
            .items()
            .iter()
            .map(|i| i.bookmark.id.as_str())
            .collect::<Vec<_>>(),
        ["Bk2", "Bk3", "Bk4"]
    );
    assert!(shell.core.session.is_none() && shell.core.transport.is_none());
    let r = cap.0.lock().unwrap();
    let RequestBody::Form(form) = &r.requests[1].body else {
        panic!("actual form")
    };
    assert_eq!(
        form.iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect::<Vec<_>>(),
        [
            ("token", "fixture-only"),
            ("channel", CHANNEL),
            ("include_folders", "true"),
            ("include_legacy_workflows", "true")
        ]
    );
}
#[tokio::test(flavor = "current_thread")]
async fn removed_or_rebound_membership_clears_old_contents_without_another_bookmark_read() {
    for rebind in [false, true] {
        let (_dom, state) = harness();
        let cap = state.peek().channel_tabs.fixture.clone().unwrap();
        if rebind {
            cap.0.lock().unwrap().raw["properties"]["tabz"][0]["data"]["folder_bookmark_id"] =
                "Bk9".into();
        } else {
            cap.0.lock().unwrap().raw["properties"]["tabz"]
                .as_array_mut()
                .unwrap()
                .remove(0);
        }
        read(state).await;
        assert_eq!(endpoints(&cap), ["conversations.info"]);
        assert!(
            super::super::entry(&state.peek())
                .unwrap()
                .folder
                .contents
                .is_none()
        );
        assert!(folder::selected(&state.peek()).is_none());
        assert!(
            selection::folder_selected(&state.peek()),
            "stale tab remains explicit unavailable, not whole hierarchy"
        );
    }
}
#[tokio::test(flavor = "current_thread")]
async fn late_folder_page_cannot_adopt_after_account_navigation_or_history_replacement() {
    for variant in 0..3 {
        let (_dom, mut state) = harness();
        let cap = state.peek().channel_tabs.fixture.clone().unwrap();
        let entered = Arc::new(tokio::sync::Notify::new());
        let release = Arc::new(tokio::sync::Notify::new());
        cap.0.lock().unwrap().manager_pause = Some((entered.clone(), release.clone()));
        let prior = super::super::entry(&state.peek())
            .unwrap()
            .folder
            .contents
            .clone();
        let job = async {
            tokio::join!(read(state), async {
                entered.notified().await;
                match variant {
                    0 => state.write().core.account_epoch += 1,
                    1 => state.write().channel_generation += 1,
                    _ => {
                        let mut shell = state.write();
                        selection::ordinary_open(&mut shell);
                    }
                }
                cap.0.lock().unwrap().folder_bookmarks = json!([]);
                release.notify_one();
            });
        };
        tokio::time::timeout(std::time::Duration::from_secs(2), job)
            .await
            .unwrap();
        assert_eq!(
            super::super::entry(&state.peek()).unwrap().folder.contents,
            prior
        );
        assert_eq!(endpoints(&cap), ["conversations.info", "bookmarks.list"]);
    }
}
#[test]
fn folder_and_whole_hierarchy_have_distinct_history_and_old_steps_cannot_rebind() {
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    install_folder(&mut shell);
    let selected = selection::current(&shell).unwrap();
    let back = selection::step(&shell, false).unwrap();
    assert_eq!(
        back.route,
        selection::Route::Resource(crate::state::ChannelTab::Messages)
    );
    selection::record(&mut shell, back.route.clone(), Some(&back));
    shell.channel_tab = back.route.tab();
    let forward = selection::step(&shell, true).unwrap();
    assert_eq!(forward.route, selected);
    selection::record(&mut shell, forward.route.clone(), Some(&forward));
    shell.channel_tab = forward.route.tab();
    assert!(selection::folder_selected(&shell));
    selection::record(
        &mut shell,
        selection::Route::Resource(crate::state::ChannelTab::Bookmarks),
        None,
    );
    assert!(!selection::folder_selected(&shell));
    assert!(folder::selected(&shell).is_none());
    assert!(!selection::accepts(&shell, &back));
    selection::ordinary_open(&mut shell);
    assert_eq!(shell.channel_tab, crate::state::ChannelTab::Messages);
}

#[test]
fn actual_folder_panel_mount_uses_shared_bookmark_controls_and_only_direct_children() {
    use dioxus::core::{Mutation, Mutations};
    let mut dom = VirtualDom::new(|| {
        let state = use_signal(|| {
            let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
            install_folder(&mut shell);
            shell
        });
        rsx! {folder::Panel{state}}
    });
    let mut mutations = Mutations::default();
    dom.rebuild(&mut mutations);
    let text = mutations
        .edits
        .iter()
        .filter_map(|edit| match edit {
            Mutation::CreateTextNode { value, .. } | Mutation::SetText { value, .. } => {
                Some(value.to_string())
            }
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" ");
    assert!(
        text.contains("Launch guide")
            && text.contains("Project notes")
            && text.contains("Retained future item")
    );
    assert!(!text.contains("Top-level bookmark"));
}

#[tokio::test(flavor = "current_thread")]
async fn folder_read_rate_limit_keeps_manual_retry_fence_without_mutation() {
    let (dom, state) = harness();
    let cap = state.peek().channel_tabs.fixture.clone().unwrap();
    cap.0.lock().unwrap().manager_rate_limited = true;
    let mut future = Box::pin(read(state));
    std::future::poll_fn(|cx| dom.in_runtime(|| std::future::Future::poll(future.as_mut(), cx)))
        .await;
    assert!(super::super::entry(&state.peek()).unwrap().folder.cooling);
    assert!(folder::prepare(state).is_none());
    assert_eq!(endpoints(&cap), ["conversations.info", "bookmarks.list"]);
    assert!(
        super::super::entry(&state.peek())
            .unwrap()
            .folder
            .contents
            .is_some()
    );
}

#[tokio::test(flavor = "current_thread")]
async fn rebinding_during_bookmark_read_is_checked_again_before_projection() {
    let (_dom, state) = harness();
    let cap = state.peek().channel_tabs.fixture.clone().unwrap();
    let entered = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    cap.0.lock().unwrap().manager_pause = Some((entered.clone(), release.clone()));
    let job = async {
        tokio::join!(read(state), async {
            entered.notified().await;
            cap.0.lock().unwrap().raw["properties"]["tabz"][0]["data"]["folder_bookmark_id"] =
                "Bk9".into();
            release.notify_one();
        });
    };
    tokio::time::timeout(std::time::Duration::from_secs(2), job)
        .await
        .unwrap();
    assert_eq!(
        endpoints(&cap),
        ["conversations.info", "bookmarks.list", "conversations.info"]
    );
    assert!(
        super::super::entry(&state.peek())
            .unwrap()
            .folder
            .contents
            .is_none()
    );
    assert!(folder::selected(&state.peek()).is_none());
}

#[test]
fn folder_panel_bounds_mounted_rows_without_truncating_accepted_inventory() {
    use dioxus::core::Mutation;
    let mut dom = VirtualDom::new(|| {
        let state = use_signal(|| {
            let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
            install_folder(&mut shell);
            let base = super::super::inventory(&shell).unwrap();
            let f = folder::selected(&shell).unwrap();
            let items=(0..160).map(|n|json!({"id":format!("Bk{}",n+1000),"type":"link","parent_id":"Bk1","title":format!("Bounded item {n}"),"link":"https://example.com"})).collect::<Vec<_>>();
            let contents = super_platinum_core::channel_tabs::folder::Contents::parse(
                &base,
                &f,
                &json!({"ok":true,"bookmarks":items}),
            )
            .unwrap();
            assert_eq!(contents.items().len(), 160);
            let key = key(&shell).unwrap();
            shell
                .channel_tabs
                .entries
                .get_mut(&key)
                .unwrap()
                .folder
                .contents = Some(contents);
            shell
        });
        rsx! {folder::Panel{state}}
    });
    let changes = dom.rebuild_to_vec();
    assert_eq!(changes.edits.iter().filter(|m|matches!(m,Mutation::CreateTextNode{value,..} if value.starts_with("Bounded item "))).count(),64);
}

fn switch_folder(mut state: Signal<ShellState>, id: &str) {
    let mut shell = state.write();
    let base = super::super::inventory(&shell).unwrap();
    let folder = super_platinum_core::channel_tabs::folder::Folder::capture(&base, id).unwrap();
    selection::record(&mut shell, selection::Route::Folder(folder), None);
    shell.channel_tab = crate::state::ChannelTab::Bookmarks;
}
#[tokio::test(flavor = "current_thread")]
async fn switching_folder_supersedes_pending_read_and_late_a_cannot_clear_b_busy_or_adopt() {
    let (_dom, state) = harness();
    let cap = state.peek().channel_tabs.fixture.clone().unwrap();
    let entered = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    cap.0.lock().unwrap().manager_pause = Some((entered.clone(), release.clone()));
    let first = folder::prepare(state).unwrap();
    assert!(
        folder::prepare(state).is_none(),
        "same owner/route coalesces"
    );
    let second = std::cell::RefCell::new(None);
    let job = async {
        tokio::join!(folder::read(state, first.0, first.1, first.2), async {
            entered.notified().await;
            switch_folder(state, "Ct00000002");
            let b = folder::prepare(state).expect("new route supersedes pending A");
            assert_ne!(b.2, first.2);
            assert!(folder::prepare(state).is_none(), "same B coalesces");
            *second.borrow_mut() = Some(b);
            cap.0.lock().unwrap().manager_pause = None;
            release.notify_one();
        });
    };
    tokio::time::timeout(std::time::Duration::from_secs(2), job)
        .await
        .unwrap();
    assert!(
        super::super::entry(&state.peek()).unwrap().folder.loading,
        "late A cannot clear B busy"
    );
    assert_eq!(
        super::super::entry(&state.peek())
            .unwrap()
            .folder
            .contents
            .as_ref()
            .unwrap()
            .folder()
            .tab(),
        "Ct00000001",
        "late A does not replace retained accepted inventory"
    );
    let b = second.into_inner().unwrap();
    folder::read(state, b.0, b.1, b.2).await;
    let shell = state.peek();
    let e = super::super::entry(&shell).unwrap();
    assert!(!e.folder.loading);
    let contents = e.folder.contents.as_ref().unwrap();
    assert_eq!(contents.folder().tab(), "Ct00000002");
    assert_eq!(
        contents
            .items()
            .iter()
            .map(|i| i.bookmark.id.as_str())
            .collect::<Vec<_>>(),
        ["Bk5"]
    );
}
#[test]
fn same_folder_new_channel_visit_supersedes_retired_owner_and_deadlines_never_overflow() {
    let (_dom, mut state) = harness();
    let a = folder::prepare(state).unwrap();
    state.write().channel_generation += 1;
    let b = folder::prepare(state).expect("same folder but new visit is a new read");
    assert_ne!(a.2, b.2);
    let now = Instant::now();
    assert_eq!(folder::deadline(now, Some(0)), Some(now));
    assert_eq!(
        folder::deadline(now, None),
        now.checked_add(std::time::Duration::from_secs(30))
    );
    assert!(folder::deadline(now, Some(u64::MAX)).is_none());
    assert!(folder::deadline(now, Some(366 * 24 * 60 * 60 + 1)).is_none());
}
#[tokio::test(flavor = "current_thread")]
async fn file_read_errors_retain_contents_with_feedback_and_rate_limit_blocks_all_folder_requests()
{
    for fault in [None, Some(u64::MAX), Some(1)] {
        let (dom, state) = harness();
        let cap = state.peek().channel_tabs.fixture.clone().unwrap();
        read(state).await;
        let (owner, selected, serial) = folder::prepare(state).unwrap();
        folder::read(state, owner.clone(), selected.clone(), serial).await;
        let prior = super::super::entry(&state.peek())
            .unwrap()
            .folder
            .contents
            .clone();
        cap.0.lock().unwrap().folder_file_error = Some(fault);
        let opening = folder::prepare_file(state, &owner, &selected, serial).unwrap();
        assert!(
            folder::prepare_file(state, &owner, &selected, serial).is_none(),
            "duplicate open coalesces"
        );
        let mut future = Box::pin(folder::read_file(
            state,
            owner.clone(),
            selected.clone(),
            serial,
            opening,
            "F00000001".into(),
        ));
        std::future::poll_fn(|cx| {
            dom.in_runtime(|| std::future::Future::poll(future.as_mut(), cx))
        })
        .await;
        assert_eq!(
            super::super::entry(&state.peek()).unwrap().folder.contents,
            prior
        );
        assert!(
            !super::super::entry(&state.peek())
                .unwrap()
                .folder
                .notice
                .is_empty()
        );
        assert_eq!(
            super::super::entry(&state.peek()).unwrap().folder.cooling,
            fault.is_some()
        );
        if fault.is_some() {
            assert!(folder::prepare(state).is_none());
            assert!(folder::prepare_file(state, &owner, &selected, serial).is_none());
        } else {
            assert!(
                folder::prepare_file(state, &owner, &selected, serial).is_some(),
                "explicit retry after non-rate error"
            );
        }
        assert_eq!(
            endpoints(&cap)
                .iter()
                .filter(|e| e.as_str() == "files.info")
                .count(),
            1
        );
        assert!(state.peek().core.session.is_none() && state.peek().core.transport.is_none());
    }
}
#[tokio::test(flavor = "current_thread")]
async fn late_file_rate_error_cannot_clobber_replacement_route_or_account_presentation() {
    for account in [false, true] {
        let (_dom, mut state) = harness();
        let cap = state.peek().channel_tabs.fixture.clone().unwrap();
        let (owner, selected, serial) = folder::prepare(state).unwrap();
        folder::read(state, owner.clone(), selected.clone(), serial).await;
        let entered = Arc::new(tokio::sync::Notify::new());
        let release = Arc::new(tokio::sync::Notify::new());
        cap.0.lock().unwrap().folder_file_pause = Some((entered.clone(), release.clone()));
        cap.0.lock().unwrap().folder_file_error = Some(Some(u64::MAX));
        let opening = folder::prepare_file(state, &owner, &selected, serial).unwrap();
        let job = async {
            tokio::join!(
                folder::read_file(state, owner, selected, serial, opening, "F00000001".into()),
                async {
                    entered.notified().await;
                    if account {
                        state.write().core.account_epoch += 1;
                    } else {
                        switch_folder(state, "Ct00000002");
                        read(state).await;
                    }
                    let mut shell = state.write();
                    let k = key(&shell).unwrap();
                    shell
                        .channel_tabs
                        .entries
                        .get_mut(&k)
                        .unwrap()
                        .folder
                        .notice = "Newer presentation".into();
                    drop(shell);
                    release.notify_one();
                }
            );
        };
        tokio::time::timeout(std::time::Duration::from_secs(2), job)
            .await
            .unwrap();
        let shell = state.peek();
        let e = super::super::entry(&shell).unwrap();
        assert!(!e.folder.cooling);
        assert_eq!(e.folder.notice, "Newer presentation");
        if !account {
            assert_eq!(
                e.folder.contents.as_ref().unwrap().folder().tab(),
                "Ct00000002"
            );
        }
    }
}

#[tokio::test(flavor = "current_thread")]
async fn overlay_retirement_releases_only_its_open_token_and_allows_new_explicit_open() {
    for pre_dispatch in [false, true] {
        let (_dom, mut state) = harness();
        let cap = state.peek().channel_tabs.fixture.clone().unwrap();
        let (owner, selected, serial) = folder::prepare(state).unwrap();
        folder::read(state, owner.clone(), selected.clone(), serial).await;
        let opening = folder::prepare_file(state, &owner, &selected, serial).unwrap();
        let prior = super::super::entry(&state.peek())
            .unwrap()
            .folder
            .contents
            .clone();
        if pre_dispatch {
            state.write().overlay = Some(crate::model::Overlay::Palette);
            folder::read_file(
                state,
                owner.clone(),
                selected.clone(),
                serial,
                opening,
                "F00000001".into(),
            )
            .await;
            assert!(!endpoints(&cap).iter().any(|e| e == "files.info"));
        } else {
            let entered = Arc::new(tokio::sync::Notify::new());
            let release = Arc::new(tokio::sync::Notify::new());
            cap.0.lock().unwrap().folder_file_pause = Some((entered.clone(), release.clone()));
            cap.0.lock().unwrap().folder_file_error = Some(Some(u64::MAX));
            let job = async {
                tokio::join!(
                    folder::read_file(
                        state,
                        owner.clone(),
                        selected.clone(),
                        serial,
                        opening,
                        "F00000001".into()
                    ),
                    async {
                        entered.notified().await;
                        state.write().overlay = Some(crate::model::Overlay::Palette);
                        release.notify_one();
                    }
                );
            };
            tokio::time::timeout(std::time::Duration::from_secs(2), job)
                .await
                .unwrap();
            assert_eq!(
                endpoints(&cap)
                    .iter()
                    .filter(|e| e.as_str() == "files.info")
                    .count(),
                1
            );
        }
        assert_eq!(
            super::super::entry(&state.peek()).unwrap().folder.contents,
            prior
        );
        assert!(
            super::super::entry(&state.peek())
                .unwrap()
                .folder
                .notice
                .is_empty()
        );
        assert!(!super::super::entry(&state.peek()).unwrap().folder.cooling);
        state.write().overlay = None;
        let newer = folder::prepare_file(state, &owner, &selected, serial)
            .expect("closing overlay permits a new explicit open");
        assert_ne!(newer, opening);
        // A retained old task reaching its initial guard must not clear newer.
        folder::read_file(
            state,
            owner.clone(),
            selected.clone(),
            serial,
            opening,
            "F00000001".into(),
        )
        .await;
        assert!(
            folder::prepare_file(state, &owner, &selected, serial).is_none(),
            "new opening remains owned"
        );
    }
}
