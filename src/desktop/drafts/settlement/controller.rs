use super::*;
use crate::drafts::Owner;
use crate::state::{ComposerTarget, MainView, ShellState};
use dioxus::prelude::*;
use super_platinum_core::slack::Transport;

#[derive(Clone)]
pub(crate) struct Prepared {
    pub review: LocalReview,
    owner: Owner,
    editor: Option<Arc<checkpoint::Editor>>,
    serial: u64,
    visit: Visit,
}
#[derive(Clone)]
struct Visit {
    account: String,
    epoch: u64,
    transport: Option<Arc<Transport>>,
    channel_generation: u64,
    thread_generation: u64,
    key: DraftKey,
    live: Live,
}
#[derive(Clone)]
struct Live {
    book: Option<LocalDraft>,
    channel: Option<LocalDraft>,
    thread: Option<LocalDraft>,
}
impl Live {
    fn capture(shell: &ShellState, key: &DraftKey) -> Self {
        Self {
            book: shell.core.drafts.items.get(key).cloned(),
            channel: (shell.composer_key(ComposerTarget::Channel).as_ref() == Some(key)).then(
                || LocalDraft {
                    composer: shell.core.composer.clone(),
                    attachments: shell.core.composer_attachments.clone(),
                },
            ),
            thread: (shell.composer_key(ComposerTarget::Thread).as_ref() == Some(key)).then(|| {
                LocalDraft {
                    composer: shell.core.thread_composer.clone(),
                    attachments: shell.core.thread_composer_attachments.clone(),
                }
            }),
        }
    }
    fn same(&self, other: &Self) -> bool {
        same_optional(&self.book, &other.book)
            && same_optional(&self.channel, &other.channel)
            && same_optional(&self.thread, &other.thread)
    }
    fn represented(&self, review: &LocalReview) -> bool {
        [&self.book, &self.channel, &self.thread]
            .into_iter()
            .flatten()
            .all(|local| review.contains_local(local))
    }
}
fn same_optional(a: &Option<LocalDraft>, b: &Option<LocalDraft>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => same_local(a, b),
        (None, None) => true,
        _ => false,
    }
}
impl Visit {
    fn account_current(&self, shell: &ShellState) -> bool {
        shell.core.active_account.as_deref() == Some(&self.account)
            && shell.core.account_epoch == self.epoch
            && session::same_transport(&self.transport, &shell.core.transport)
    }
    fn current(&self, shell: &ShellState) -> bool {
        self.account_current(shell)
            && shell.main_view == MainView::Scheduled
            && shell.core.active_team.as_ref() == Some(&self.key.team)
            && shell.channel_generation == self.channel_generation
            && shell.thread_generation == self.thread_generation
            && self.live.same(&Live::capture(shell, &self.key))
    }
}

pub(crate) async fn open(
    state: Signal<ShellState>,
    account_id: String,
    epoch: u64,
    key: DraftKey,
) -> Result<Prepared, Fault> {
    let (visit, editor, serial) = {
        let shell = state.try_peek().map_err(|_| Fault::Stale)?;
        if shell.core.active_account.as_deref() != Some(&account_id)
            || shell.core.account_epoch != epoch
            || shell.main_view != MainView::Scheduled
            || shell.core.active_team.as_ref() != Some(&key.team)
        {
            return Err(Fault::Stale);
        }
        let editor = shell.draft_writes.writers.get(&account_id, epoch, &key);
        let snapshot = editor.as_ref().map(|editor| editor.snapshot());
        if snapshot.as_ref().is_some_and(|snapshot| snapshot.running) {
            return Err(Fault::Pending);
        }
        let serial = snapshot.map_or(0, |snapshot| snapshot.serial);
        let visit = Visit {
            account: account_id.clone(),
            epoch,
            transport: shell.core.transport.clone(),
            channel_generation: shell.channel_generation,
            thread_generation: shell.thread_generation,
            key: key.clone(),
            live: Live::capture(&shell, &key),
        };
        (visit, editor, serial)
    };
    let owner = account(&account_id)?.owner(key);
    let review = owner.review_local().await?;
    if !state.try_peek().is_ok_and(|shell| visit.current(&shell))
        || !visit.live.represented(&review)
    {
        return Err(Fault::Stale);
    }
    if let Some(editor) = &editor {
        let current = editor.snapshot();
        if current.serial != serial
            || current.running
            || current
                .pending
                .as_ref()
                .is_some_and(|pending| !review.contains_local(pending))
        {
            return Err(Fault::Stale);
        }
    }
    Ok(Prepared {
        review,
        owner,
        editor,
        serial,
        visit,
    })
}

