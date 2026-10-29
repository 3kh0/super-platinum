//! The three native resource read slots: request authority and local cleanup.
use crate::state::{ChannelTab, MainView, ShellState};
use std::{rc::Rc, sync::Arc};
use super_platinum_core::{
    config::WorkspaceSession,
    slack::{SlackClient, Transport},
};

#[derive(Clone, Copy)]
pub(crate) enum Slot {
    Files,
    ChannelFiles,
    Links,
}
impl Slot {
    fn index(self) -> usize {
        match self {
            Self::Files => 0,
            Self::ChannelFiles => 1,
            Self::Links => 2,
        }
    }
}
#[derive(Default)]
pub(crate) struct Root {
    pending: [Option<Arc<Ticket>>; 3],
}
#[derive(Clone)]
pub(crate) enum Route {
    Files,
    ChannelFiles(String),
    Links(String),
    Picker {
        channel: String,
        visit: crate::channel_tabs::Visit,
    },
}
impl Route {
    fn slot(&self) -> Slot {
        match self {
            Self::Files => Slot::Files,
            Self::Links(_) => Slot::Links,
            _ => Slot::ChannelFiles,
        }
    }
    pub(crate) fn channel(&self) -> Option<&str> {
        match self {
            Self::Files => None,
            Self::ChannelFiles(c) | Self::Links(c) | Self::Picker { channel: c, .. } => Some(c),
        }
    }
}
// Never Debug/serialize/persist: the captured native credentials stay private.
pub(crate) struct Context {
    owner: Arc<crate::bootstrap::ReadOwner>,
    pub(crate) transport: Arc<Transport>,
    pub(crate) client: SlackClient,
    pub(crate) session: WorkspaceSession,
    pub(crate) team: String,
    pub(crate) epoch: u64,
    pub(crate) route: Route,
    view: MainView,
    thread_generation: u64,
    channel_generation: u64,
    reconnect: Option<crate::bootstrap::reconnect::Visit>,
}
impl Context {
    pub(crate) fn capture(
        shell: &ShellState,
        route: Route,
        reconnect: Option<crate::bootstrap::reconnect::Visit>,
    ) -> Option<Rc<Self>> {
        if !shell.signed_in {
            return None;
        }
        let team = shell.core.active_team.clone()?;
        let session = shell.core.session.as_ref()?.workspaces.get(&team)?.clone();
        let transport = shell.core.transport.clone()?;
        let owner = Arc::new(crate::bootstrap::ReadOwner::capture(
            &shell.core,
            &transport,
            &session,
        )?);
        let context = Rc::new(Self {
            owner,
            transport,
            client: shell.core.client.clone(),
            session,
            team,
            epoch: shell.core.account_epoch,
            route,
            view: shell.main_view,
            thread_generation: shell.thread_generation,
            channel_generation: shell.channel_generation,
            reconnect,
        });
        context.current(shell).then_some(context)
    }
    pub(crate) fn authority_current(&self, shell: &ShellState) -> bool {
        shell.signed_in
            && shell.core.active_team.as_ref() == Some(&self.team)
            && self.owner.current(&shell.core)
    }
    pub(crate) fn current(&self, shell: &ShellState) -> bool {
        if !self.authority_current(shell)
            || self.reconnect.as_ref().is_some_and(|v| !v.current(shell))
        {
            return false;
        }
        match &self.route {
            // Global Files owns its rail visit, never an incidental conversation.
            Route::Files => {
                shell.main_view == MainView::Files
                    && shell.thread_generation == self.thread_generation
            }
            Route::Picker { channel, visit } => {
                visit.native_read_current(shell) && visit.channel() == channel.as_str()
            }
            Route::ChannelFiles(channel) | Route::Links(channel) => {
                let tab = if matches!(&self.route, Route::Links(_)) {
                    ChannelTab::Links
                } else {
                    ChannelTab::Files
                };
                shell.main_view == self.view
                    && shell.thread_generation == self.thread_generation
                    && shell.channel_generation == self.channel_generation
                    && shell.channel_tab == tab
                    && shell.core.active_channel.as_ref() == Some(channel)
                    && shell.conversation_visible()
            }
        }
    }
    pub(crate) fn claim(&self, shell: &mut ShellState, generation: u64) -> Arc<Ticket> {
        let ticket = Arc::new(Ticket {
            slot: self.route.slot(),
            epoch: self.epoch,
            team: self.team.clone(),
            channel: self.route.channel().map(str::to_owned),
            generation,
        });
        shell.resource_reads.pending[ticket.slot.index()] = Some(ticket.clone());
        ticket
    }
}
// One token per active slot, reset with the constructed shell. A token grants
// local busy cleanup only; HTTP/result publication still needs full Context.
pub(crate) struct Ticket {
    slot: Slot,
    epoch: u64,
    team: String,
    channel: Option<String>,
    pub(crate) generation: u64,
}
impl Ticket {
    fn owns_slot(&self, shell: &ShellState) -> bool {
        shell.resource_reads.pending[self.slot.index()]
            .as_ref()
            .is_some_and(|current| std::ptr::eq(current.as_ref(), self))
    }
    pub(crate) fn active(&self, shell: &ShellState) -> bool {
        self.owns_slot(shell)
            && match self.slot {
                Slot::Files => {
                    shell.core.files.matches_scope(self.epoch, &self.team)
                        && shell.core.files.query.channel == self.channel
                        && shell.core.files.accepts(self.generation)
                }
                Slot::ChannelFiles => {
                    shell
                        .core
                        .channel_files
                        .matches_scope(self.epoch, &self.team)
                        && shell.core.channel_files.query.channel == self.channel
                        && shell.core.channel_files.accepts(self.generation)
                }
                Slot::Links => {
                    self.channel.as_deref().is_some_and(|c| {
                        shell
                            .core
                            .channel_links
                            .matches_scope(self.epoch, &self.team, c)
                    }) && shell.core.channel_links.accepts(self.generation)
                }
            }
    }
    pub(crate) fn clear(&self, shell: &mut ShellState) {
        if self.owns_slot(shell) {
            shell.resource_reads.pending[self.slot.index()] = None;
        }
    }
    pub(crate) fn cancel(&self, shell: &mut ShellState) {
        if self.active(shell) {
            match self.slot {
                Slot::Files => {
                    shell.core.files.cancel(self.generation);
                }
                Slot::ChannelFiles => {
                    shell.core.channel_files.cancel(self.generation);
                }
                Slot::Links => {
                    shell.core.channel_links.cancel(self.generation);
                }
            }
        }
        self.clear(shell);
    }
}

