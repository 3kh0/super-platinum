//! Scoped thread loading and navigation to an exact saved/pinned reply.
use crate::state::ShellState;
use dioxus::prelude::*;
use std::sync::Arc;
use super_platinum_core::slack::{Transport, api};

struct Visit {
    scope: String,
    transport: Arc<Transport>,
}
impl Visit {
    fn current(&self, shell: &ShellState) -> bool {
        shell.thread_scope() == self.scope
            && shell
                .core
                .transport
                .as_ref()
                .is_some_and(|transport| Arc::ptr_eq(transport, &self.transport))
    }
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
        scope: state.peek().thread_scope(),
        transport,
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
    let result = api::fetch_replies(
        &visit.transport,
        &client,
        &session,
        range.request(channel.clone(), root.clone()),
    )
    .await;
    if !visit.current(&state.peek()) {
        return;
    }
    match result {
        Ok(page) => {
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
                shell.thread_history.start(&range, &root, &page);
                let messages = shell
                    .core
                    .threads
                    .entry((team.clone(), channel.clone(), root.clone()))
                    .or_default();
                for message in page.messages {
                    messages.upsert(super_platinum_core::state::visible_message(message));
                }
                messages.loaded = true;
                messages.recount_thread_unreads(&root);
                if let Some(cursor) = last_read {
                    shell
                        .core
                        .confirm_thread_mark(&thread_key, &cursor, read_revision);
                }
                crate::thread_unread::remember_anchor(&mut shell);
                shell.thread_loading = false;
                if !found {
                    shell.thread_error =
                        Some("This reply is unavailable. It may have been deleted.".into());
                    shell.show_toast("This reply is unavailable. It may have been deleted.");
                }
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
    match result {
        Ok(page) => {
            {
                let mut shell = state.write();
                shell
                    .thread_history
                    .apply(direction, &request, &root, &page);
                let bag = shell
                    .core
                    .threads
                    .entry((team, channel, root.clone()))
                    .or_default();
                for message in page.messages {
                    bag.upsert(super_platinum_core::state::visible_message(message));
                }
                bag.recount_thread_unreads(&root);
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
        };
        assert!(visit.current(&shell));
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
        };
        assert!(current.current(&shell));
        shell.core.transport = Some(Arc::new(Transport::new("other-synthetic-cookie").unwrap()));
        assert!(!current.current(&shell));
        shell.core.transport = Some(current.transport.clone());
        shell.core.account_epoch += 1;
        assert!(!current.current(&shell));
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
