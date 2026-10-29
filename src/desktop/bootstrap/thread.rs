//! Scoped thread loading and navigation to an exact saved/pinned reply.
use crate::state::ShellState;
use dioxus::prelude::*;
use std::sync::Arc;
use super_platinum_core::slack::{Transport, api};

struct Visit {
    scope: String,
    transport: Arc<Transport>,
    reconnect: Option<super::reconnect::Visit>,
    request: u64,
}
impl Visit {
    fn current(&self, shell: &ShellState) -> bool {
        started_current(shell, &self.scope, self.request, self.reconnect.as_ref())
            && shell
                .core
                .transport
                .as_ref()
                .is_some_and(|transport| Arc::ptr_eq(transport, &self.transport))
    }
}

fn started_current(
    shell: &ShellState,
    scope: &str,
    request: u64,
    reconnect: Option<&super::reconnect::Visit>,
) -> bool {
    shell.thread_scope() == scope
        && shell.reconnect_generation == request
        // A workflow can hide the pane without changing its thread scope.
        // Reconnect reads must remain visible through every existing fence.
        && reconnect.is_none_or(|visit| visit.current(shell) && shell.thread_visible())
}

#[cfg(test)]
fn reply_args(channel: String, root: String, target: Option<&str>) -> api::RepliesArgs {
    super_platinum_core::thread_history::ThreadHistory::initial(
        target,
        super_platinum_core::state::now_secs(),
    )
    .request(channel, root)
}

pub async fn open_thread(state: Signal<ShellState>, channel: String, root: String) {
    open_thread_at(state, channel, root, None).await;
}

pub async fn open_thread_at(
    mut state: Signal<ShellState>,
    channel: String,
    root: String,
    target: Option<String>,
) {
    {
        let mut shell = state.write();
        if shell.core.active_channel.as_ref() != Some(&channel) {
            let Some(index) = shell
                .channels
                .iter()
                .position(|candidate| candidate.id == channel)
            else {
                shell.show_toast("This conversation is unavailable.");
                return;
            };
            let open = if shell.main_view == crate::state::MainView::Threads {
                crate::state::ChannelOpen::InSurface
            } else {
                crate::state::ChannelOpen::Global
            };
            shell.select_channel(index, open);
        }
        shell.begin_thread(root.clone(), target.clone());
        shell.remember_surface();
    }
    let scope = state.peek().thread_scope();
    load_started_thread(state, channel, root, target, scope).await;
}

/// Continue an already-admitted thread open without beginning a second visit.
/// Check before credentials or fixture mutation, as well as after network awaits.
pub(crate) async fn load_started_thread(
    state: Signal<ShellState>,
    channel: String,
    root: String,
    target: Option<String>,
    scope: String,
) {
    load_started_thread_owned(state, channel, root, target, scope, None).await;
}

pub(super) async fn load_started_thread_owned(
    state: Signal<ShellState>,
    channel: String,
    root: String,
    target: Option<String>,
    scope: String,
    reconnect: Option<super::reconnect::Visit>,
) {
    load_started_thread_with(
        state,
        channel,
        root,
        target,
        scope,
        reconnect,
        |transport, client, session, args| async move {
            api::fetch_replies(&transport, &client, &session, args).await
        },
    )
    .await;
}

