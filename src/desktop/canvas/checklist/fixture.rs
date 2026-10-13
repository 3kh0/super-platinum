//! Disconnected canonical fixtures exercise the same intent and receipt guards.
use super::*;
use super_platinum_core::canvas::edit::CheckboxIntent;
#[allow(dead_code)]
#[path = "test_snapshot.rs"]
mod snapshots;

pub(in crate::canvas) const HTML: &str = "<div class='quip-canvas-content'><h1>Checklist verification</h1><p>Existing checklist items keep their canonical identity when the reader refreshes.</p><div data-section-style='7'><ul><li id='item'><span id='item'>Check keyboard navigation and narrow layouts</span></li><li id='unmapped'>An unsupported exported item remains read-only</li></ul></div></div>";
#[derive(Default)]
pub(super) struct State {
    pub attempts: usize,
    checked: bool,
    sequence: u64,
    pending: Option<(Attempt, PreparedCheckboxEdit)>,
}
pub(in crate::canvas) fn requested() -> bool {
    crate::fixture::is_fixture()
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("canvas-checklist"
                | "canvas-checklist-narrow"
                | "canvas-checklist-saving"
                | "canvas-checklist-uncertain")
        )
}
pub(in crate::canvas) fn enabled(shell: &ShellState) -> bool {
    requested() && shell.core.transport.is_none() && shell.canvas_checklist.fixture.is_some()
}
fn canonical(checked: bool, sequence: u64) -> Arc<CanvasCollabSnapshot> {
    let original = snapshots::local_snapshot(checked, sequence);
    let mut file = original.file.clone();
    file.id = Some("Ffixture".into());
    Arc::new(
        CanvasCollabSnapshot::from_responses(file, original.init.clone(), original.load.clone())
            .unwrap(),
    )
}
pub(in crate::canvas) fn install(shell: &mut ShellState) {
    if !requested() || shell.core.transport.is_some() {
        return;
    }
    let initial = shell.canvas_checklist.fixture.is_none();
    open_visit(shell);
    if !initial {
        refresh(shell);
        return;
    }
    shell.canvas_checklist.fixture = Some(State {
        sequence: 1000,
        ..Default::default()
    });
    refresh(shell);
    let state = std::env::var("SUPER_PLATINUM_FIXTURE").unwrap_or_default();
    if matches!(
        state.as_str(),
        "canvas-checklist-saving" | "canvas-checklist-uncertain"
    ) {
        let view = presentation(shell).unwrap();
        let intent = Arc::new(
            CheckboxIntent::capture(
                view.snapshot.as_ref().unwrap(),
                "item",
                true,
                save::now_usec().unwrap(),
            )
            .unwrap(),
        );
        let attempt = begin(shell, view.visit, view.read, "item", true).unwrap();
        dispatch(shell, &attempt, intent);
        if state == "canvas-checklist-uncertain" {
            uncertain(shell);
        }
    }
}
pub(in crate::canvas) fn refresh(shell: &mut ShellState) {
    if !enabled(shell) {
        return;
    }
    let Some(read) = begin_read(shell) else {
        return;
    };
    let fixture = shell.canvas_checklist.fixture.as_ref().unwrap();
    let snapshot = canonical(fixture.checked, fixture.sequence);
    finish_read(shell, &read, Ok(snapshot));
    if let Some(attempt) = pending_attempt(shell) {
        confirm_readback(shell, &attempt, &read);
    }
}
pub(super) fn dispatch(shell: &mut ShellState, attempt: &Attempt, intent: Arc<CheckboxIntent>) {
    if !enabled(shell) {
        return;
    }
    let fixture = shell.canvas_checklist.fixture.as_ref().unwrap();
    let fresh = canonical(fixture.checked, fixture.sequence);
    let prepared = with_edit_session(shell, attempt, |session| {
        intent.prepare(&fresh, session, save::now_usec().unwrap())
    });
    let Some(Ok(prepared)) = prepared else {
        cancel_preflight(shell, attempt);
        return;
    };
    if !mark_dispatched(shell, attempt, &prepared) {
        return;
    }
    let fixture = shell.canvas_checklist.fixture.as_mut().unwrap();
    fixture.attempts += 1;
    fixture.pending = Some((attempt.clone(), prepared));
}
pub(super) fn complete(shell: &mut ShellState) -> bool {
    if !enabled(shell) {
        return false;
    }
    let Some((attempt, prepared)) = shell
        .canvas_checklist
        .fixture
        .as_mut()
        .unwrap()
        .pending
        .take()
    else {
        return false;
    };
    let sequence = prepared.base_sequence() + 1000;
    let snapshot = canonical(prepared.desired(), sequence);
    let accepted = snapshots::accepted_with_snapshot(
        &prepared,
        &snapshot,
        save::now_usec().unwrap() + 60_000_000,
    );
    let fixture = shell.canvas_checklist.fixture.as_mut().unwrap();
    fixture.checked = prepared.desired();
    fixture.sequence = sequence;
    finish(shell, &attempt, Outcome::Accepted(accepted));
    refresh(shell);
    true
}
pub(super) fn uncertain(shell: &mut ShellState) -> bool {
    if !enabled(shell) {
        return false;
    }
    let Some((attempt, _)) = shell
        .canvas_checklist
        .fixture
        .as_mut()
        .unwrap()
        .pending
        .take()
    else {
        return false;
    };
    finish(shell, &attempt, Outcome::Uncertain)
}
pub(super) fn attempts(shell: &ShellState) -> usize {
    shell
        .canvas_checklist
        .fixture
        .as_ref()
        .map_or(0, |fixture| fixture.attempts)
}
