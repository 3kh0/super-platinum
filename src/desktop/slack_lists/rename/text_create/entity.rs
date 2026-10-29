//! Creation-owned directory lifetime; never installs a fabricated cell editor.
use super::*;
use crate::slack_lists::entities::directory::{self, Kind, Query, Results};
use super_platinum_core::slack_lists::{EntityCreateDomain, EntityCreateOptions};

#[derive(Clone)]
pub(crate) struct Draft {
    options: EntityCreateOptions,
    epoch: u64,
    directory: directory::State,
    labels: std::collections::BTreeMap<String, String>,
}
fn domain(kind: ColumnCreateKind) -> Option<EntityCreateDomain> {
    match kind {
        ColumnCreateKind::PeopleSingleDefault => Some(EntityCreateDomain::People),
        ColumnCreateKind::ChannelSingleDefault => Some(EntityCreateDomain::Channel),
        _ => None,
    }
}
fn directory_kind(domain: EntityCreateDomain) -> Kind {
    match domain {
        EntityCreateDomain::People => Kind::Users,
        EntityCreateDomain::Channel => Kind::Channels,
    }
}
impl Draft {
    pub(super) fn new(shell: &ShellState, kind: ColumnCreateKind) -> Option<Self> {
        let domain = domain(kind)?;
        Some(Self {
            options: EntityCreateOptions::new(domain),
            epoch: next_id(),
            directory: directory::State::new(shell, directory_kind(domain)),
            labels: Default::default(),
        })
    }
    pub(crate) fn options(&self) -> &EntityCreateOptions {
        &self.options
    }
    /// The configuration adapter calls this only within with_edit and separately
    /// validates current selection eligibility.
    pub(super) fn replace_options(&mut self, options: EntityCreateOptions) -> bool {
        if options.domain() != self.options.domain() || options == self.options {
            return false;
        }
        self.options = options;
        self.epoch = next_id();
        self.directory.retire();
        true
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Stamp {
    pub serial: u64,
    pub visit: u64,
    pub selection: u64,
    pub choice: Choice,
    pub epoch: u64,
}
impl Stamp {
    pub(crate) fn capture(shell: &ShellState) -> Option<Self> {
        let key = Key::current(shell)?;
        let visit = generation(shell)?;
        let selection = super::super::selection(shell)?;
        let stamp = with_registry(|r| {
            let e = r.entries.get(&key)?;
            let draft = e.entity_draft.as_ref()?;
            Some(Self {
                serial: e.serial,
                visit,
                selection,
                choice: e.field_choice,
                epoch: draft.epoch,
            })
        })?;
        stamp.with(shell, |_| true).then_some(stamp)
    }
    fn with(self, shell: &ShellState, action: impl FnOnce(&mut Draft) -> bool) -> bool {
        with_edit(
            shell,
            self.serial,
            self.visit,
            self.selection,
            self.choice,
            |entry| {
                if entry.job.is_some() || entry.intent.is_some() {
                    return false;
                }
                let Some(draft) = entry.entity_draft.as_mut().filter(|d| {
                    d.epoch == self.epoch && Some(d.options.domain()) == domain(self.choice.kind)
                }) else {
                    return false;
                };
                action(draft)
            },
        )
    }
}
#[derive(Clone, PartialEq)]
pub(crate) struct Presentation {
    pub stamp: Stamp,
    pub enabled: bool,
    pub options: EntityCreateOptions,
    pub directory: directory::Presentation,
    pub selected: Vec<crate::slack_lists::entities::Choice>,
}
pub(crate) fn presentation(shell: &ShellState) -> Option<Presentation> {
    // Read the retained entry even when current document/population admission
    // refuses edits. This is presentation, never mutation authority.
    let key = Key::current(shell)?;
    let visit = generation(shell)?;
    let selection = super::super::selection(shell)?;
    let mut result = with_registry(|r| {
        let e = r
            .entries
            .get(&key)
            .filter(|e| e.target.is_text_creation())?;
        let draft = e
            .entity_draft
            .as_ref()
            .filter(|d| Some(d.options.domain()) == domain(e.field_choice.kind))?;
        Some(Presentation {
            stamp: Stamp {
                serial: e.serial,
                visit,
                selection,
                choice: e.field_choice,
                epoch: draft.epoch,
            },
            enabled: false,
            options: draft.options.clone(),
            directory: draft.directory.presentation(),
            selected: draft
                .options
                .selected_ids()
                .iter()
                .map(|id| crate::slack_lists::entities::Choice {
                    id: id.clone(),
                    label: draft.labels.get(id).cloned().unwrap_or_else(|| id.clone()),
                })
                .collect(),
        })
    })?;
    result.enabled = result.stamp.with(shell, |_| true);
    Some(result)
}

pub(crate) struct Request {
    key: Key,
    scope: Scope,
    stamp: Stamp,
    query: Query,
}
impl Request {
    pub(crate) fn current(&self, shell: &ShellState) -> bool {
        self.scope.current(shell)
            && Key::current(shell).as_ref() == Some(&self.key)
            && self
                .stamp
                .with(shell, |draft| draft.directory.current(&self.query))
    }
}
pub(crate) fn begin(shell: &ShellState, stamp: Stamp, text: String) -> Option<Request> {
    let scope = shell.slack_lists.scope.clone()?;
    let key = Key::current(shell)?;
    if !scope.current(shell) || !stamp.with(shell, |_| true) {
        return None;
    }
    // Ranking/allocations happen outside the registry mutex; a second guarded
    // commit refuses a changed owner before touching any directory state.
    let (query, choices, fixture) =
        directory::prepare(shell, directory_kind(domain(stamp.choice.kind)?), text);
    let mut admitted = false;
    let committed = stamp.with(shell, |draft| {
        admitted = draft.directory.begin(&query, choices, fixture);
        true
    });
    (committed && admitted).then_some(Request {
        key,
        scope,
        stamp,
        query,
    })
}
pub(crate) fn finish(shell: &ShellState, request: &Request, result: Result<Results, Error>) {
    if !request.current(shell) {
        return;
    }
    let update = directory::resolved(shell, &request.query, result);
    request.stamp.with(shell, |draft| {
        if !draft.directory.current(&request.query) {
            return false;
        }
        draft.directory.finish(&request.query, update);
        true
    });
}
pub(crate) fn search(mut state: Signal<ShellState>, stamp: Stamp, text: String) {
    let request = begin(&state.peek(), stamp, text);
    // Also publish cached/cooldown updates when no remote request was admitted.
    state.write();
    let Some(request) = request else {
        return;
    };
    if crate::slack_lists::fixture::enabled(&state.peek()) {
        return;
    }
    spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        if !request.current(&state.peek()) {
            return;
        }
        let Some(context) = request.scope.request(&state.peek()) else {
            finish(&state.peek(), &request, Err(Error::TransportNotConfigured));
            state.write();
            return;
        };
        complete_with(state, request, &context, |request| {
            context.transport.execute(request)
        })
        .await;
    });
}

