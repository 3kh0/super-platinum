//! Lane Add prepares typed defaults off the renderer thread, then uses the
//! ordinary strict create preflight and stable receipt. No mutation is retried.
//!
//! Attempt::board retains the lane origin through the ordinary preflight.
//! A LaneAdd component owns one Lease, retires it on unmount, and passes its clone
//! with the exact visit/layout/key to add_item. Merely finding a cached lane is
//! not evidence that the lane which received the click remains mounted.
use super::*;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use super_platinum_core::slack_lists::saved_views::ProjectionLayout;

#[cfg(test)]
mod tests;

const CHANGED: &str = "This Board lane changed. Review it before adding an item.";

/// One actual Lane component mount, never recycled for another mount.
#[derive(Clone)]
pub(crate) struct Lease(Arc<AtomicBool>);
impl Lease {
    pub(crate) fn new() -> Self {
        Self(Arc::new(AtomicBool::new(true)))
    }
    pub(crate) fn retire(&self) {
        self.0.store(false, Ordering::Release);
    }
    fn current(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

#[derive(Clone)]
pub(super) struct Origin {
    lease: Lease,
    layout: u64,
    selected: String,
    lane: String,
    viewer: String,
}
impl Origin {
    /// Called as part of Attempt::current both before and after preflight.
    /// The parent checks captured account/epoch/credentials, visit and revision.
    pub(super) fn current(&self, shell: &ShellState, intent: &CreationIntent) -> bool {
        if !self.lease.current() {
            return false;
        }
        let Some(scope) = shell
            .slack_lists
            .scope
            .as_ref()
            .filter(|s| s.current(shell))
        else {
            return false;
        };
        let Some(view) = shell.slack_lists.view.as_ref() else {
            return false;
        };
        let Some(loaded) = view.loaded.as_ref() else {
            return false;
        };
        scope.stable_owner().2 == self.viewer
            && view.layout.generation == self.layout
            && view.layout.kind == ProjectionLayout::Board
            && view.layout.selected.as_deref() == Some(self.selected.as_str())
            && view.layout.lanes.iter().any(|lane| lane.key == self.lane)
            // Also guards seed inputs changed without an intervening projection.
            // Core permits only the observed harmless top-level last_read delta.
            && intent
                .revalidate(
                    &loaded.document,
                    loaded.allow_lists.as_deref(),
                    loaded.personal_task_lists.as_ref(),
                    Some(&self.selected),
                )
                .is_ok()
    }
}

struct Pending {
    attempt: Attempt,
    seed: SeedMetadata,
}

/// Seed work never owns a Loaded/RecordsPage or clones canonical row payloads.
/// Keeping the capture signature metadata-only makes that boundary explicit.
#[derive(Clone)]
struct SeedMetadata {
    document: Document,
    allow_lists: Option<String>,
    personal_task_lists: Option<PersonalTaskLists>,
}
impl SeedMetadata {
    fn capture(
        document: &Document,
        allow_lists: Option<&str>,
        personal_task_lists: Option<&PersonalTaskLists>,
    ) -> Self {
        Self {
            document: document.clone(),
            allow_lists: allow_lists.map(str::to_owned),
            personal_task_lists: personal_task_lists.cloned(),
        }
    }
}

/// Read just this mounted action's label, without constructing the editor,
/// detail, mutation or full List presentation on each lane render.
pub(crate) fn lane_label(
    shell: &ShellState,
    visit: u64,
    layout: u64,
    lane: &str,
) -> Option<String> {
    if !layout::current(shell, visit, layout) {
        return None;
    }
    let view = shell.slack_lists.view.as_ref()?;
    if view.layout.kind != ProjectionLayout::Board {
        return None;
    }
    let lane = view.layout.lanes.iter().find(|entry| entry.key == lane)?;
    Some(if lane.label.is_empty() {
        "Items".into()
    } else {
        lane.label.clone()
    })
}

/// Cheap eligibility only. Full seed admission runs asynchronously on click.
pub(crate) fn reason(shell: &ShellState) -> Option<&'static str> {
    unavailable(shell).or_else(|| {
        let view = shell.slack_lists.view.as_ref()?;
        (view.layout.kind != ProjectionLayout::Board || view.layout.selected.is_none())
            .then_some(CHANGED)
    })
}

pub(crate) fn add_item(
    mut state: Signal<ShellState>,
    visit: u64,
    layout: u64,
    lane: String,
    lease: Lease,
) {
    let result = begin(&mut state.write(), visit, layout, &lane, lease);
    let pending = match result {
        Ok(pending) => pending,
        Err(error) => {
            if let Some(view) = state
                .write()
                .slack_lists
                .view
                .as_mut()
                .filter(|view| view.generation == visit && view.layout.generation == layout)
            {
                view.error = Some(error.into());
            }
            return;
        }
    };
    // Retained independently of the Lane component. Unmount/cancel invalidates
    // authority, but the completed worker still releases its captured snapshot.
    dioxus::core::spawn_forever(async move {
        let attempt = pending.attempt;
        let origin = attempt.board.clone().unwrap();
        let prepared = tokio::task::spawn_blocking(move || prepare(pending.seed, &origin))
            .await
            .unwrap_or(Err(
                "Could not prepare this Board item. Refresh and try again.",
            ));
        let result = prepared.and_then(|intent| install(&mut state.write(), &attempt, intent));
        match result {
            Ok(attempt) => network::dispatch(state, attempt),
            Err(error) => rejected(&mut state.write(), &attempt, error, None),
        }
    });
}

fn begin(
    shell: &mut ShellState,
    visit: u64,
    layout: u64,
    lane: &str,
    lease: Lease,
) -> Result<Pending, &'static str> {
    if !lease.current() || !layout::current(shell, visit, layout) {
        return Err(CHANGED);
    }
    if let Some(reason) = reason(shell) {
        return Err(reason);
    }
    let view = shell.slack_lists.view.as_ref().ok_or(CHANGED)?;
    if !view.layout.lanes.iter().any(|entry| entry.key == lane) {
        return Err(CHANGED);
    }
    let origin = Origin {
        lease,
        layout,
        selected: view.layout.selected.clone().ok_or(CHANGED)?,
        lane: lane.into(),
        viewer: shell
            .slack_lists
            .scope
            .as_ref()
            .filter(|scope| scope.current(shell))
            .ok_or(CHANGED)?
            .stable_owner()
            .2,
    };
    let loaded = view.loaded.as_ref().ok_or(CHANGED)?;
    let seed = SeedMetadata::capture(
        &loaded.document,
        loaded.allow_lists.as_deref(),
        loaded.personal_task_lists.as_ref(),
    );
    // Reserve the same per-owner/list lock before any await; double clicks and
    // header Add/Duplicate/cell edits cannot race this preparation.
    let mut attempt = super::begin(shell, visit, layout)?;
    attempt.board = Some(origin);
    shell
        .slack_lists
        .create
        .receipts
        .get_mut(&attempt.key())
        .unwrap()
        .attempt = attempt.clone();
    Ok(Pending { attempt, seed })
}

fn prepare(seed: SeedMetadata, origin: &Origin) -> Result<CreationIntent, &'static str> {
    CreationIntent::capture_board(
        &seed.document,
        seed.allow_lists.as_deref(),
        seed.personal_task_lists.as_ref(),
        &origin.selected,
        &origin.lane,
        &origin.viewer,
    )
    .map_err(|error| error.0)
}

fn install(
    shell: &mut ShellState,
    attempt: &Attempt,
    intent: CreationIntent,
) -> Result<Attempt, &'static str> {
    if !attempt.current(shell)
        || shell
            .slack_lists
            .create
            .receipts
            .get(&attempt.key())
            .is_none_or(|receipt| receipt.stage != Stage::Preparing)
    {
        return Err(CHANGED);
    }
    let origin = attempt.board.as_ref().ok_or(CHANGED)?;
    if !origin.current(shell, &intent) {
        return Err(CHANGED);
    }
    let mut prepared = attempt.clone();
    prepared.intent = intent;
    shell
        .slack_lists
        .create
        .receipts
        .get_mut(&attempt.key())
        .unwrap()
        .attempt = prepared.clone();
    Ok(prepared)
}