/// Called in spawn_forever. The immutable owner's transaction completes even if
/// the panel disappears; only matching UI projections may consume its result.
pub(crate) async fn apply(
    mut state: Signal<ShellState>,
    prepared: Prepared,
    choice: LocalChoice,
) -> Result<(), Fault> {
    if !state
        .try_peek()
        .is_ok_and(|shell| prepared.visit.current(&shell))
    {
        return Err(Fault::Stale);
    }
    let view = if let Some(editor) = prepared.editor {
        editor
            .settle(prepared.review.clone(), choice, prepared.serial)
            .await?
    } else {
        prepared
            .owner
            .settle_local(prepared.review.clone(), choice)
            .await?
    };
    let mut field = None;
    if let Ok(mut shell) = state.try_write()
        && prepared.visit.account_current(&shell)
    {
        shell
            .managed_drafts
            .settled(&prepared.review, prepared.visit.epoch, view.clone());
        field = apply_local(&mut shell, &prepared.visit, &view.local);
        shell.show_toast("The selected local version is saved. Nothing was sent.");
    }
    if let Some((id, scope, expected, text)) = field {
        crate::view::composer::replace_reviewed_field(id, &scope, &expected, &text);
    }
    Ok(())
}

fn apply_local(
    shell: &mut ShellState,
    visit: &Visit,
    chosen: &LocalDraft,
) -> Option<(&'static str, String, String, String)> {
    if !visit.account_current(shell) {
        return None;
    }
    let live = Live::capture(shell, &visit.key);
    let empty = LocalDraft::default();
    let expected_channel = visit
        .live
        .channel
        .as_ref()
        .or(visit.live.book.as_ref())
        .unwrap_or(&empty);
    let expected_thread = visit
        .live
        .thread
        .as_ref()
        .or(visit.live.book.as_ref())
        .unwrap_or(&empty);
    // A bound native field may be ahead of the Book. Require both to match the
    // reviewed content; navigation alone does not turn unchanged text into an edit.
    if !same_optional(&live.book, &visit.live.book)
        || live
            .channel
            .as_ref()
            .is_some_and(|local| !same_local(local, expected_channel))
        || live
            .thread
            .as_ref()
            .is_some_and(|local| !same_local(local, expected_thread))
    {
        return None;
    }
    if chosen.empty() {
        shell.core.drafts.items.remove(&visit.key);
    } else {
        shell
            .core
            .drafts
            .items
            .insert(visit.key.clone(), chosen.clone());
    }
    if live.channel.is_some() {
        shell.core.composer = chosen.composer.clone();
        shell.core.composer_attachments = chosen.attachments.clone();
        return Some((
            "channel-composer",
            crate::view::composer::field_scope(shell, ComposerTarget::Channel),
            expected_channel.composer.text.clone(),
            chosen.composer.text.clone(),
        ));
    }
    if live.thread.is_some() {
        shell.core.thread_composer = chosen.composer.clone();
        shell.core.thread_composer_attachments = chosen.attachments.clone();
        return Some((
            "thread-composer",
            crate::view::composer::field_scope(shell, ComposerTarget::Thread),
            expected_thread.composer.text.clone(),
            chosen.composer.text.clone(),
        ));
    }
    None
}

#[cfg(test)]
mod tests;
