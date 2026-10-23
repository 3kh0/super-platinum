//! Owned Canvas comment reader with separately guarded resolve/reopen custody.
//! Virtual-channel messages never enter the active conversation; no read marks or replies.
use super::checklist::scope::Scope;
#[cfg(test)]
use crate::state::Overlay;
use crate::state::ShellState;
use dioxus::prelude::*;
use std::time::Instant;
use super_platinum_core::{
    canvas::comments::{Filter, ReplyPage, StreamPage},
    slack::{Error, api, models::Message},
};
pub(in crate::canvas) mod annotation;
pub(crate) mod attribution;
pub(crate) mod check;
pub(crate) mod emoji_fixture;
pub(in crate::canvas) mod fixture;
pub(crate) use fixture::attribution::install as fixture_install_attribution;
pub(crate) use fixture::attribution::recovery as editor_recovery_fixture;
mod file_acquisition;
mod file_navigation;
mod file_source;
pub(in crate::canvas) mod focus;
pub(in crate::canvas) mod mutation;
mod pages;
mod projection;
mod rate;
pub(in crate::canvas) mod reply;
mod selection;
use pages::Direction;
mod view;
pub(super) use view::Panel;
pub(in crate::canvas) use view::ProfileToken;
pub(crate) use view::{clear_profile_hover_token, clear_profile_token, profile_hover_token};
pub(crate) use view::{profile_active, profile_overlay, profile_visible};
#[cfg(test)]
mod tests;

/// Loaded shells contain fresh comment roots. Move the original retained roots
/// into that shell before replacement; account/visit guards remain attached to
/// every entry and are not rebound to the newly selected account.
pub(crate) fn carry_pending(previous: &mut ShellState, loaded: &mut ShellState) {
    reply::carry_pending(previous, loaded);
    mutation::carry_pending(previous, loaded);
    annotation::carry_pending(previous, loaded);
}