// Only the request factory varies; admission, merge and settlement stay shared.
async fn load_started_thread_with<F, Fut>(
    mut state: Signal<ShellState>,
    channel: String,
    root: String,
    target: Option<String>,
    scope: String,
    reconnect: Option<super::reconnect::Visit>,
    fetch: F,
) where
    F: FnOnce(
        Arc<Transport>,
        super_platinum_core::slack::SlackClient,
        super_platinum_core::config::WorkspaceSession,
        api::RepliesArgs,
    ) -> Fut,
    Fut: std::future::Future<
            Output = Result<
                super_platinum_core::slack::models::HistoryPage,
                super_platinum_core::slack::Error,
            >,
        >,
{
    let request = state.peek().reconnect_generation;
    if !started_current(&state.peek(), &scope, request, reconnect.as_ref()) {
        return;
    }
    if super::reconnect::fixture::enabled(&state.peek()) {
        let id =
            super::reconnect::fixture::thread_read(state, target.as_deref(), reconnect.is_some())
                .await;
        let mut shell = state.write();
        let current = started_current(&shell, &scope, request, reconnect.as_ref());
        super::reconnect::fixture::finish_thread(id, &mut shell, &channel, &root, current);
        return;
    }
    if std::env::var_os("SUPER_PLATINUM_FIXTURE").is_some() {
        state.write().thread_loading = false;
        return;
    }
    let Some((transport, client, sessions)) = super::credentials(&state) else {
        return;
    };
    let Some(team) = state.peek().core.active_team.clone() else {
        return;
    };
    let Some(session) = sessions.into_iter().find(|session| session.team_id == team) else {
        return;
    };
    let visit = Visit {
        scope,
        transport,
        reconnect,
        request,
    };
    let range = super_platinum_core::thread_history::ThreadHistory::initial(
        target.as_deref(),
        super_platinum_core::state::now_secs(),
    );
    let thread_key = (team.clone(), channel.clone(), root.clone());
    dioxus::prelude::spawn(crate::thread_subscription::refresh(
        state,
        thread_key.clone(),
    ));
    let read_revision = state.peek().core.thread_read_revision(&thread_key);
    let cursor_owner = crate::realtime::activity_counts::CursorOwner::capture(
        &state.peek().core,
        &visit.transport,
        &session,
    );
    let file_read = state.write().core.begin_file_display_read(&team);
    let result = fetch(
        visit.transport.clone(),
        client,
        session,
        range.request(channel.clone(), root.clone()),
    )
    .await;
    if !visit.current(&state.peek()) {
        return;
    }
    let result = result.and_then(|page| {
        state
            .read()
            .core
            .finish_file_display_read(&team, file_read)
            .map(|files| (page, files))
            .map_err(|reason| super_platinum_core::slack::Error::Api(reason.into()))
    });
    match result {
        Ok((page, files)) => {
            let last_read = page
                .messages
                .iter()
                .find(|m| m.ts.as_ref() == Some(&root))
                .and_then(|m| m.extra.get("last_read"))
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned);
            let found = target.as_ref().is_none_or(|target| {
                page.messages
                    .iter()
                    .any(|message| message.ts.as_ref() == Some(target))
            });
            {
                let mut shell = state.write();
                let upload_ids = crate::messaging::upload_recovery::incoming(
                    &shell,
                    &team,
                    &channel,
                    &page.messages,
                );
                shell.thread_history.start(&range, &root, &page);
                let messages = shell
                    .core
                    .threads
                    .entry((team.clone(), channel.clone(), root.clone()))
                    .or_default();
                for message in page.messages {
                    messages.upsert(files.message(message));
                }
                messages.loaded = true;
                messages.recount_thread_unreads(&root);
                if let Some(cursor) = last_read
                    && cursor_owner
                        .as_ref()
                        .is_some_and(|owner| owner.current(&shell.core))
                {
                    crate::realtime::activity_counts::confirm_thread_read(
                        &mut shell.core,
                        &thread_key,
                        &cursor,
                        read_revision,
                    );
                }
                crate::thread_unread::remember_anchor(&mut shell);
                finish_read_status(&mut shell, found);
                crate::messaging::upload_recovery::reconcile(
                    &mut shell,
                    &team,
                    &channel,
                    Some(&upload_ids),
                );
                shell.refresh_from_core();
            }
            super::session::hydrate_current_surface(state).await;
            if !visit.current(&state.peek()) {
                return;
            }
            // Opening an older reply does not mean the newer replies were read.
            if target.is_none() {
                super::history::mark_thread_read(state, channel, root).await;
                if !visit.current(&state.peek()) {
                    return;
                }
            }
            state.write().refresh_from_core();
        }
        Err(error) => {
            let mut shell = state.write();
            shell.thread_loading = false;
            shell.thread_error = Some("Replies could not load. Please try again.".into());
            shell.report_failure(&error, "Replies cannot load while offline", || {
                "Could not load replies.".into()
            });
        }
    }
}