fn retire_owned(mut state: Signal<ShellState>, request: &Request) {
    let changed = {
        let shell = state.peek();
        request.current(&shell)
            && request
                .stamp
                .with(&shell, |draft| draft.directory.cancel(&request.query))
    };
    if changed {
        // The registry is external to the shell signal; notify only after an
        // admitted cleanup so its stopped spinner is rendered.
        state.write();
    }
}

async fn complete_with<F, Fut>(
    mut state: Signal<ShellState>,
    request: Request,
    context: &crate::scheduled::Context,
    execute: F,
) where
    F: FnMut(super_platinum_core::slack::PreparedRequest) -> Fut,
    Fut: std::future::Future<Output = Result<Value, Error>>,
{
    let current =
        |shell: &ShellState| request.current(shell) && directory::client_current(shell, context);
    let received =
        directory::fetch_with(context, &request.query, execute, || current(&state.peek())).await;
    if !current(&state.peek()) {
        retire_owned(state, &request);
        return;
    }
    match received {
        Ok(Some(rows)) => finish(&state.peek(), &request, Ok(rows)),
        Err(error) => finish(&state.peek(), &request, Err(error)),
        Ok(None) => {
            retire_owned(state, &request);
            return;
        }
    }
    state.write();
}

pub(crate) mod settings;

#[cfg(test)]
mod continuation_tests;

#[cfg(test)]
mod tests;
