use super::*;
use crate::{
    bootstrap::test_support::{harness, pending},
    state::{ChannelTab, MainView},
};
use std::cell::Cell;
use super_platinum_core::slack::models::{File, SearchPagination, User, UserProfile};
fn page(ids: &[&str], number: u32, user: Option<&str>) -> FilesPage {
    FilesPage {
        items: ids
            .iter()
            .map(|id| {
                serde_json::from_value::<File>(serde_json::json!({"id":id,"title":id,"user":user}))
                    .unwrap()
            })
            .collect(),
        pagination: Some(SearchPagination {
            page: Some(number),
            page_count: Some(4),
            total_count: Some(150),
            ..Default::default()
        }),
    }
}
fn setup(mut state: Signal<ShellState>, user: Option<&str>) {
    let mut shell = state.write();
    shell.main_view = MainView::Files;
    shell.overlay = None;
    let epoch = shell.core.account_epoch;
    shell.core.files.ensure_scope(epoch, "T1");
    let generation = shell.core.files.begin(
        FilesQuery {
            text: "report".into(),
            ..Default::default()
        },
        1,
    );
    assert!(
        shell
            .core
            .files
            .apply(generation, page(&["F1", "F_KEEP"], 1, user))
    );
}
fn source(state: Signal<ShellState>) -> Source {
    Source::capture(&state.peek(), None).unwrap()
}
async fn no_names(_: NamesRequest) -> Result<Vec<User>, Error> {
    Ok(Vec::new())
}
#[tokio::test(flavor = "current_thread")]
async fn actual_files_driver_retains_failed_next_page_deletions_and_exact_retry_and_retires_busy_render_stamp()
 {
    let h = harness();
    let mut state = h.state;
    setup(state, Some("UCOLD_RESOURCE"));
    let (release, held) = tokio::sync::oneshot::channel();
    let names_called = Rc::new(Cell::new(0));
    let observed = names_called.clone();
    let original = source(state);
    let mut work = Box::pin(original.run_with(
        state,
        FilesQuery {
            text: " report ".into(),
            ..Default::default()
        },
        2,
        move |request| async move {
            assert_eq!(request.page, 2);
            assert_eq!(request.query.text, "report");
            assert_eq!(request.query.sort, FileSort::Relevant);
            assert_eq!(request.transport.d_cookie(), "synthetic");
            held.await.unwrap();
            Err(Error::Offline("synthetic offline".into()))
        },
        move |_| {
            observed.set(observed.get() + 1);
            async { Ok(Vec::new()) }
        },
    ));
    pending(work.as_mut()).await;
    {
        let shell = state.peek();
        assert!(shell.core.files.loading);
        assert_eq!(shell.core.files.items.len(), 2);
        assert_eq!(
            (
                shell.core.files.page,
                shell.core.files.page_count,
                shell.core.files.total
            ),
            (1, 4, 150)
        );
        assert_eq!(shell.core.files.requested_page, Some(2));
    }
    release.send(()).unwrap();
    work.await;
    assert_eq!(
        names_called.get(),
        0,
        "primary failure starts no optional name lookup"
    );
    assert_eq!(state.peek().core.files.resume_page(), 2);
    assert!(state.peek().core.files.error.is_some());
    state.write().core.files.remove("F1");
    let retry = source(state);
    let target = retry.resume_page();
    assert_eq!(target, 2);
    let (release, held) = tokio::sync::oneshot::channel();
    let query = retry.query.clone();
    let mut work = Box::pin(retry.run_with(
        state,
        query,
        target,
        move |request| async move {
            assert_eq!(request.page, 2);
            held.await.unwrap();
            Ok(page(&["F1", "F2", "F2"], 2, None))
        },
        no_names,
    ));
    pending(work.as_mut()).await;
    let busy_render = source(state);
    let generation = state.peek().core.files.generation;
    release.send(()).unwrap();
    work.await;
    let shell = state.peek();
    assert_eq!(shell.core.files.generation, generation);
    assert!(
        !busy_render.current(&shell),
        "success changes source stage even without changing generation"
    );
    assert!(!shell.core.files.loading);
    assert_eq!(shell.core.files.page, 2);
    assert_eq!(shell.core.files.total, 149);
    assert_eq!(
        shell
            .core
            .files
            .items
            .iter()
            .map(|f| f.id.as_deref().unwrap())
            .collect::<Vec<_>>(),
        ["F2"]
    );
}
#[tokio::test(flavor = "current_thread")]
async fn credential_retirement_releases_only_its_private_busy_ticket_and_never_a_successor() {
    for successor in [false, true] {
        let h = harness();
        let mut state = h.state;
        setup(state, None);
        let old = source(state);
        let query = old.query.clone();
        let (release, held) = tokio::sync::oneshot::channel();
        let mut old_work = Box::pin(old.run_with(
            state,
            query,
            2,
            move |_| async move {
                held.await.unwrap();
                Ok(page(&["OLD"], 2, None))
            },
            no_names,
        ));
        pending(old_work.as_mut()).await;
        state
            .write()
            .core
            .session
            .as_mut()
            .unwrap()
            .workspaces
            .get_mut("T1")
            .unwrap()
            .token = "rotated-synthetic".into();
        if successor {
            let next = source(state);
            let query = next.query.clone();
            let (done, held) = tokio::sync::oneshot::channel();
            let mut next_work = Box::pin(next.run_with(
                state,
                query,
                3,
                move |request| async move {
                    assert_eq!(request.session.token, "rotated-synthetic");
                    held.await.unwrap();
                    Ok(page(&["CURRENT"], 3, None))
                },
                no_names,
            ));
            pending(next_work.as_mut()).await;
            let generation = state.peek().core.files.generation;
            release.send(()).unwrap();
            old_work.await;
            assert!(state.peek().core.files.loading);
            assert_eq!(state.peek().core.files.generation, generation);
            assert_eq!(state.peek().core.files.requested_page, Some(3));
            assert_eq!(state.peek().core.files.items[0].id.as_deref(), Some("F1"));
            done.send(()).unwrap();
            next_work.await;
            assert_eq!(
                state.peek().core.files.items[0].id.as_deref(),
                Some("CURRENT")
            );
        } else {
            release.send(()).unwrap();
            old_work.await;
            assert!(!state.peek().core.files.loading);
            assert_eq!(state.peek().core.files.resume_page(), 2);
            assert_eq!(state.peek().core.files.items[0].id.as_deref(), Some("F1"));
        }
    }
}
#[tokio::test(flavor = "current_thread")]
async fn global_files_ignore_incidental_conversation_but_retire_rail_visits_and_canvas_picker_keeps_its_route()
 {
    let h = harness();
    let mut state = h.state;
    setup(state, None);
    let channel = state.peek().core.active_channel.clone().unwrap();
    let global = source(state);
    {
        let mut shell = state.write();
        shell.core.active_channel = Some("INCIDENTAL".into());
        shell.channel_generation += 1;
    }
    assert!(global.current(&state.peek()));
    let query = global.query.clone();
    global
        .run_with(
            state,
            query,
            2,
            |_| async { Ok(page(&["GLOBAL"], 2, None)) },
            no_names,
        )
        .await;
    let retired = source(state);
    {
        let mut shell = state.write();
        shell.main_view = MainView::Home;
        shell.close_thread();
        shell.main_view = MainView::Files;
        shell.close_thread();
    }
    let calls = Rc::new(Cell::new(0));
    let observed = calls.clone();
    let query = retired.query.clone();
    retired
        .run_with(
            state,
            query,
            3,
            move |_| {
                observed.set(observed.get() + 1);
                async { Ok(page(&["WRONG"], 3, None)) }
            },
            no_names,
        )
        .await;
    assert_eq!(calls.get(), 0);
    let visit = crate::bootstrap::reconnect::resource_test_visit(&mut state.write());
    let reconnect = Source::capture_reconnect(&state.peek(), None, visit).unwrap();
    let query = reconnect.query.clone();
    state.write().reconnect_generation += 1;
    let reconnect_calls = Rc::new(Cell::new(0));
    let observed = reconnect_calls.clone();
    reconnect
        .run_with(
            state,
            query,
            3,
            move |_| {
                observed.set(observed.get() + 1);
                async { Ok(page(&["WRONG_RECONNECT"], 3, None)) }
            },
            no_names,
        )
        .await;
    assert_eq!(
        reconnect_calls.get(),
        0,
        "original reconnect visit remains part of admission"
    );
    {
        let mut shell = state.write();
        shell.main_view = MainView::Home;
        shell.core.active_channel = Some(channel.clone());
        shell.channel_tab = ChannelTab::Messages;
    }
    assert!(
        Source::capture(&state.peek(), Some(&channel)).is_none(),
        "ordinary Files tab is not selected"
    );
    let visit = crate::channel_tabs::Visit::capture(&state.peek()).unwrap();
    assert!(visit.native_read_current(&state.peek()));
    let picker = Source::for_route(
        &state.peek(),
        Route::Picker {
            channel: channel.clone(),
            visit: visit.clone(),
        },
        None,
    )
    .unwrap();
    let query = FilesQuery {
        channel: Some(channel.clone()),
        kind: super_platinum_core::files::FileKind::Canvases,
        ..Default::default()
    };
    picker
        .run_with(
            state,
            query,
            1,
            |request| async move {
                assert_eq!(
                    request.query.kind,
                    super_platinum_core::files::FileKind::Canvases
                );
                Ok(page(&["FCANVAS"], 1, None))
            },
            no_names,
        )
        .await;
    assert_eq!(
        state.peek().core.channel_files.items[0].id.as_deref(),
        Some("FCANVAS")
    );
    assert_eq!(state.peek().channel_tab, ChannelTab::Messages);
    state.write().channel_tab = ChannelTab::Files;
    let channel_source = Source::capture(&state.peek(), Some(&channel)).unwrap();
    let query = channel_source.query.clone();
    channel_source
        .run_with(
            state,
            query,
            2,
            |request| async move {
                assert!(request.query.channel.is_some());
                Ok(page(&["F_CHANNEL"], 2, None))
            },
            no_names,
        )
        .await;
    assert_eq!(
        state.peek().core.channel_files.items[0].id.as_deref(),
        Some("F_CHANNEL")
    );
    assert_eq!(
        state.peek().core.files.items[0].id.as_deref(),
        Some("GLOBAL")
    );
    state
        .write()
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .rt_generation += 1;
    assert!(Source::for_route(&state.peek(), Route::Picker { channel, visit }, None).is_none());
    crate::channel_tabs::fixture::install(&mut state.write());
    let closed = crate::channel_tabs::Visit::capture(&state.peek()).unwrap();
    assert!(!closed.native_read_current(&state.peek()));
}
fn user(name: &str) -> User {
    User {
        id: "UCOLD_RESOURCE".into(),
        profile: Some(UserProfile {
            display_name: Some(name.into()),
            ..Default::default()
        }),
        ..Default::default()
    }
}
#[tokio::test(flavor = "current_thread")]
async fn optional_names_never_hold_primary_busy_and_preserve_observed_updates_or_superseding_pages()
{
    for mode in [0, 1, 2, 3] {
        let h = harness();
        let mut state = h.state;
        setup(state, None);
        let current = source(state);
        let query = current.query.clone();
        let (release, held) = tokio::sync::oneshot::channel();
        let mut work = Box::pin(current.run_with(
            state,
            query,
            1,
            |_| async { Ok(page(&["NAMED"], 1, Some("UCOLD_RESOURCE"))) },
            move |request| async move {
                assert_eq!(request.ids, ["UCOLD_RESOURCE".to_owned()]);
                held.await.unwrap()
            },
        ));
        pending(work.as_mut()).await;
        assert!(!state.peek().core.files.loading);
        assert!(state.peek().core.files.loaded);
        if mode == 1 {
            let mut live = user("Realtime name");
            live.is_admin = true;
            state
                .write()
                .core
                .workspaces
                .get_mut("T1")
                .unwrap()
                .apply_user_update(live);
        }
        if mode == 2 {
            let newer = source(state);
            let query = newer.query.clone();
            newer
                .run_with(
                    state,
                    query,
                    2,
                    |_| async { Ok(page(&["NEW_PAGE"], 2, None)) },
                    no_names,
                )
                .await;
        }
        if mode == 3 {
            release
                .send(Err(Error::Offline("synthetic offline".into())))
                .unwrap();
        } else {
            release.send(Ok(vec![user("Lookup name")])).unwrap();
        }
        work.await;
        let shell = state.peek();
        let workspace = &shell.core.workspaces["T1"];
        match mode {
            0 => assert_eq!(
                super_platinum_core::state::display_name(
                    workspace.users.get("UCOLD_RESOURCE"),
                    "UCOLD_RESOURCE"
                ),
                "Lookup name"
            ),
            1 => {
                assert_eq!(
                    super_platinum_core::state::display_name(
                        workspace.users.get("UCOLD_RESOURCE"),
                        "UCOLD_RESOURCE"
                    ),
                    "Realtime name"
                );
                assert!(workspace.users["UCOLD_RESOURCE"].is_admin);
            }
            2 => {
                assert!(!workspace.users.contains_key("UCOLD_RESOURCE"));
                assert_eq!(shell.core.files.items[0].id.as_deref(), Some("NEW_PAGE"));
            }
            _ => assert!(!workspace.users.contains_key("UCOLD_RESOURCE")),
        }
        assert!(!shell.core.files.loading);
        assert!(shell.core.files.error.is_none());
    }
}
