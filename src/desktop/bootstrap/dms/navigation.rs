use super::{Arc, Owner, ShellState};
use crate::state::{ChannelOpen, ChannelTab, MainView};
use dioxus::prelude::*;
use std::{
    future::{Future, poll_fn},
    task::Poll,
    time::Duration,
};
use super_platinum_core::slack::{
    Error, PreparedRequest, api,
    models::{Channel, GenericInfoPage},
};

/// One account/visit frame shared across rendered rows, never a profile snapshot.
pub(crate) struct Frame {
    owner: Arc<Owner>,
    input: u64,
    query: String,
    unread: bool,
    channel: Option<String>,
    channel_generation: u64,
    thread_generation: u64,
    profile_generation: u64,
    palette_generation: u64,
    search_generation: u64,
    link_generation: u64,
    shortcut: u64,
    tab: ChannelTab,
    tab_revision: u64,
}
impl Frame {
    pub(super) fn owner(&self) -> Arc<Owner> {
        self.owner.clone()
    }
    pub(crate) fn capture(shell: &ShellState) -> Option<Arc<Self>> {
        if shell.main_view != MainView::Dms || shell.overlay.is_some() {
            return None;
        }
        Some(Arc::new(Self {
            owner: Owner::capture(shell)?,
            input: shell.dm_input_revision,
            query: shell.dm_query.clone(),
            unread: shell.dm_unread_only,
            channel: shell.core.active_channel.clone(),
            channel_generation: shell.channel_generation,
            thread_generation: shell.thread_generation,
            profile_generation: shell.profile_generation,
            palette_generation: shell.palette_generation,
            search_generation: shell.search_open_generation,
            link_generation: shell.slack_link_generation,
            shortcut: shell.shortcut_generation,
            tab: shell.channel_tab,
            tab_revision: crate::channel_tabs::selection::revision(shell),
        }))
    }
    pub(super) fn current(&self, shell: &ShellState) -> bool {
        self.owner.current(shell)
            && shell.signed_in
            && !shell.loading
            && shell.main_view == MainView::Dms
            && shell.overlay.is_none()
            && shell.dm_input_revision == self.input
            && shell.dm_query == self.query
            && shell.dm_unread_only == self.unread
            && shell.core.active_channel == self.channel
            && shell.channel_generation == self.channel_generation
            && shell.thread_generation == self.thread_generation
            && shell.profile_generation == self.profile_generation
            && shell.palette_generation == self.palette_generation
            && shell.search_open_generation == self.search_generation
            && shell.slack_link_generation == self.link_generation
            && shell.shortcut_generation == self.shortcut
            && shell.channel_tab == self.tab
            && crate::channel_tabs::selection::revision(shell) == self.tab_revision
    }
}
#[derive(Clone)]
pub(crate) struct Source {
    frame: Arc<Frame>,
    id: String,
}
impl Source {
    pub(crate) fn for_row(frame: &Arc<Frame>, id: &str) -> Option<Self> {
        (!id.trim().is_empty()).then(|| Self {
            frame: frame.clone(),
            id: id.into(),
        })
    }
    fn current(&self, shell: &ShellState) -> bool {
        self.frame.current(shell)
            && shell
                .core
                .dms
                .entries
                .iter()
                .any(|e| e.id == self.id && shell.dm_row_visible(e))
    }
    fn begin(self, shell: &mut ShellState) -> Option<Arc<Opening>> {
        if !self.current(shell) || is_opening(shell, &self.id) {
            return None;
        }
        let opening = Arc::new(Opening { source: self });
        shell.dm_open = Some(opening.clone());
        Some(opening)
    }
}
/// Ephemeral pointer-owned busy slot; an older completion cannot clear a newer one.
pub(crate) struct Opening {
    source: Source,
}
fn current(shell: &ShellState, opening: &Arc<Opening>) -> bool {
    shell
        .dm_open
        .as_ref()
        .is_some_and(|o| Arc::ptr_eq(o, opening))
        && opening.source.current(shell)
}
// Used only after this request synchronously admitted its own metadata. That
// metadata can change unread eligibility; the accepted click still owns its
// original frame, list identity and busy slot, with no intervening await.
fn accepted_handoff(shell: &ShellState, opening: &Arc<Opening>) -> bool {
    shell
        .dm_open
        .as_ref()
        .is_some_and(|o| Arc::ptr_eq(o, opening))
        && opening.source.frame.current(shell)
        && shell
            .core
            .dms
            .entries
            .iter()
            .any(|e| e.id == opening.source.id)
}
pub(crate) fn cancel(shell: &mut ShellState) {
    shell.dm_open = None;
}
pub(crate) fn is_opening(shell: &ShellState, id: &str) -> bool {
    shell
        .dm_open
        .as_ref()
        .is_some_and(|o| o.source.id == id && current(shell, o))
}
struct Lease {
    state: Signal<ShellState>,
    opening: Arc<Opening>,
}
impl Drop for Lease {
    fn drop(&mut self) {
        if let Ok(mut shell) = self.state.try_write() {
            if shell
                .dm_open
                .as_ref()
                .is_some_and(|o| Arc::ptr_eq(o, &self.opening))
            {
                shell.dm_open = None;
            }
        }
    }
}
pub(crate) fn open(mut state: Signal<ShellState>, source: Source) {
    let Some(opening) = state
        .try_write()
        .ok()
        .and_then(|mut s| source.begin(&mut s))
    else {
        return;
    };
    dioxus::core::spawn_forever(async move {
        let owner = opening.source.frame.owner.clone();
        navigate_with(state,opening,|request|owner.transport.execute(request),||async move {Box::pin(super::super::history::refresh_selected_channel(state)).await}).await;
    });
}
fn dm(channel: &Channel) -> bool {
    channel.is_im || channel.is_mpim
}
fn ordinary_channel(channel: &Channel) -> bool {
    !dm(channel) && (channel.is_channel || channel.is_group)
}
fn unavailable(mut state: Signal<ShellState>, opening: &Arc<Opening>, error: Option<&Error>) {
    let Ok(mut shell) = state.try_write() else {
        return;
    };
    if !current(&shell, opening) {
        return;
    }
    if let Some(error) = error {
        shell.report_failure(
            error,
            "Connect to Slack to open this direct message",
            || "Could not open this direct message. Please try again.".into(),
        );
    } else {
        shell.show_toast("This direct message is unavailable. Please try again.");
    }
}
async fn while_current<T>(future: impl Future<Output = T>, check: impl Fn() -> bool) -> Option<T> {
    let mut future = std::pin::pin!(future);
    let guarded = poll_fn(|cx| {
        if !check() {
            Poll::Ready(None)
        } else {
            future.as_mut().poll(cx).map(Some)
        }
    });
    let retired = async {
        loop {
            if !check() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    };
    tokio::select! {biased;_=retired=>None,result=guarded=>result}
}

pub(super) async fn navigate_with<F, Fut, R, RFut>(
    mut state: Signal<ShellState>,
    opening: Arc<Opening>,
    execute: F,
    refresh: R,
) -> bool
where
    F: FnOnce(PreparedRequest) -> Fut,
    Fut: Future<Output = Result<serde_json::Value, Error>>,
    R: FnOnce() -> RFut,
    RFut: Future<Output = ()>,
{
    let lease = Lease {
        state,
        opening: opening.clone(),
    };
    let owner = opening.source.frame.owner.clone();
    let id = opening.source.id.clone();
    let Some((known, revision)) = state.try_read().ok().and_then(|shell| {
        if !current(&shell, &opening) {
            return None;
        }
        let ws = shell.core.workspaces.get(&owner.team)?;
        Some((
            ws.channels.get(&id).cloned(),
            ws.channel_lifecycle.revision(),
        ))
    }) else {
        return false;
    };
    if known.as_ref().is_some_and(ordinary_channel) {
        unavailable(state, &opening, None);
        return false;
    }
    let mut adopted = false;
    if !known.as_ref().is_some_and(dm) {
        let posting =
            super::super::posting_ingress::capture(&state, &owner.transport, &owner.session)
                .for_channels([id.as_str()]);
        let Some(result) = while_current(
            api::fetch_conversation_info_with(&owner.client, &owner.session, id.clone(), execute),
            || state.try_read().ok().is_some_and(|s| current(&s, &opening)),
        )
        .await
        else {
            return false;
        };
        let info = match result {
            Ok(info) if info.id == id && dm(&info) => info,
            Ok(_) => {
                unavailable(state, &opening, None);
                return false;
            }
            Err(error) => {
                unavailable(state, &opening, Some(&error));
                return false;
            }
        };
        let accepted = {
            let Ok(mut shell) = state.try_write() else {
                return false;
            };
            if !current(&shell, &opening) {
                return false;
            }
            let Some(ws) = shell.core.workspaces.get_mut(&owner.team) else {
                return false;
            };
            if ws.channels.get(&id).is_some_and(ordinary_channel)
                || (!ws.channels.contains_key(&id)
                    && ws.channel_lifecycle.channel_revision(&id) > revision)
            {
                false
            } else {
                ws.apply_channel_refresh_since(
                    GenericInfoPage {
                        channels: vec![info.clone()],
                        ..Default::default()
                    },
                    revision,
                );
                ws.channels.get(&id).is_some_and(dm)
            }
        };
        if !accepted {
            unavailable(state, &opening, None);
            return false;
        }
        adopted = true;
        // Only the accepted actual response contributes external-routing history.
        posting.channels(&state, std::slice::from_ref(&info));
    }
    let selected = {
        let Ok(mut shell) = state.try_write() else {
            return false;
        };
        let owned = if adopted {
            accepted_handoff(&shell, &opening)
        } else {
            current(&shell, &opening)
        };
        if !owned || crate::canvas_create::departure(&mut shell) {
            return false;
        }
        let Some(ws) = shell.core.workspaces.get(&owner.team) else {
            return false;
        };
        let Some(info) = ws.channels.get(&id).filter(|c| dm(c)) else {
            return false;
        };
        // Reconcile only the canonical destination row. Do not rebuild the old
        // transcript before selection checkpoints its composer/read anchor.
        let row = crate::channel_vm::channel_vm(ws, info, &shell.media);
        let index = if let Some(index) = shell.channels.iter().position(|c| c.id == id) {
            shell.channels[index] = row;
            index
        } else {
            shell.channels.push(row);
            shell.channels.len() - 1
        };
        if shell
            .dm_open
            .as_ref()
            .is_some_and(|o| Arc::ptr_eq(o, &opening))
        {
            shell.dm_open = None;
        }
        shell.select_channel(index, ChannelOpen::InSurface);
        shell.main_view == MainView::Dms
            && shell.core.active_channel.as_deref() == Some(id.as_str())
    };
    if selected
        && state.try_read().ok().is_some_and(|s| {
            owner.current(&s)
                && s.main_view == MainView::Dms
                && s.core.active_channel.as_deref() == Some(id.as_str())
        })
    {
        refresh().await;
    }
    drop(lease);
    selected
}

#[cfg(test)]
mod tests;
