//! Bounded, occurrence-owned metadata reads for the visible List transcript.
//! A title read grants neither navigation nor draft/editor authority.
use super::*;
use crate::profile_edit::sections::rich::search::{
    FileTarget,
    file_source::{FileInfoRead, FileObservation},
};
use file_navigation::Source;
use serde_json::json;
use super_platinum_core::slack::{
    PreparedRequest,
    api::custom::{RichTextContext, texty::file_title_source::OrderedFileInfo},
};
pub(super) mod run;
#[cfg(test)]
mod tests;

const MAX_FILES: usize = 64;

#[derive(Clone, Default)]
pub(super) struct State {
    // Cached rows alone cannot rearm a retired socket's automatic title work.
    transcript_socket: Option<(u64, u64)>,
    generation: u64,
    attempts: Vec<Attempt>,
    worker: Option<Arc<()>>,
    cooldown: Option<(Scope, i64)>,
}
#[derive(Clone)]
struct Attempt {
    demand: Demand,
    serial: u64,
    phase: Phase,
    retry_at: i64,
    notice: Option<&'static str>,
    titles: Option<Arc<title_users::Observed>>,
}
#[derive(Clone, Copy, PartialEq)]
enum Phase {
    Queued,
    Loading,
    Ready,
    Unavailable,
}
impl Phase {
    fn name(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Loading => "loading",
            Self::Ready => "ready",
            Self::Unavailable => "unavailable",
        }
    }
}
#[derive(Clone)]
pub(crate) struct Demand {
    source: Source,
    team: String,
    raw: Value,
    ordinal: usize,
    id: String,
}
pub(crate) struct Presentation {
    pub state: &'static str,
    pub notice: Option<String>,
    pub retryable: bool,
}
impl Demand {
    pub(super) fn capture(
        shell: &ShellState,
        source: Source,
        team: &str,
        raw: &Value,
        ordinal: usize,
    ) -> Option<Self> {
        if !enabled(shell) || !matches!(raw["type"].as_str(), Some("canvas" | "file")) {
            return None;
        }
        // The supported whole-token grammar does not promote subobject links.
        if ["section_id", "record_id", "is_skill_invocation"]
            .iter()
            .any(|key| truth(&raw[*key]))
        {
            return None;
        }
        let id = raw["file_id"].as_str()?;
        if !(2..=255).contains(&id.len())
            || !id.starts_with('F')
            || !id.bytes().all(|b| b.is_ascii_alphanumeric())
        {
            return None;
        }
        let user = &shell.core.workspaces.get(team)?.self_user_id;
        RichTextContext::new_ordered(team, user)
            .ok()?
            .file(raw, "File".into())
            .ok()?;
        Some(Self {
            source,
            team: team.into(),
            raw: raw.clone(),
            ordinal,
            id: id.into(),
        })
    }
    fn current(&self, shell: &ShellState) -> bool {
        enabled(shell) && self.source.current(shell)
    }
    fn ready(&self, shell: &ShellState) -> bool {
        self.current(shell) && target(shell, &self.source, &self.team, &self.raw).is_ok()
    }
    fn same(&self, other: &Self) -> bool {
        self.id == other.id && self.team == other.team && self.source.same_source(&other.source)
    }
    fn attempt<'a>(&self, shell: &'a ShellState) -> Option<&'a Attempt> {
        shell
            .slack_lists
            .comments
            .file_acquisition
            .attempts
            .iter()
            .find(|a| a.demand.same(self) && a.demand.current(shell))
    }
    pub(crate) fn presentation(&self, shell: &ShellState) -> Presentation {
        if self.ready(shell) {
            return Presentation {
                state: "ready",
                notice: None,
                retryable: false,
            };
        }
        let current = self.current(shell);
        let attempt = current.then(|| self.attempt(shell)).flatten();
        let loading = attempt.is_some_and(|a| matches!(a.phase, Phase::Queued | Phase::Loading));
        let waiting = attempt.is_some_and(|a| a.retry_at > now()) || cooldown(shell) > now();
        Presentation {
            state: if loading { "loading" } else { "unavailable" },
            notice: Some(
                if loading {
                    "Loading file…"
                } else if waiting {
                    "Slack asked us to wait before retrying this file."
                } else {
                    attempt.and_then(|a| a.notice).unwrap_or("File unavailable")
                }
                .into(),
            ),
            retryable: current && !loading && !waiting,
        }
    }
}
fn truth(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::String(s) => !s.is_empty(),
        Value::Number(n) => n.as_f64() != Some(0.0),
        _ => true,
    }
}
fn enabled(shell: &ShellState) -> bool {
    !crate::fixture::is_fixture() || fixture::cold_files::enabled(shell)
}
pub(super) fn transcript_accepted(shell: &mut ShellState, owner: &Owner) {
    if owner.active(shell) {
        shell
            .slack_lists
            .comments
            .file_acquisition
            .transcript_socket = shell
            .core
            .workspaces
            .get(&owner.scope.team)
            .map(|w| (owner.serial, w.rt_generation));
    }
}
pub(super) fn retire(shell: &mut ShellState) {
    let debt = shell.slack_lists.comments.file_acquisition.cooldown.take();
    shell.slack_lists.comments.file_acquisition = State {
        cooldown: debt,
        ..Default::default()
    };
}
fn cooldown(shell: &ShellState) -> i64 {
    shell
        .slack_lists
        .comments
        .file_acquisition
        .cooldown
        .as_ref()
        .filter(|(scope, _)| scope.stable_current(shell))
        .map_or(0, |(_, at)| *at)
}