/// Closed named snapshots only; no session, transport, or native activity.
pub(crate) fn fixture(shell: &mut ShellState, links: bool) {
    use super_platinum_core::{
        files::{FilesPage, FilesQuery},
        links::{ChannelLink, LinksPage},
        slack::models::{File, SearchPagination},
    };
    shell.core.transport = None;
    shell.core.session = None;
    shell.overlay = None;
    shell.close_thread();
    shell.core.composer.text = "Keep resource draft".into();
    let epoch = shell.core.account_epoch;
    let team = shell.core.active_team.clone().unwrap();
    let pagination = Some(SearchPagination {
        page: Some(1),
        page_count: Some(4),
        total_count: Some(73),
        ..Default::default()
    });
    if links {
        shell.main_view = MainView::Home;
        shell.channel_tab = ChannelTab::Links;
        let channel = shell.core.active_channel.clone().unwrap();
        let state = &mut shell.core.channel_links;
        state.ensure_scope(epoch, &team, &channel);
        let generation = state.begin("project".into(), 1);
        state.apply(
            generation,
            LinksPage {
                items: vec![ChannelLink {
                    url: "https://example.test/retained".into(),
                    title: Some("Retained project reference".into()),
                    timestamp: "1719800400.000000".into(),
                    ..Default::default()
                }],
                pagination,
            },
        );
        let generation = state.begin(state.query.clone(), 3);
        state.fail(generation, "Links could not load. Please try again.");
    } else {
        shell.main_view = MainView::Files;
        let state = &mut shell.core.files;
        state.ensure_scope(epoch, &team);
        let generation = state.begin(
            FilesQuery {
                text: "project".into(),
                ..Default::default()
            },
            1,
        );
        state.apply(
            generation,
            FilesPage {
                items: ["FRESOURCE1", "FRESOURCE2"]
                    .into_iter()
                    .map(|id| File {
                        id: Some(id.into()),
                        title: Some(format!("Retained project reference {id}")),
                        filetype: Some("pdf".into()),
                        ..Default::default()
                    })
                    .collect(),
                pagination,
            },
        );
        let generation = state.begin(state.query.clone(), 3);
        state.fail(generation, "Files could not load. Please try again.");
    }
}
#[cfg(test)]
pub(crate) mod render_test;