// Called only inside the accepted, current replies-success commit above.
fn finish_read_status(shell: &mut ShellState, found: bool) {
    shell.thread_loading = false;
    shell.thread_error = None;
    if !found {
        shell.thread_error = Some("This reply is unavailable. It may have been deleted.".into());
        shell.show_toast("This reply is unavailable. It may have been deleted.");
    }
}

pub async fn scroll_thread(state: Signal<ShellState>, target: Option<String>) -> bool {
    let args = {
        let shell = state.peek();
        serde_json::json!({
            "rootId":"thread-body", "scope":shell.thread_scope(),
            "inputGeneration":shell.thread_window.input_generation,
            "selector":target.map(|ts| format!("[data-message-id={}]", serde_json::to_string(&ts).unwrap())).unwrap_or_default(),
            "latestId":shell.thread_messages.last().map(|message| &message.id),
        })
    };
    dioxus::document::eval(&format!(
        "return await ({})({args});",
        include_str!("../timeline_navigation.js")
    ))
    .await
    .ok()
    .and_then(|value| value.as_bool())
    .unwrap_or(false)
}

pub async fn load_thread_page(
    mut state: Signal<ShellState>,
    direction: super_platinum_core::thread_history::Direction,
) {
    let Some((transport, client, sessions)) = super::credentials(&state) else {
        return;
    };
    let (team, channel, root) = {
        let shell = state.peek();
        let (Some(team), Some(channel), Some(root)) = (
            shell.core.active_team.clone(),
            shell.core.active_channel.clone(),
            shell.thread_root.clone(),
        ) else {
            return;
        };
        (team, channel, root)
    };
    let Some(session) = sessions.into_iter().find(|session| session.team_id == team) else {
        return;
    };
    let visit = Visit {
        scope: state.peek().thread_scope(),
        transport,
        reconnect: None,
        request: state.peek().reconnect_generation,
    };
    let request = {
        let mut shell = state.write();
        if shell.thread_loading {
            return;
        }
        let Some(request) = shell.thread_history.begin(direction) else {
            return;
        };
        shell.thread_at_bottom = false;
        request
    };
    dioxus::document::eval("window.superPlatinumThreadAnchor?.hold();");
    let file_read = state.write().core.begin_file_display_read(&team);
    let result = api::fetch_replies(
        &visit.transport,
        &client,
        &session,
        request.request(channel.clone(), root.clone()),
    )
    .await;
    if !visit.current(&state.peek()) {
        return;
    }
    let result = result.and_then(|page| {
        state
            .read()
            .core
            .finish_file_display_read(&team, file_read)
            .map(|files| (page, files))
            .map_err(|reason| super_platinum_core::slack::Error::Api(reason.into()))
    });
    match result {
        Ok((page, files)) => {
            {
                let mut shell = state.write();
                let upload_ids = crate::messaging::upload_recovery::incoming(
                    &shell,
                    &team,
                    &channel,
                    &page.messages,
                );
                shell
                    .thread_history
                    .apply(direction, &request, &root, &page);
                let bag = shell
                    .core
                    .threads
                    .entry((team.clone(), channel.clone(), root.clone()))
                    .or_default();
                for message in page.messages {
                    bag.upsert(files.message(message));
                }
                bag.recount_thread_unreads(&root);
                crate::messaging::upload_recovery::reconcile(
                    &mut shell,
                    &team,
                    &channel,
                    Some(&upload_ids),
                );
                shell.refresh_from_core();
            }
            super::session::hydrate_current_surface(state).await;
        }
        Err(error) => {
            let mut shell = state.write();
            shell.thread_history.fail(direction);
            shell.report_failure(&error, "Replies cannot load while offline", || {
                "Could not load more replies. Please try again.".into()
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn late_thread_work_cannot_cross_reopen_account_or_transport_changes() {
        let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
        let transport = Arc::new(Transport::new("synthetic-cookie").unwrap());
        shell.core.transport = Some(transport.clone());
        shell.begin_thread("100.000001".into(), Some("150.000002".into()));
        let visit = Visit {
            scope: shell.thread_scope(),
            transport: transport.clone(),
            reconnect: None,
            request: shell.reconnect_generation,
        };
        assert!(visit.current(&shell));
        shell.reconnect_generation += 1;
        assert!(!visit.current(&shell));
        shell.reconnect_generation -= 1;
        assert!(!shell.thread_at_bottom);
        assert_eq!(shell.thread_target.as_deref(), Some("150.000002"));
        shell.close_thread();
        assert!(shell.thread_target.is_none());
        assert!(!visit.current(&shell));
        shell.begin_thread("100.000001".into(), None);
        assert!(shell.thread_at_bottom);
        assert!(!visit.current(&shell));
        let current = Visit {
            scope: shell.thread_scope(),
            transport,
            reconnect: None,
            request: shell.reconnect_generation,
        };
        assert!(current.current(&shell));
        shell.core.transport = Some(Arc::new(Transport::new("other-synthetic-cookie").unwrap()));
        assert!(!current.current(&shell));
        shell.core.transport = Some(current.transport.clone());
        shell.core.account_epoch += 1;
        assert!(!current.current(&shell));
    }

    #[test]
    fn accepted_reply_status_clears_retained_failure_without_changing_read_or_draft_state() {
        let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
        let root = "1750000900.000001".to_owned();
        shell.begin_thread(root.clone(), None);
        let key = (
            shell.core.active_team.clone().unwrap(),
            shell.core.active_channel.clone().unwrap(),
            root.clone(),
        );
        let bag = shell.core.threads.entry(key.clone()).or_default();
        bag.upsert(
            serde_json::from_value(
                serde_json::json!({"ts":root,"thread_ts":root,"text":"Retained replies"}),
            )
            .unwrap(),
        );
        bag.last_read = Some(root);
        bag.read_revision = 17;
        crate::thread_unread::remember_anchor(&mut shell);
        shell.core.composer.text = "Keep conversation draft".into();
        shell.core.thread_composer.text = "Keep thread draft".into();
        let before = (
            format!("{:?}", shell.core.threads.get(&key)),
            shell.core.composer.text.clone(),
            shell.core.thread_composer.text.clone(),
            crate::thread_unread::shortcut_initial_cursor(&shell).map(str::to_owned),
        );
        shell.thread_error = Some("Replies could not load. Please try again.".into());
        shell.thread_loading = true;
        shell.show_toast("Existing unrelated toast");
        finish_read_status(&mut shell, true);
        assert!(!shell.thread_loading);
        assert!(shell.thread_error.is_none());
        assert_eq!(
            shell.toast.as_ref().map(|t| t.text.as_str()),
            Some("Existing unrelated toast")
        );
        let after = (
            format!("{:?}", shell.core.threads.get(&key)),
            shell.core.composer.text.clone(),
            shell.core.thread_composer.text.clone(),
            crate::thread_unread::shortcut_initial_cursor(&shell).map(str::to_owned),
        );
        assert_eq!(after, before);
        shell.thread_loading = true;
        finish_read_status(&mut shell, false);
        assert!(!shell.thread_loading);
        assert_eq!(
            shell.thread_error.as_deref(),
            Some("This reply is unavailable. It may have been deleted.")
        );
        assert_eq!(
            shell.toast.as_ref().map(|t| t.text.as_str()),
            Some("This reply is unavailable. It may have been deleted.")
        );
        assert_eq!(shell.core.thread_read_revision(&key), 17);
        assert_eq!(shell.core.thread_cursor(&key), Some(key.2.as_str()));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn accepted_reply_driver_clears_retained_failure_after_decoded_success() {
        use crate::bootstrap::test_support::harness;
        use std::{cell::Cell, rc::Rc};
        let mut h = harness();
        let mut state = h.state;
        let root = "1750000900.000001".to_owned();
        let reply = "1750000900.000002".to_owned();
        let (channel, scope, reconnect) = {
            let mut s = state.write();
            assert!(s.core.cache.is_none());
            assert!(std::env::var_os("SUPER_PLATINUM_FIXTURE").is_none());
            s.overlay = None;
            s.main_view = crate::state::MainView::Home;
            s.channel_tab = crate::state::ChannelTab::Files;
            s.begin_thread(root.clone(), Some(reply.clone()));
            s.thread_error = Some("Replies could not load. Please try again.".into());
            s.core.composer.text = "Retained channel draft".into();
            s.core.thread_composer.text = "Retained reply draft".into();
            let reconnect = super::super::reconnect::resource_test_visit(&mut s);
            (
                s.core.active_channel.clone().unwrap(),
                s.thread_scope(),
                reconnect,
            )
        };
        let done = Rc::new(Cell::new(false));
        let finished = done.clone();
        let expected_channel = channel.clone();
        let expected_root = root.clone();
        let expected_reply = reply.clone();
        let work = load_started_thread_with(
            state,
            channel.clone(),
            root.clone(),
            Some(reply.clone()),
            scope.clone(),
            Some(reconnect),
            move |transport, client, session, args| async move {
                assert_eq!(transport.d_cookie(), "synthetic");
                assert_eq!(args.channel, expected_channel);
                assert_eq!(args.ts, expected_root);
                assert_eq!(args.oldest.as_deref(), Some(expected_reply.as_str()));
                assert!(args.inclusive);
                api::fetch_replies_with(&client,&session,args,move |request| async move {
                    assert!(request.url.contains("conversations.replies"));
                    Ok(serde_json::json!({"ok":true,"messages":[
                        {"ts":expected_root,"thread_ts":expected_root,"text":"Retained root"},
                        {"ts":expected_reply,"thread_ts":expected_root,"text":"Accepted recovered reply"}
                    ],"has_more":false}))
                }).await
            },
        );
        h._dom.in_runtime(|| {
            h._dom.in_scope(dioxus::core::ScopeId::ROOT, || {
                spawn(async move {
                    work.await;
                    finished.set(true);
                })
            })
        });
        tokio::time::timeout(std::time::Duration::from_secs(3), async {
            while !done.get() {
                h._dom.render_immediate_to_vec();
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("actual replies success continuation must settle");
        let s = state.peek();
        assert!(!s.thread_loading);
        assert!(s.thread_error.is_none());
        assert_eq!(s.thread_scope(), scope);
        assert_eq!(s.thread_target.as_deref(), Some(reply.as_str()));
        assert_eq!(s.core.composer.text, "Retained channel draft");
        assert_eq!(s.core.thread_composer.text, "Retained reply draft");
        let key = ("T1".into(), channel, root);
        assert!(
            s.core.threads[&key]
                .messages
                .iter()
                .any(|m| m.ts.as_deref() == Some(reply.as_str())
                    && m.text.as_deref() == Some("Accepted recovered reply"))
        );
        assert!(s.core.threads[&key].loaded);
    }

    #[test]
    fn exact_reply_request_starts_at_target_and_includes_it() {
        let request = reply_args("C".into(), "100.000001".into(), Some("150.000002"));
        assert_eq!(request.ts, "100.000001");
        assert_eq!(request.oldest.as_deref(), Some("150.000002"));
        assert!(request.latest.is_none());
        assert!(request.inclusive);
        assert_eq!(request.limit, Some(200));
        let newest = reply_args("C".into(), "100.000001".into(), None);
        assert!(newest.oldest.is_none());
        assert!(newest.latest.is_some());
    }
}