/// Called from the mounted Comments effect after its reactive read is released.
/// No mutation occurs when every visible demand is already represented.
pub(crate) fn demand(mut state: Signal<ShellState>, generation: u64) {
    let work = pending(&state.peek(), generation);
    if work.is_empty() {
        return;
    }
    {
        let mut shell = state.write();
        queue(&mut shell, generation, work);
    }
    start(state);
}
fn pending(shell: &ShellState, generation: u64) -> Vec<Demand> {
    if !enabled(shell) {
        return Vec::new();
    }
    let Some(entry) = shell
        .slack_lists
        .comments
        .entry
        .as_ref()
        .filter(|e| e.owner.serial == generation && e.owner.active(shell))
    else {
        return Vec::new();
    };
    let state = &shell.slack_lists.comments.file_acquisition;
    if state.transcript_socket
        != shell
            .core
            .workspaces
            .get(&entry.owner.scope.team)
            .map(|w| (entry.owner.serial, w.rt_generation))
    {
        return Vec::new();
    }
    let mut remaining = MAX_FILES.saturating_sub(
        state
            .attempts
            .iter()
            .filter(|a| a.demand.current(shell))
            .count(),
    );
    let mut work = Vec::new();
    for row in entry.rows.iter() {
        let Some(source) = Source::capture(shell, generation, row) else {
            continue;
        };
        for demand in source.cold_demands(shell) {
            if work.iter().any(|d: &Demand| d.same(&demand)) {
                continue;
            }
            let existing = state
                .attempts
                .iter()
                .find(|a| a.demand.same(&demand) && a.demand.current(shell));
            let admitted = match existing {
                Some(a) => reprojectable(shell, a),
                None if remaining > 0 && !demand.ready(shell) => {
                    remaining -= 1;
                    true
                }
                None => false,
            };
            if admitted {
                work.push(demand);
                if work.len() >= MAX_FILES {
                    return work;
                }
            }
        }
    }
    work
}
/// A fresh same-file publication may repair a previously successful title.
/// This is not an automatic retry of a failed read, nor a policy/Quip retry.
fn reprojectable(shell: &ShellState, attempt: &Attempt) -> bool {
    attempt.phase == Phase::Ready
        && attempt.demand.current(shell)
        && attempt.retry_at <= now()
        && cooldown(shell) <= now()
        && attempt
            .titles
            .as_ref()
            .is_some_and(|titles| !titles.files.metadata_current(shell))
        && FileObservation::capture(shell, &attempt.demand.team, &attempt.demand.id).is_ok()
}
fn queue(shell: &mut ShellState, generation: u64, work: Vec<Demand>) {
    if !controller::callback_current(shell, generation) {
        return;
    }
    let retained: Vec<_> = shell
        .slack_lists
        .comments
        .file_acquisition
        .attempts
        .iter()
        .filter(|a| {
            a.demand.current(shell)
                && !(work.iter().any(|d| a.demand.same(d)) && reprojectable(shell, a))
        })
        .cloned()
        .collect();
    let retry_at = cooldown(shell);
    let limited = retry_at > now();
    let state = &mut shell.slack_lists.comments.file_acquisition;
    state.generation = generation;
    state.attempts = retained;
    for demand in work {
        if state.attempts.len() >= MAX_FILES {
            break;
        }
        if !state.attempts.iter().any(|a| a.demand.same(&demand)) {
            state.attempts.push(Attempt {
                demand,
                serial: next_id(),
                phase: if limited {
                    Phase::Unavailable
                } else {
                    Phase::Queued
                },
                retry_at,
                notice: None,
                titles: None,
            });
        }
    }
}
pub(crate) fn retry(mut state: Signal<ShellState>, demand: Demand) {
    if !retry_allowed(&state.peek(), &demand) {
        return;
    }
    let generation = state
        .peek()
        .slack_lists
        .comments
        .entry
        .as_ref()
        .unwrap()
        .owner
        .serial;
    {
        let mut shell = state.write();
        // Only this original occurrence may ask for a new read. Old HTTP bytes
        // are never attached to this new serial/fence.
        shell
            .slack_lists
            .comments
            .file_acquisition
            .attempts
            .retain(|a| !a.demand.same(&demand));
        queue(&mut shell, generation, vec![demand]);
    }
    start(state);
}
fn retry_allowed(shell: &ShellState, demand: &Demand) -> bool {
    demand.current(shell) && !demand.ready(shell) && demand.presentation(shell).retryable
}
fn start(mut state: Signal<ShellState>) {
    if state
        .peek()
        .slack_lists
        .comments
        .file_acquisition
        .worker
        .is_some()
    {
        return;
    }
    let queued = state
        .peek()
        .slack_lists
        .comments
        .file_acquisition
        .attempts
        .iter()
        .any(|a| a.phase == Phase::Queued);
    if !queued {
        let wake = state
            .peek()
            .slack_lists
            .comments
            .file_acquisition
            .attempts
            .iter()
            .find(|a| a.retry_at > now())
            .cloned();
        if let Some(attempt) = wake {
            let deadline = attempt.retry_at;
            run::arm_retry(state, attempt, deadline);
        }
        return;
    }
    let worker = Arc::new(());
    state.write().slack_lists.comments.file_acquisition.worker = Some(worker.clone());
    dioxus::core::spawn_forever(run::worker(state, worker));
}
fn worker_current(shell: &ShellState, worker: &Arc<()>) -> bool {
    shell
        .slack_lists
        .comments
        .file_acquisition
        .worker
        .as_ref()
        .is_some_and(|w| Arc::ptr_eq(w, worker))
}
fn attempt_current(shell: &ShellState, attempt: &Attempt, worker: &Arc<()>) -> bool {
    worker_current(shell, worker)
        && attempt.demand.current(shell)
        && shell
            .slack_lists
            .comments
            .file_acquisition
            .attempts
            .iter()
            .any(|a| a.serial == attempt.serial && a.demand.same(&attempt.demand))
}
pub(super) fn request(context: &crate::scheduled::Context, id: &str) -> PreparedRequest {
    context.client.rest_form(
        &context.session,
        "files.info",
        vec![
            ("file", id.into()),
            ("page", "1".into()),
            ("count", "500".into()),
            ("truncate", "true".into()),
        ],
    )
}
fn failure(error: &Error) -> (i64, &'static str) {
    match error {
        Error::RateLimited { retry_after_secs }
        | Error::HttpStatus {
            status: 429,
            retry_after_secs,
        } => (
            now().saturating_add(i64::try_from(retry_after_secs.unwrap_or(30)).unwrap_or(i64::MAX)),
            "Slack asked us to wait before retrying this file.",
        ),
        Error::Offline(_) => (0, "File unavailable while offline. Retry when connected."),
        _ => (0, "File unavailable. Retry to load it again."),
    }
}
pub(crate) fn snapshot(shell: &ShellState) -> Value {
    if !cfg!(test) && !fixture::cold_files::enabled(shell) {
        return Value::Null;
    }
    let state = &shell.slack_lists.comments.file_acquisition;
    json!({"generation":state.generation,"running":state.worker.is_some(),"rows":state.attempts.iter().map(|a| {
        let p=a.demand.presentation(shell);
        json!({"id":a.demand.id,"serial":a.serial,"phase":a.phase.name(),"state":p.state,
            "current":a.demand.current(shell),"ordinal":a.demand.ordinal,"retryAt":a.retry_at,
            "retryable":p.retryable,"notice":p.notice,"titleUsers":a.titles.as_ref().map(|t|t.snapshot(shell))})
    }).collect::<Vec<_>>()})
}

/// An occurrence still owns the click; this only supplies current title facts.
pub(super) fn target(
    shell: &ShellState,
    source: &Source,
    team: &str,
    raw: &Value,
) -> Result<FileTarget, Error> {
    let display = shell
        .slack_lists
        .comments
        .file_acquisition
        .attempts
        .iter()
        .find(|a| {
            a.demand.team == team
                && a.demand.id == raw["file_id"].as_str().unwrap_or_default()
                && a.demand.source.same_source(source)
                && a.demand.current(shell)
        })
        .and_then(|a| a.titles.as_ref())
        .filter(|o| o.current(shell))
        .map(|o| o.files.clone());
    FileTarget::capture_with_titles(shell, team, raw, display)
}
