//! A local picker retains its List visit; its catalog is never durable action debt.
use super::*;
use crate::emoji_catalog::{Loaded, Selection};
use std::sync::atomic::{AtomicU8, Ordering};
use super_platinum_core::composer_emoji::{catalog::ReactionChoice, preferences::Preferences};

#[derive(Clone)]
pub(super) struct Source {
    pub(super) owner: Owner,
    pub(super) target: Target,
    pub(super) ts: String,
    socket: u64,
    original: Value,
}
#[derive(Clone)]
pub(crate) struct Picker {
    token: u64,
    source: Source,
    observation: Option<Observation>,
    binding: Arc<AtomicU8>,
    notice: Option<String>,
}
#[derive(Clone)]
pub(super) struct Observation {
    loaded: Arc<Loaded>,
    preferences: Preferences,
}
impl Observation {
    pub(super) fn capture(shell: &ShellState) -> Option<Self> {
        let loaded = crate::emoji_catalog::current(shell)?;
        let preferences = &shell
            .core
            .workspaces
            .get(shell.core.active_team.as_ref()?)?
            .emoji_preferences;
        preferences.loaded().then(|| Self {
            loaded,
            preferences: preferences.clone(),
        })
    }
    pub(super) fn current(&self, shell: &ShellState) -> bool {
        self.loaded.current(shell)
            && crate::emoji_catalog::current(shell).is_some_and(|c| Arc::ptr_eq(&c, &self.loaded))
            && shell
                .core
                .active_team
                .as_ref()
                .and_then(|t| shell.core.workspaces.get(t))
                .is_some_and(|w| {
                    w.emoji_preferences == self.preferences && w.emoji_preferences.loaded()
                })
    }
}
impl Source {
    pub(super) fn capture(shell: &ShellState, generation: u64, ts: &str) -> Option<Self> {
        if !target_available(shell, generation, ts) {
            return None;
        }
        let entry = shell.slack_lists.comments.entry.as_ref()?;
        Some(Self {
            owner: entry.owner.clone(),
            target: entry.target.clone(),
            ts: ts.into(),
            socket: shell
                .core
                .workspaces
                .get(&entry.owner.scope.team)?
                .rt_generation,
            original: serde_json::to_value(message(entry, ts)?).ok()?,
        })
    }
    pub(super) fn current(&self, shell: &ShellState) -> bool {
        self.owner.active(shell)
            && shell
                .core
                .workspaces
                .get(&self.owner.scope.team)
                .is_some_and(|w| w.rt_generation == self.socket)
            && shell.slack_lists.comments.entry.as_ref().is_some_and(|e| {
                e.target == self.target
                    && !e.deleted.contains(&self.ts)
                    && e.rows.iter().any(|r| r.ts == self.ts)
                    && message(e, &self.ts)
                        .and_then(|m| serde_json::to_value(m).ok())
                        .as_ref()
                        == Some(&self.original)
            })
    }
}
impl Picker {
    fn current(&self, shell: &ShellState) -> bool {
        self.source.current(shell)
    }
}
#[derive(Clone)]
pub(super) struct Witness {
    observation: Observation,
    source: Source,
    pub(super) intent: ReactionChoice,
}
impl Witness {
    pub(super) fn pill(
        shell: &ShellState,
        source: &Source,
        observation: Observation,
        requested: &str,
    ) -> Option<Self> {
        if !source.current(shell) || !observation.current(shell) {
            return None;
        }
        let row = message(shell.slack_lists.comments.entry.as_ref()?, &source.ts)?;
        let intent = ReactionChoice::resolve_pill_addition(
            &observation.loaded.catalog,
            &observation.preferences,
            &row.reactions,
            requested,
            &source.owner.scope.stable_owner().2,
        )
        .ok()?;
        Some(Self {
            observation,
            source: source.clone(),
            intent,
        })
    }
    pub(super) fn current(&self, shell: &ShellState) -> bool {
        self.source.current(shell)
            && self.observation.current(shell)
            && shell
                .slack_lists
                .comments
                .entry
                .as_ref()
                .and_then(|e| message(e, &self.source.ts))
                .is_some_and(|m| self.matches_fresh(shell, m))
    }
    pub(super) fn matches_fresh(&self, shell: &ShellState, message: &Message) -> bool {
        self.observation.current(shell)
            && self.intent.current(
                &self.observation.loaded.catalog,
                &self.observation.preferences,
                &message.reactions,
            )
    }
}

