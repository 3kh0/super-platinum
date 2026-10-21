use super::*;
pub(super) const MAX_ACTIONS: usize = 128;
pub(super) fn failure_deadline(error: &Error) -> i64 {
    match error {
        Error::RateLimited {
            retry_after_secs: None,
        }
        | Error::HttpStatus {
            status: 429,
            retry_after_secs: None,
        } => now().saturating_add(30),
        _ => retry_at(error),
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Phase {
    Preparing,
    Queued,
    Sent,
    NotSent,
    Rejected,
    Unknown,
    Confirmed,
    Desired,
    Observed,
}
impl Phase {
    pub fn label(self) -> &'static str {
        match self {
            Self::Preparing => "preparing",
            Self::Queued => "queued",
            Self::Sent => "sent",
            Self::NotSent => "not-sent",
            Self::Rejected => "rejected",
            Self::Unknown => "unknown",
            Self::Confirmed => "confirmed",
            Self::Desired => "desired-state",
            Self::Observed => "observed",
        }
    }
    pub fn notice(self) -> &'static str {
        match self {
            Self::Preparing => "Checking the current reaction. Your draft is unchanged.",
            Self::Queued => "Updating the reaction family. Your draft is unchanged.",
            Self::Sent => "Updating the reaction. Your draft is unchanged.",
            Self::NotSent => "The reaction was not sent. Refresh comments before trying again.",
            Self::Rejected => "Slack rejected the reaction. Your draft is unchanged.",
            Self::Unknown => {
                "Reaction delivery is unconfirmed. Refresh comments to check before trying again."
            }
            Self::Confirmed => "Reaction updated.",
            Self::Desired => "The requested reaction state is already present.",
            Self::Observed => "The requested reaction state was observed in refreshed comments.",
        }
    }
}
#[derive(Clone)]
pub(super) struct Attempt {
    pub id: u64,
    pub scope: Scope,
    pub target: Target,
    pub ts: String,
    pub name: String,
    pub user: String,
    pub adding: bool,
    pub phase: Phase,
    pub record: Option<ReactionRecord>,
    pub raw: Option<Value>,
    pub socket_observed: bool,
    pub canonical_observed: bool,
    pub socket_revision: u64,
    pub readback: bool,
    pub published: bool,
    pub retry_at: i64,
    pub full_readback: Option<Value>,
    pub family: Option<family::Family>,
    pub issued: Option<bool>,
    pub failure: Option<&'static str>,
}
impl Attempt {
    pub fn belongs(&self, owner: &Owner, target: &Target) -> bool {
        self.scope.stable_owner() == owner.scope.stable_owner() && same_target(&self.target, target)
    }
    pub fn blocking(&self) -> bool {
        matches!(
            self.phase,
            Phase::Preparing | Phase::Queued | Phase::Sent | Phase::Unknown
        )
    }
}
pub(super) fn same_target(a: &Target, b: &Target) -> bool {
    a.list_id() == b.list_id()
        && a.record_id() == b.record_id()
        && a.channel_id() == b.channel_id()
        && a.root_ts() == b.root_ts()
}
#[derive(Clone)]
pub(super) struct Work {
    pub id: u64,
    pub owner: Owner,
    pub target: Target,
    pub ts: String,
    pub name: String,
    pub user: String,
    pub adding: bool,
    pub socket: u64,
    pub serial: u64,
    pub original: Value,
    pub ledger: State,
    pub choice: Option<selection::Witness>,
    pub family: Option<family::Family>,
}
impl Work {
    pub fn owner_current(&self, shell: &ShellState) -> bool {
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
            })
    }
    pub fn current(&self, shell: &ShellState) -> bool {
        self.owner_current(shell)
            && self
                .choice
                .as_ref()
                .is_none_or(|choice| choice.current(shell))
            && shell
                .slack_lists
                .comments
                .entry
                .as_ref()
                .is_some_and(|e| e.read == self.serial)
            && shell
                .slack_lists
                .comments
                .entry
                .as_ref()
                .and_then(|e| message(e, &self.ts))
                .is_some_and(|m| {
                    self.family.as_ref().map_or_else(
                        || serde_json::to_value(m).ok() == Some(self.original.clone()),
                        |f| f.current(m),
                    )
                })
    }
    pub fn update(&self, f: impl FnOnce(&mut Attempt)) {
        if let Ok(mut actions) = self.ledger.0.lock() {
            if let Some(a) = actions.iter_mut().find(|a| a.id == self.id) {
                f(a);
            }
        }
    }
    pub fn attempt(&self) -> Option<Attempt> {
        self.ledger
            .0
            .lock()
            .ok()?
            .iter()
            .find(|a| a.id == self.id)
            .cloned()
    }
}
pub(super) fn begin(shell: &mut ShellState, generation: u64, ts: &str, name: &str) -> Option<Work> {
    if !available(shell, generation, ts, name) {
        return None;
    }
    begin_with(shell, generation, ts, name, None)
}
pub(super) fn begin_with(
    shell: &mut ShellState,
    generation: u64,
    ts: &str,
    name: &str,
    choice: Option<selection::Witness>,
) -> Option<Work> {
    if let Some(choice) = &choice {
        if !choice.current(shell) || !selection::can_dispatch(shell, generation, ts, name) {
            return None;
        }
    }
    let entry = shell.slack_lists.comments.entry.as_ref()?;
    let owner = entry.owner.clone();
    let target = entry.target.clone();
    let user = owner.scope.stable_owner().2;
    let original = message(entry, ts)?;
    let plan = if choice.is_some() {
        None
    } else {
        family::selection(original, &user, name).ok()?
    };
    let adding = plan.is_none() && !member(original, &user, name);
    let id = next_id();
    let family =
        plan.map(|(base, names)| family::Family::new(id, base, name, names, original.clone()));
    let original = serde_json::to_value(original).ok()?;
    let socket = shell.core.workspaces.get(&owner.scope.team)?.rt_generation;
    let ledger = shell.slack_lists.comments.reaction_actions.clone();
    let mut actions = ledger.0.lock().ok()?;
    let names = family
        .as_ref()
        .map_or_else(|| vec![name.to_owned()], |f| f.names.clone());
    family::prune(&mut actions, names.len());
    if actions.len() + names.len() > MAX_ACTIONS {
        return None;
    }
    let (read_owner, serial, _, _) = controller::begin_preflight(shell)?;
    shell.slack_lists.comments.entry.as_mut()?.reaction_addition = None;
    let work = Work {
        id,
        owner: read_owner,
        target: target.clone(),
        ts: ts.into(),
        name: name.into(),
        user: user.clone(),
        adding,
        socket,
        serial,
        original,
        ledger: ledger.clone(),
        choice,
        family: family.clone(),
    };
    for (index, child) in names.into_iter().enumerate() {
        actions.push(Attempt {
            id: if index == 0 { work.id } else { next_id() },
            scope: owner.scope.receipt_only(),
            target: target.clone(),
            ts: ts.into(),
            name: child,
            user: user.clone(),
            adding,
            phase: if index == 0 {
                Phase::Preparing
            } else {
                Phase::Queued
            },
            record: None,
            raw: None,
            socket_observed: false,
            canonical_observed: false,
            socket_revision: 0,
            readback: false,
            published: false,
            retry_at: 0,
            full_readback: None,
            family: family.clone(),
            issued: None,
            failure: None,
        });
    }
    drop(actions);
    Some(work)
}
/// A failed renderer handshake must not strand its original preflight spinner.
/// It has no authority over a successor read, visit, or any draft field.
pub(super) fn retire_preflight(shell: &mut ShellState, work: &Work) {
    if !work.owner.active(shell) {
        return;
    }
    if let Some(entry) = shell
        .slack_lists
        .comments
        .entry
        .as_mut()
        .filter(|e| e.read == work.serial)
    {
        entry.read = next_id();
        entry.loading = false;
        entry.read_replaces_page = false;
    }
}
pub(super) fn finish(shell: &mut ShellState, work: &Work) {
    if !work.owner.active(shell) {
        return;
    }
    if let Some(entry) = shell
        .slack_lists
        .comments
        .entry
        .as_mut()
        .filter(|e| e.read == work.serial)
    {
        entry.loading = false;
    }
    if !work.owner_current(shell) {
        return;
    }
    let Some(a) = work.attempt() else {
        return;
    };
    if !matches!(a.phase, Phase::Confirmed | Phase::Desired)
        || a.socket_observed
        || a.canonical_observed
        || a.published
    {
        return;
    }
    let event = if a.adding {
        RtEvent::ReactionAdded {
            channel: a.target.channel_id().into(),
            ts: a.ts,
            user: a.user,
            reaction: a.name,
        }
    } else {
        RtEvent::ReactionRemoved {
            channel: a.target.channel_id().into(),
            ts: a.ts,
            user: a.user,
            reaction: a.name,
        }
    };
    // Existing narrow reducer updates all loaded stores and preserves raw pages.
    reactions::apply_local(shell, &work.owner.scope.team, work.socket, &event);
    if let Some(f) = &work.family {
        f.apply(&event, false, &work.user);
    }
    work.update(|a| a.published = true);
}
pub(super) fn settle(work: &Work, result: Result<Value, ListCommentReactionFailure>) {
    work.update(|a| match result {
        Ok(raw) => {
            a.issued = Some(true);
            a.failure = None;
            a.phase = match a.record.as_ref().map(|r| r.classify(&raw)) {
                Some(ReactionResponse::Confirmed) => Phase::Confirmed,
                Some(ReactionResponse::DesiredState) => Phase::Desired,
                Some(ReactionResponse::Rejected) => Phase::Rejected,
                _ => Phase::Unknown,
            };
            a.raw = Some(raw);
        }
        Err(ListCommentReactionFailure::NotRequested(error)) => {
            a.phase = Phase::NotSent;
            a.issued = Some(false);
            a.failure = Some(failure_kind(&error));
        }
        Err(ListCommentReactionFailure::Unconfirmed(error)) => {
            a.phase = if failure_kind(&error) == "rate-limited" {
                Phase::Rejected
            } else {
                Phase::Unknown
            };
            a.issued = Some(true);
            a.failure = Some(failure_kind(&error));
        }
    });
}
fn failure_kind(error: &Error) -> &'static str {
    match error {
        Error::RateLimited { .. } | Error::HttpStatus { status: 429, .. } => "rate-limited",
        Error::Offline(_) => "offline",
        Error::Transport(_) => "transport",
        _ => "other",
    }
}
