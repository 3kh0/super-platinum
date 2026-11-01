//! Narrow native ordinary-root intent, never a count-to-permission conversion.
//! Source's permalink/card/draft/AI policies remain separate.
use super::*;
use super_platinum_core::slack::models::Message;

pub(in crate::bootstrap::search) fn ordinary_root(message: &Message) -> bool {
    message.ts.as_deref().is_some_and(|ts| {
        !ts.is_empty() && message.thread_ts.as_deref().is_none_or(|root| root == ts)
    }) && message.kind.as_deref().is_none_or(|kind| kind == "message")
        && message
            .subtype
            .as_deref()
            .is_none_or(|kind| kind == "bot_message")
}

pub(crate) fn reply_count(shell: &ShellState, index: usize) -> Option<u32> {
    let book = shell
        .search_message_pages
        .as_ref()
        .filter(|b| b.current(shell))?;
    if known_deleted(shell, book, index) {
        return None;
    }
    let row = book.pages.get(&book.shown)?.get(index)?;
    if row.thread_ts.is_some() || row.channel_id.is_empty() || row.ts.is_empty() {
        return None;
    }
    let supplied = book
        .bodies
        .get(&book.shown)?
        .get(&(row.channel_id.clone(), row.ts.clone()))?;
    if !supplied.coherent || !supplied.ordinary_root {
        return None;
    }
    let workspace = shell.core.workspaces.get(&book.frame.team)?;
    let thread = shell.core.threads.get(&(
        book.frame.team.clone(),
        row.channel_id.clone(),
        row.ts.clone(),
    ));
    if thread.is_some_and(|bag| bag.was_deleted(&row.ts)) {
        return None;
    }
    let shared = workspace
        .messages
        .get(&row.channel_id)
        .and_then(|bag| {
            bag.messages
                .iter()
                .find(|m| m.ts.as_deref() == Some(&row.ts))
        })
        .or_else(|| {
            thread.and_then(|bag| {
                bag.messages
                    .iter()
                    .find(|m| m.ts.as_deref() == Some(&row.ts))
            })
        });
    let count = if let Some(message) = shared {
        if !ordinary_root(message)
            || message
                .channel
                .as_deref()
                .is_some_and(|c| c != row.channel_id)
            || message
                .team
                .as_deref()
                .is_some_and(|t| t != book.frame.team)
        {
            return None;
        }
        // A known explicit zero is authoritative in this narrower native policy.
        message.reply_count.or(supplied.reply_count)
    } else {
        supplied.reply_count
    };
    count.filter(|n| *n > 0)
}

#[derive(Clone)]
pub(crate) struct ThreadSource {
    row: RowSource,
}
impl ThreadSource {
    pub(crate) fn capture(shell: &ShellState, index: usize) -> Option<Self> {
        let row = RowSource::capture(shell, index)?;
        reply_count(shell, index)?;
        Some(Self { row })
    }
    pub(in crate::bootstrap) fn resolve(&self, shell: &ShellState) -> Option<SearchResultVm> {
        let mut hit = self.row.resolve(shell)?;
        reply_count(shell, self.row.index)?;
        hit.thread_ts = Some(hit.ts.clone());
        Some(hit)
    }
    pub(in crate::bootstrap) fn proof(&self) -> OpenProof {
        self.row.proof()
    }
}
pub(crate) fn open_thread(state: Signal<ShellState>, source: ThreadSource) {
    super::super::super::search_navigation::open_message_page_thread(state, source);
}