fn picker(shell: &ShellState, generation: u64, token: u64) -> Option<&Picker> {
    let entry = shell.slack_lists.comments.entry.as_ref()?;
    (entry.owner.serial == generation && entry.owner.active(shell)).then_some(())?;
    entry.reaction_picker.as_ref().filter(|p| p.token == token)
}
pub(super) fn has_picker(shell: &ShellState) -> bool {
    shell
        .slack_lists
        .comments
        .entry
        .as_ref()
        .is_some_and(|e| e.owner.active(shell) && e.reaction_picker.is_some())
}
fn target_available(shell: &ShellState, generation: u64, ts: &str) -> bool {
    if crate::fixture::is_fixture() && !fixture::enabled(shell) {
        return false;
    }
    shell.slack_lists.comments.entry.as_ref().is_some_and(|e| {
        e.owner.serial == generation
            && e.owner.active(shell)
            && !e.loading
            && cooldown(shell) <= now()
            && !draft::presentation(shell).is_some_and(|d| d.busy || d.loading)
            && e.target.root_ts() != Some(ts)
            && !e.deleted.contains(ts)
            && e.rows.iter().any(|r| r.ts == ts)
            && message(e, ts).is_some()
    })
}
pub(crate) fn available(shell: &ShellState, generation: u64, ts: &str) -> bool {
    if has_picker(shell) || addition::busy(shell) || !target_available(shell, generation, ts) {
        return false;
    }
    let entry = shell.slack_lists.comments.entry.as_ref().unwrap();
    shell
        .slack_lists
        .comments
        .reaction_actions
        .0
        .lock()
        .is_ok_and(|actions| {
            family::room(&actions, 1)
                && !actions.iter().any(|a| {
                    a.belongs(&entry.owner, &entry.target)
                        && (a.retry_at > now()
                            || matches!(a.phase, Phase::Preparing | Phase::Queued | Phase::Sent))
                })
        })
}
pub(super) fn can_dispatch(shell: &ShellState, generation: u64, ts: &str, name: &str) -> bool {
    if !target_available(shell, generation, ts) {
        return false;
    }
    let entry = shell.slack_lists.comments.entry.as_ref().unwrap();
    shell
        .slack_lists
        .comments
        .reaction_actions
        .0
        .lock()
        .is_ok_and(|actions| {
            family::room(&actions, 1)
                && !actions.iter().any(|a| {
                    a.belongs(&entry.owner, &entry.target)
                        && (a.retry_at > now()
                            || (a.blocking()
                                && (a.phase == Phase::Preparing
                                    || (a.ts == ts
                                        && (a.name == name
                                            || a.family.as_ref().is_some_and(|f| {
                                                reaction_family_base(name) == Some(f.base.as_str())
                                            }))))))
                })
        })
}
fn capture_picker(shell: &ShellState, generation: u64, ts: &str) -> Option<Picker> {
    if !available(shell, generation, ts) {
        return None;
    }
    let value = Picker {
        token: next_id(),
        source: Source::capture(shell, generation, ts)?,
        observation: Observation::capture(shell),
        binding: Arc::new(AtomicU8::new(0)),
        notice: None,
    };
    Some(value)
}
fn install_picker(shell: &mut ShellState, value: Picker) -> Option<u64> {
    if !available(shell, value.source.owner.serial, &value.source.ts)
        || !value.current(shell)
        || value
            .observation
            .as_ref()
            .is_some_and(|o| !o.current(shell))
    {
        return None;
    }
    let token = value.token;
    shell.slack_lists.comments.entry.as_mut()?.reaction_picker = Some(value);
    Some(token)
}
#[cfg(test)]
fn open_shell(shell: &mut ShellState, generation: u64, ts: &str) -> Option<u64> {
    let captured = capture_picker(shell, generation, ts)?;
    install_picker(shell, captured)
}
pub(crate) fn open(mut state: Signal<ShellState>, generation: u64, ts: String) {
    let Some(captured) = capture_picker(&state.peek(), generation, &ts) else {
        return;
    };
    dioxus::core::spawn_forever(async move {
        let Ok(lease) = draft::seal_for_render(state).await else {
            return;
        };
        install_picker(&mut state.write(), captured);
        if let Some(lease) = lease {
            let _ = lease.release().await;
        }
    });
}
pub(crate) fn presentation(shell: &ShellState) -> Option<(u64, u64, bool, Option<String>)> {
    let entry = shell.slack_lists.comments.entry.as_ref()?;
    if !entry.owner.active(shell) {
        return None;
    }
    let p = entry.reaction_picker.as_ref()?;
    let ready = p.current(shell) && p.observation.as_ref().is_some_and(|o| o.current(shell));
    let status = if !p.current(shell) || p.observation.as_ref().is_some_and(|o| !o.current(shell)) {
        Some("The comment or emoji preferences changed. Close and reopen the picker.".into())
    } else if p.binding.load(Ordering::SeqCst) == 2 {
        Some("Finish editing the draft, then close and reopen the picker.".into())
    } else if p.observation.is_none() {
        Some("Loading emoji and preferences. Close and reopen to retry if unavailable.".into())
    } else {
        p.notice.clone()
    };
    Some((entry.owner.serial, p.token, ready, status))
}
/// Mount completion is event-driven by existing catalog/preferences state reads.
pub(crate) fn needs_binding(shell: &ShellState, generation: u64, token: u64) -> bool {
    picker(shell, generation, token).is_some_and(|p| {
        p.observation.is_none() && p.binding.load(Ordering::SeqCst) == 0 && p.current(shell)
    }) && Observation::capture(shell).is_some()
}
pub(crate) fn bind(mut state: Signal<ShellState>, generation: u64, token: u64) {
    let (binding, observed) = {
        let shell = state.peek();
        if !needs_binding(&shell, generation, token) {
            return;
        }
        let binding = picker(&shell, generation, token).unwrap().binding.clone();
        if binding
            .compare_exchange(0, 1, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return;
        }
        (binding, Observation::capture(&shell).unwrap())
    };
    dioxus::core::spawn_forever(async move {
        let Ok(lease) = draft::seal_for_render(state).await else {
            binding.store(2, Ordering::SeqCst);
            return;
        };
        {
            let mut shell = state.write();
            if picker(&shell, generation, token)
                .is_some_and(|p| p.observation.is_none() && p.current(&shell))
            {
                shell
                    .slack_lists
                    .comments
                    .entry
                    .as_mut()
                    .unwrap()
                    .reaction_picker
                    .as_mut()
                    .unwrap()
                    .observation = Some(observed);
            }
        }
        if let Some(lease) = lease {
            let _ = lease.release().await;
        }
    });
}
fn close_shell(shell: &mut ShellState, generation: u64, token: u64) -> bool {
    // Dismiss stale contents, but never a successor token/visit.
    if picker(shell, generation, token).is_none() {
        return false;
    }
    shell
        .slack_lists
        .comments
        .entry
        .as_mut()
        .unwrap()
        .reaction_picker = None;
    true
}
pub(crate) fn close(mut state: Signal<ShellState>, generation: u64, token: u64) {
    if picker(&state.peek(), generation, token).is_none() {
        return;
    }
    dioxus::core::spawn_forever(async move {
        let Ok(lease) = draft::seal_for_render(state).await else {
            return;
        };
        close_shell(&mut state.write(), generation, token);
        if let Some(lease) = lease {
            let _ = lease.release().await;
        }
    });
}
fn admit(shell: &ShellState, generation: u64, token: u64, picked: &Selection) -> Option<Witness> {
    let p = picker(shell, generation, token)?;
    let observation = p.observation.as_ref()?;
    if !p.current(shell)
        || !observation.current(shell)
        || !Arc::ptr_eq(&picked.loaded, &observation.loaded)
        || !picked.loaded.picker.iter().any(|v| v.name == picked.name)
    {
        return None;
    }
    let row = message(shell.slack_lists.comments.entry.as_ref()?, &p.source.ts)?;
    let intent = ReactionChoice::resolve(
        &picked.loaded.catalog,
        &observation.preferences,
        &row.reactions,
        &picked.name,
    )
    .ok()?;
    if !can_dispatch(shell, generation, &p.source.ts, intent.name()) {
        return None;
    }
    Some(Witness {
        observation: observation.clone(),
        source: p.source.clone(),
        intent,
    })
}
pub(crate) fn choose(
    mut state: Signal<ShellState>,
    generation: u64,
    token: u64,
    picked: Selection,
) {
    // Freeze before awaiting native capture, then revalidate this exact choice.
    if picker(&state.peek(), generation, token).is_none() {
        return;
    }
    let Some(choice) = admit(&state.peek(), generation, token, &picked) else {
        dioxus::core::spawn_forever(async move {
            let Ok(lease) = draft::seal_for_render(state).await else {
                return;
            };
            {
                let mut shell = state.write();
                if picker(&shell, generation, token).is_some() {
                    shell.slack_lists.comments.entry.as_mut().unwrap().reaction_picker.as_mut().unwrap().notice = Some("This reaction is unavailable or changed. Choose another emoji or reopen the picker.".into());
                }
            }
            if let Some(lease) = lease {
                let _ = lease.release().await;
            }
        });
        return;
    };
    dioxus::core::spawn_forever(async move {
        let Ok(lease) = draft::seal_for_render(state).await else {
            return;
        };
        let work = {
            let mut shell = state.write();
            if picker(&shell, generation, token).is_none() || !choice.current(&shell) {
                return;
            }
            let ts = choice.source.ts.clone();
            let name = choice.intent.name().to_owned();
            let Some(work) = model::begin_with(&mut shell, generation, &ts, &name, Some(choice))
            else {
                return;
            };
            close_shell(&mut shell, generation, token);
            work
        };
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
pub(crate) fn target_ts(shell: &ShellState, generation: u64, token: u64) -> Option<String> {
    picker(shell, generation, token).map(|p| p.source.ts.clone())
}
pub(crate) fn snapshot(shell: &ShellState) -> Value {
    presentation(shell).map_or(Value::Null, |(generation, token, ready, notice)| json!({"generation":generation,"token":token,"ready":ready,"notice":notice}))
}

#[cfg(test)]
mod tests;
