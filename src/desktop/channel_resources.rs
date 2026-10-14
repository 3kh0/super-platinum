//! Loading channel resources without disturbing the global Files browser.
use crate::state::{ChannelTab, ShellState};
use dioxus::prelude::*;
use super_platinum_core::{
    files::{FileSort, FilesQuery},
    links::LinksState,
    slack::api,
};

pub fn visible_links<'a>(shell: &'a ShellState, channel: &str) -> Option<&'a LinksState> {
    shell
        .core
        .channel_links
        .matches_scope(
            shell.core.account_epoch,
            shell.core.active_team.as_deref()?,
            channel,
        )
        .then_some(&shell.core.channel_links)
}

pub fn select(mut state: Signal<ShellState>, tab: ChannelTab) {
    if state.peek().channel_tab == tab {
        return;
    }
    dioxus::core::spawn_forever(async move {
        let (epoch, team, channel, generation, previous) = {
            let shell = state.peek();
            (
                shell.core.account_epoch,
                shell.core.active_team.clone(),
                shell.core.active_channel.clone(),
                shell.channel_generation,
                shell.channel_tab,
            )
        };
        let Some(channel) = channel else {
            return;
        };
        let bookmark = if previous == ChannelTab::Messages {
            // Use the center visible row, not the overscan range's first row.
            // Reading this before unmount keeps a resource round trip from
            // returning to a giant virtual spacer with no visible messages.
            dioxus::document::eval(r#"
                const t = document.getElementById('message-timeline');
                if (!t) { dioxus.send(null); }
                else {
                    const bounds = t.getBoundingClientRect();
                    const middle = (bounds.top + bounds.bottom) / 2;
                    const rows = [...t.querySelectorAll('[data-message-id]')];
                    const row = rows.find(row => {const b=row.getBoundingClientRect();return b.bottom >= middle && b.top <= bounds.bottom;});
                    dioxus.send({bottom:t.scrollHeight-t.scrollTop-t.clientHeight<48, ts:row?.dataset.messageId});
                }
            "#).recv::<Option<serde_json::Value>>().await.ok().flatten()
        } else {
            None
        };
        {
            let mut shell = state.write();
            if shell.core.account_epoch != epoch
                || shell.core.active_team != team
                || shell.core.active_channel.as_ref() != Some(&channel)
                || shell.channel_generation != generation
                || shell.channel_tab != previous
            {
                return;
            }
            use super_platinum_core::domain::PendingScrollTarget;
            crate::read_preferences::opening::cancel(&mut shell);
            if previous == ChannelTab::Messages {
                let target = if bookmark
                    .as_ref()
                    .and_then(|v| v.get("bottom"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(shell.stick_to_bottom)
                {
                    PendingScrollTarget::Latest
                } else {
                    bookmark
                        .as_ref()
                        .and_then(|v| v.get("ts"))
                        .and_then(|v| v.as_str())
                        .map(|ts| PendingScrollTarget::Message(ts.into()))
                        .unwrap_or(PendingScrollTarget::Latest)
                };
                shell.core.pending_scroll_to = Some((channel.clone(), target));
            }
            shell.channel_tab = tab;
            // Invalidate measurements and history work queued by the old DOM.
            shell.channel_generation = shell.channel_generation.wrapping_add(1);
            if tab == ChannelTab::Messages {
                if shell.core.pending_scroll_to.is_none() {
                    shell.core.pending_scroll_to = Some((channel, PendingScrollTarget::Latest));
                }
                shell.reset_timeline_window();
            }
        }
        if tab == ChannelTab::Messages {
            crate::bootstrap::refresh_selected_channel(state).await;
        } else {
            refresh(state).await;
        }
    });
}

pub async fn refresh(state: Signal<ShellState>) {
    if !state.peek().detail_open() {
        return;
    }
    if std::env::var_os("SUPER_PLATINUM_FIXTURE").is_some() {
        return;
    }
    let Some(channel) = state.peek().core.active_channel.clone() else {
        return;
    };
    let tab = state.peek().channel_tab;
    match tab {
        ChannelTab::Messages => {}
        ChannelTab::Bookmarks => crate::bookmarks::load(state, channel).await,
        ChannelTab::Pins => crate::pins::load(state, channel).await,
        ChannelTab::Files => {
            let (query, page) = crate::files::visible_for(&state.peek(), Some(&channel))
                .map(|f| (f.query.clone(), f.page))
                .unwrap_or((
                    FilesQuery {
                        channel: Some(channel),
                        sort: FileSort::Newest,
                        ..Default::default()
                    },
                    1,
                ));
            crate::files::load(state, query, page).await;
        }
        ChannelTab::Links => {
            let (query, page) = visible_links(&state.peek(), &channel)
                .map(|l| (l.query.clone(), l.page))
                .unwrap_or((String::new(), 1));
            load_links(state, channel, query, page).await;
        }
    }
}

pub async fn load_links(mut state: Signal<ShellState>, channel: String, query: String, page: u32) {
    let Some((transport, client, sessions)) = crate::bootstrap::credentials(&state) else {
        return;
    };
    let Some(team) = state.peek().core.active_team.clone() else {
        return;
    };
    let Some(session) = sessions.into_iter().find(|s| s.team_id == team) else {
        return;
    };
    let (epoch, generation) = {
        let mut shell = state.write();
        let epoch = shell.core.account_epoch;
        shell
            .core
            .channel_links
            .ensure_scope(epoch, &team, &channel);
        (epoch, shell.core.channel_links.begin(query.clone(), page))
    };
    let result =
        api::fetch_channel_links(&transport, &client, &session, &channel, &query, page).await;
    let mut shell = state.write();
    if shell.core.account_epoch != epoch
        || shell.core.active_team.as_ref() != Some(&team)
        || !shell
            .core
            .channel_links
            .matches_scope(epoch, &team, &channel)
        || shell.core.channel_links.generation != generation
        || !shell
            .core
            .transport
            .as_ref()
            .is_some_and(|current| std::sync::Arc::ptr_eq(current, &transport))
    {
        return;
    }
    match result {
        Ok(page) => {
            shell.core.channel_links.apply(generation, page);
        }
        Err(error) => {
            shell.core.channel_links.loading = false;
            shell.core.channel_links.error = Some("Links could not load. Please try again.".into());
            shell.report_failure(
                &error,
                "Links will be available when you reconnect.",
                || "Could not load channel links.".into(),
            );
        }
    }
}