const MAX_ROWS: usize = 200;
#[derive(Clone)]
struct Owner {
    scope: Scope,
    file: String,
    visit: u64,
}
impl Owner {
    fn capture(shell: &ShellState) -> Option<Self> {
        let value = Self {
            scope: Scope::capture(shell)?,
            file: shell.canvas.as_ref()?.id.clone(),
            visit: shell.shortcut_generation,
        };
        value.current(shell).then_some(value)
    }
    fn current(&self, shell: &ShellState) -> bool {
        self.scope.current(shell)
            && !self.scope.deleted(shell, &self.file)
            && super::surface::visible(shell, &self.file)
            && shell.shortcut_generation == self.visit
            && shell.canvas.as_ref().is_some_and(|v| v.id == self.file)
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
enum Query {
    Stream(Filter),
    Replies(String),
}
#[derive(Default)]
pub(super) struct State {
    attribution: attribution::State,
    file_source: file_source::State,
    file_acquisition: file_acquisition::State,
    pending_file: Option<String>,
    selected_control: Option<selection::Pending>,
    selected_thread: Option<selection::Selected>,
    owner: Option<Owner>,
    serial: u64,
    query: Option<Query>,
    rows: Vec<Row>,
    // Original selected-map witness used for these cached body projections.
    // Missing remains unobserved; this is never comment/reply authority.
    emoji_standard: Option<crate::app_home::channel_fetches::EmojiStandard>,
    cursor: Option<String>,
    pages: pages::Pages,
    loading: bool,
    error: Option<&'static str>,
    retry_at: Option<Instant>,
    rate_blocked: bool,
    filter: Filter,
    root: Option<Message>,
    file: Option<super_platinum_core::slack::models::File>,
    mutation_ready: bool,
    review: Option<mutation::review::Draft>,
    review_serial: u64,
    mutation_error: Option<&'static str>,
    mutation_receipt:
        Option<super_platinum_core::canvas::edit::structure::recovery::journal::CommentReceipt>,
}
struct Row {
    message: Message,
    body: crate::model::MessageVm,
    preview: Option<Message>,
    preview_body: Option<crate::model::MessageVm>,
}
#[derive(Clone)]
struct Ticket {
    owner: Owner,
    serial: u64,
    query: Query,
    cursor: Option<String>,
    direction: Direction,
}
impl State {
    fn accepts(&self, shell: &ShellState, ticket: &Ticket) -> bool {
        ticket.owner.current(shell)
            && self.serial == ticket.serial
            && self.query.as_ref() == Some(&ticket.query)
    }
    fn cooling(&self) -> bool {
        self.rate_blocked || self.retry_at.is_some_and(|time| Instant::now() < time)
    }
    fn next_serial(&mut self) {
        self.attribution = Default::default();
        file_source::retire(&mut self.file_source);
        file_acquisition::retire(&mut self.file_acquisition);
        self.pending_file = None;
        self.review = None;
        self.serial = self.serial.wrapping_add(1);
    }
}
pub(super) fn open(mut state: Signal<ShellState>, mut model: Signal<State>) {
    // Comments replace the Canvas body. A previously clicked link must not
    // finish resolving later and remove this newly opened comment/reply view.
    cancel_pending_link(&mut state.write());
    selection::clear(&mut model.write());
    if model.peek().query.is_none() {
        if let Some(root) = reply::fixture::initial_root(&state.peek()) {
            let owner = Owner::capture(&state.peek());
            if let Some(owner) = owner {
                let Some(ts) = root.ts.clone() else {
                    return;
                };
                {
                    let mut value = model.write();
                    value.owner = Some(owner);
                    value.root = Some(root);
                }
                start(state, model, Query::Replies(ts), false);
                return;
            }
        }
    }
    let filter = model.peek().filter;
    start(state, model, Query::Stream(filter), false);
}
fn cancel_pending_link(shell: &mut ShellState) {
    shell.slack_link_generation = shell.slack_link_generation.wrapping_add(1);
}
fn start(state: Signal<ShellState>, model: Signal<State>, query: Query, more: bool) {
    begin(
        state,
        model,
        query,
        if more {
            Direction::Next
        } else {
            Direction::First
        },
    );
}
fn begin(state: Signal<ShellState>, mut model: Signal<State>, query: Query, direction: Direction) {
    let more = direction != Direction::First;
    let owner = Owner::capture(&state.peek());
    let Some(owner) = owner else { return };
    if model
        .peek()
        .owner
        .as_ref()
        .is_some_and(|previous| !previous.current(&state.peek()))
    {
        *model.write() = State::default();
    }
    if let Some(wait) = rate::current(&state.peek()) {
        let mut value = model.write();
        selection::before_read(&state.peek(), &mut value, &query);
        value.rows.clear();
        value.root = None;
        value.cursor = None;
        value.pages = Default::default();
        value.next_serial();
        value.owner = Some(owner.clone());
        value.query = Some(query);
        value.loading = false;
        value.error = Some("Slack asked us to wait before reading more comments.");
        drop(value);
        set_wait(state, model, owner, wait);
        return;
    }
    let ticket = {
        let mut value = model.write();
        if value.cooling()
            || value.loading
            || (direction == Direction::Next && value.cursor.is_none())
            || (direction == Direction::Previous && value.pages.previous().is_none())
        {
            return;
        }
        if (more || matches!(query, Query::Replies(_)))
            && value
                .owner
                .as_ref()
                .is_none_or(|owner| !owner.current(&state.peek()))
        {
            return;
        }
        if more && value.query.as_ref() != Some(&query) {
            return;
        }
        if !more {
            if let Query::Replies(root) = &query {
                if !selection::begin_replies(&state.peek(), &mut value, root) {
                    return;
                }
            } else {
                value.root = None;
            }
            value.rows.clear();
            value.cursor = None;
            value.pages = Default::default();
        }
        selection::before_read(&state.peek(), &mut value, &query);
        if let Query::Stream(filter) = query {
            value.filter = filter;
        }
        value.next_serial();
        value.loading = true;
        value.error = None;
        value.query = Some(query.clone());
        value.owner = Some(owner.clone());
        Ticket {
            owner,
            serial: value.serial,
            query,
            cursor: if direction == Direction::Previous {
                value.pages.previous().unwrap()
            } else {
                value.cursor.clone()
            },
            direction,
        }
    };
    spawn(async move {
        fetch(state, model, ticket).await;
    });
}
async fn fetch(state: Signal<ShellState>, mut model: Signal<State>, ticket: Ticket) {
    let result = read(state, &ticket).await;
    if let Err(Error::RateLimited { retry_after_secs }) = &result {
        if ticket.owner.scope.current(&state.peek()) {
            rate::remember(&state.peek(), *retry_after_secs);
        }
    }
    if !model.peek().accepts(&state.peek(), &ticket) {
        return;
    }
    match result {
        Ok((page, file, files)) => {
            // A selected annotation remains bound to the actual numeric root
            // through every reply read, before projection or write admission.
            if !selection::admit_page(&state.peek(), &model.peek(), &ticket, &page) {
                selection::refuse(&mut model.write());
                return;
            }
            mutation::load(state, model, &ticket).await;
            if !model.peek().accepts(&state.peek(), &ticket) {
                return;
            }
            let shell = state.peek();
            let Some(workspace) = shell.core.workspaces.get(&ticket.owner.scope.team) else {
                return;
            };
            let mut value = model.write();
            let (messages, cursor) = match page {
                Page::Stream(page) => (
                    page.threads
                        .into_iter()
                        .map(|t| (t.root, t.last_reply))
                        .collect::<Vec<_>>(),
                    page.cursor,
                ),
                Page::Replies(page) => (
                    page.messages.into_iter().map(|m| (m, None)).collect(),
                    page.cursor,
                ),
            };
            if messages.len() > MAX_ROWS {
                value.loading = false;
                value.error =
                    Some("This comment page exceeds the supported size. Open the canvas in Slack.");
                return;
            }
            let ids = messages
                .iter()
                .filter_map(|(m, _)| m.ts.clone())
                .collect::<Vec<_>>();
            if let Err(error) = value.pages.accept(
                ticket.direction,
                ticket.cursor.clone(),
                cursor.as_deref(),
                &ids,
            ) {
                value.loading = false;
                value.error = Some(error);
                value.cursor = None;
                return;
            }
            value.file = Some(file);
            value.rows.clear();
            let standard =
                crate::app_home::channel_fetches::emoji_standard(&shell, &ticket.owner.scope.team);
            if let Some((read, messages)) = files {
                // Original JSON preserves absent fields. Publication is admitted
                // only after the same page, selection and pagination checks.
                let _ = file_source::publish(&shell, &mut value, &ticket, &read, &messages);
            }
            for (message, preview) in messages {
                if matches!(&ticket.query, Query::Replies(root) if message.ts.as_ref() == Some(root))
                {
                    value.root = Some(message.clone());
                }
                let row = Row {
                    body: projection::row_vm(
                        workspace,
                        &message,
                        &shell.media,
                        standard.as_ref().map(|observed| observed.standard()),
                    ),
                    preview_body: preview.as_ref().map(|m| {
                        crate::message_vm::message_vm_with_standard(
                            workspace,
                            m,
                            &shell.media,
                            false,
                            standard.as_ref().map(|observed| observed.standard()),
                        )
                    }),
                    preview,
                    message,
                };
                if let Some(index) = value
                    .rows
                    .iter()
                    .position(|r| r.message.ts == row.message.ts)
                {
                    value.rows[index] = row;
                } else {
                    value.rows.push(row);
                }
            }
            value.emoji_standard = standard;
            value.cursor = cursor;
            value.loading = false;
            value.error = None;
            selection::accepted(&mut value, &ticket);
        }
        Err(error) => {
            let limited = matches!(error, Error::RateLimited { .. });
            let mut value = model.write();
            value.loading = false;
            value.error = Some(if limited {
                "Slack asked us to wait before reading more comments."
            } else {
                "Comments could not be loaded. Your canvas is unchanged. Retry when ready."
            });
            drop(value);
            if limited {
                if let Some(wait) = rate::current(&state.peek()) {
                    set_wait(state, model, ticket.owner.clone(), wait);
                }
            }
        }
    }
    file_acquisition::after_read(state, model, &ticket);
    attribution::after_read(state, model, &ticket);
    mutation::after_read(state, model).await;
    selection::after_read(state, model, &ticket);
}
fn set_wait(state: Signal<ShellState>, mut model: Signal<State>, owner: Owner, wait: rate::Wait) {
    match wait {
        rate::Wait::Blocked => {
            model.write().rate_blocked = true;
        }
        rate::Wait::Until(deadline) => {
            model.write().retry_at = Some(deadline);
            // One owner-bound wake enables explicit retry; it makes no request.
            spawn(async move {
                tokio::time::sleep(deadline.saturating_duration_since(Instant::now())).await;
                if owner.current(&state.peek()) && model.peek().retry_at == Some(deadline) {
                    model.write().retry_at = None;
                }
            });
        }
    }
}

enum Page {
    Stream(StreamPage),
    Replies(ReplyPage),
}
async fn read(
    state: Signal<ShellState>,
    ticket: &Ticket,
) -> Result<
    (
        Page,
        super_platinum_core::slack::models::File,
        Option<(file_source::Read, Vec<serde_json::Value>)>,
    ),
    Error,
> {
    let invalid = || Error::Api("canvas_comments_unavailable".into());
    if !ticket.owner.current(&state.peek()) {
        return Err(invalid());
    }
    let files_read = file_source::Read::capture(&state.peek(), &ticket.owner);
    let (response, file) = if crate::fixture::is_fixture() {
        (fixture::response(&state.peek(), ticket)?, fixture::file())
    } else {
        let ctx = ticket
            .owner
            .scope
            .request(&state.peek())
            .ok_or_else(invalid)?;
        let file = api::fetch_canvas_file(
            &ctx.transport,
            &ctx.client,
            &ctx.session,
            &ticket.owner.file,
        )
        .await?;
        if !ticket.owner.current(&state.peek()) {
            return Err(invalid());
        }
        let request = request(&ctx.client, &ctx.session, ticket)?;
        (ctx.transport.execute(request).await?, file)
    };
    let files = files_read.zip(response.get("messages").and_then(|v| v.as_array()).cloned());
    let ticket = ticket.clone();
    let page = tokio::task::spawn_blocking(move || match ticket.query {
        Query::Stream(_) => StreamPage::parse(&ticket.owner.file, &response).map(Page::Stream),
        Query::Replies(root) => ReplyPage::parse(
            &ticket.owner.file,
            &root,
            ticket.cursor.is_none(),
            &response,
        )
        .map(Page::Replies),
    })
    .await
    .map_err(|_| invalid())?
    .map_err(|_| invalid())?;
    Ok((page, file, files))
}
fn request(
    client: &super_platinum_core::slack::client::SlackClient,
    session: &super_platinum_core::config::WorkspaceSession,
    ticket: &Ticket,
) -> Result<super_platinum_core::slack::client::PreparedRequest, Error> {
    match &ticket.query {
        Query::Stream(filter) => api::canvas_comments(
            client,
            session,
            &ticket.owner.file,
            *filter,
            ticket.cursor.as_deref(),
        ),
        Query::Replies(root) => api::canvas_comment_replies(
            client,
            session,
            &ticket.owner.file,
            root,
            ticket.cursor.as_deref(),
        ),
    }
}

pub(crate) fn install_fixture(shell: &mut ShellState) {
    mutation::fixture::install(shell);
    reply::fixture::install(shell);
    annotation::fixture::install(shell);
    fixture::canonical_files::install(shell);
    emoji_fixture::install(shell);
}
