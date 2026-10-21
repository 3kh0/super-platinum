//! A visible unowned pill acquires observations for this invocation only.
//! Acquisition is bounded and never starts from render or replays a write.
use super::*;
use crate::emoji_catalog::Loaded;
use selection::{Observation, Source, Witness};
use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};
use super_platinum_core::composer_emoji::preferences::Preferences;

const LOADING: &str = "Loading emoji and preferences. Your draft is unchanged.";
const FAILED: &str =
    "Emoji details could not load. Click the reaction to retry. Your draft is unchanged.";
const CHANGED: &str =
    "The comment or emoji details changed. Click the reaction to retry. Your draft is unchanged.";
const INVALID: &str = "This reaction is unavailable. Your draft is unchanged.";

#[derive(Clone)]
struct Seed {
    custom_revision: u64,
    locale: Option<String>,
    loaded: Option<Arc<Loaded>>,
    preferences: Preferences,
}
impl Seed {
    fn capture(shell: &ShellState, source: &Source) -> Option<Self> {
        let workspace = shell.core.workspaces.get(&source.owner.scope.team)?;
        Some(Self {
            custom_revision: workspace.custom_emoji_revision,
            locale: workspace.locale.clone(),
            loaded: crate::emoji_catalog::current(shell),
            preferences: workspace.emoji_preferences.clone(),
        })
    }
    fn current(&self, shell: &ShellState, source: &Source) -> bool {
        source.current(shell)
            && shell
                .core
                .workspaces
                .get(&source.owner.scope.team)
                .is_some_and(|w| {
                    w.custom_emoji_revision == self.custom_revision
                        && w.locale == self.locale
                        && (self.preferences == w.emoji_preferences
                            || (!self.preferences.loaded()
                                && w.emoji_preferences.loaded()
                                && w.emoji_preferences.revision()
                                    == self.preferences.revision().wrapping_add(1)))
                })
            && self.loaded.as_ref().is_none_or(|old| {
                old.current(shell)
                    && crate::emoji_catalog::current(shell)
                        .is_some_and(|now| Arc::ptr_eq(old, &now))
            })
    }
}
#[derive(Clone)]
pub(crate) struct Pending {
    token: u64,
    source: Source,
    requested: String,
    seed: Seed,
    running: Arc<AtomicBool>,
    notice: Arc<Mutex<&'static str>>,
}
impl Pending {
    fn capture(shell: &ShellState, generation: u64, ts: &str, requested: &str) -> Option<Self> {
        if !super::available(shell, generation, ts, requested) {
            return None;
        }
        let source = Source::capture(shell, generation, ts)?;
        let row = message(shell.slack_lists.comments.entry.as_ref()?, ts)?;
        if !family::needs_addition(row, &source.owner.scope.stable_owner().2, requested) {
            return None;
        }
        let seed = Seed::capture(shell, &source)?;
        Some(Self {
            token: next_id(),
            source,
            requested: requested.into(),
            seed,
            running: Arc::new(AtomicBool::new(true)),
            notice: Arc::new(Mutex::new(LOADING)),
        })
    }
    fn installed(&self, shell: &ShellState) -> bool {
        self.source.owner.active(shell)
            && shell
                .slack_lists
                .comments
                .entry
                .as_ref()
                .and_then(|e| e.reaction_addition.as_ref())
                .is_some_and(|p| p.token == self.token)
    }
    fn current(&self, shell: &ShellState) -> bool {
        self.installed(shell)
            && self.running.load(Ordering::SeqCst)
            && self.seed.current(shell, &self.source)
    }
    fn fail(&self, notice: &'static str) {
        if let Ok(mut text) = self.notice.lock() {
            *text = notice;
        }
        self.running.store(false, Ordering::SeqCst);
    }
}
struct Completion(Pending);
impl Drop for Completion {
    fn drop(&mut self) {
        if self.0.running.load(Ordering::SeqCst) {
            self.0.fail(FAILED);
        }
    }
}
pub(super) fn busy(shell: &ShellState) -> bool {
    shell.slack_lists.comments.entry.as_ref().is_some_and(|e| {
        e.owner.active(shell)
            && e.reaction_addition
                .as_ref()
                .is_some_and(|p| p.running.load(Ordering::SeqCst))
    })
}
pub(super) fn status(shell: &ShellState, generation: u64) -> Option<String> {
    let e = shell.slack_lists.comments.entry.as_ref()?;
    if e.owner.serial != generation || !e.owner.active(shell) {
        return None;
    }
    e.reaction_addition
        .as_ref()?
        .notice
        .lock()
        .ok()
        .map(|s| s.to_string())
}
fn install(shell: &mut ShellState, pending: &Pending) -> bool {
    if !pending.seed.current(shell, &pending.source)
        || !super::available(
            shell,
            pending.source.owner.serial,
            &pending.source.ts,
            &pending.requested,
        )
    {
        return false;
    }
    shell
        .slack_lists
        .comments
        .entry
        .as_mut()
        .unwrap()
        .reaction_addition = Some(pending.clone());
    true
}
fn resolve(shell: &ShellState, pending: &Pending) -> Result<Option<Witness>, &'static str> {
    if !pending.current(shell) {
        return Err(CHANGED);
    }
    let Some(observed) = Observation::capture(shell) else {
        return Ok(None);
    };
    Witness::pill(shell, &pending.source, observed, &pending.requested)
        .map(Some)
        .ok_or(INVALID)
}
async fn acquire(state: Signal<ShellState>, pending: &Pending) -> Result<Witness, &'static str> {
    let ready = async {
        loop {
            if let Some(choice) = resolve(&state.peek(), pending)? {
                return Ok(choice);
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    };
    tokio::time::timeout(Duration::from_secs(6), ready)
        .await
        .unwrap_or(Err(FAILED))
}
async fn fail(mut state: Signal<ShellState>, pending: &Pending, notice: &'static str) {
    if !pending.installed(&state.peek()) {
        return;
    }
    let lease = draft::seal_for_render(state).await;
    pending.fail(notice);
    // If native capture refuses, its existing draft guidance is authoritative.
    // Atomic completion still releases admission; no unguarded render is forced.
    let Ok(lease) = lease else {
        return;
    };
    if pending.installed(&state.peek()) {
        drop(state.write());
    }
    if let Some(lease) = lease {
        let _ = lease.release().await;
    }
}
pub(super) fn toggle(
    mut state: Signal<ShellState>,
    generation: u64,
    ts: String,
    requested: String,
) {
    let Some(pending) = Pending::capture(&state.peek(), generation, &ts, &requested) else {
        return;
    };
    dioxus::core::spawn_forever(async move {
        let _completion = Completion(pending.clone());
        let Ok(lease) = draft::seal_for_render(state).await else {
            return;
        };
        if !install(&mut state.write(), &pending) {
            return;
        }
        // These existing services own source reads/revision adoption. Calling
        // once while sealed cannot borrow a later List target for this action.
        crate::emoji_preferences::ensure(state);
        crate::emoji_catalog::ensure(state);
        if let Some(lease) = lease {
            if lease.release().await.is_err() {
                pending.fail(FAILED);
                return;
            }
        }
        let choice = match acquire(state, &pending).await {
            Ok(choice) => choice,
            Err(notice) => {
                fail(state, &pending, notice).await;
                return;
            }
        };
        let Ok(lease) = draft::seal_for_render(state).await else {
            pending.fail(FAILED);
            return;
        };
        let work = {
            let mut shell = state.write();
            if !pending.current(&shell) || !choice.current(&shell) {
                None
            } else {
                let name = choice.intent.name().to_owned();
                model::begin_with(&mut shell, generation, &ts, &name, Some(choice))
            }
        };
        let Some(work) = work else {
            pending.fail(CHANGED);
            if let Some(lease) = lease {
                let _ = lease.release().await;
            }
            return;
        };
        pending.running.store(false, Ordering::SeqCst);
        if let Some(lease) = lease {
            if lease.release().await.is_err() {
                settle(
                    &work,
                    Err(ListCommentReactionFailure::NotRequested(unavailable())),
                );
                retire_preflight(&mut state.write(), &work);
                return;
            }
        }
        run::execute(state, work).await;
    });
}
pub(crate) fn snapshot(shell: &ShellState) -> Value {
    let Some(e) = shell
        .slack_lists
        .comments
        .entry
        .as_ref()
        .filter(|e| e.owner.active(shell))
    else {
        return Value::Null;
    };
    e.reaction_addition.as_ref().map_or(Value::Null,|p|json!({"token":p.token,"generation":e.owner.serial,
        "ts":p.source.ts,"requested":p.requested,"phase":if p.running.load(Ordering::SeqCst){"loading"}else{"failed"},
        "notice":p.notice.lock().ok().map(|s|*s)}))
}

#[cfg(test)]
mod tests;
