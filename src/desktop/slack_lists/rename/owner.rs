use super::*;
// The only credentials are in the short-lived original async job, never the registry.
#[derive(Clone)]
pub(super) struct Guard {
    pub key: Key,
    pub serial: u64,
    pub job: u64,
    pub scope: Scope,
    cookie: Option<String>,
    enterprise: Option<String>,
    visit: u64,
    read: u64,
    revision: u64,
    selection: u64,
    document: Document,
    records: Option<std::sync::Arc<RecordsPage>>,
}
fn enterprise(shell: &ShellState, team: &str) -> Option<String> {
    shell
        .core
        .session
        .as_ref()?
        .workspaces
        .get(team)?
        .enterprise_id
        .clone()
}
pub(super) fn available(shell: &ShellState) -> bool {
    readable(shell) && shell.slack_lists.view.as_ref().is_some_and(|v| !v.stale)
}
pub(super) fn readable(shell: &ShellState) -> bool {
    let Some(scope) = shell.slack_lists.scope.as_ref() else {
        return false;
    };
    let Some(view) = shell.slack_lists.view.as_ref() else {
        return false;
    };
    scope.current(shell)
        && shell.overlay == Some(Overlay::SlackList)
        && !scope.deleted(shell, &view.id)
        && !view.loading
        && !view.unavailable
        && view.retry_at <= now()
        && view.editor.is_none()
        && view.detail.is_none()
        && view.loaded.is_some()
        && !create::blocks_mutation(shell)
        && !delete::blocks_mutation(shell)
        && !shell.slack_lists.operations.contains_key(&view.id)
}
impl Guard {
    pub fn capture(shell: &ShellState, key: Key, serial: u64) -> Option<Self> {
        if !available(shell)
            || with_registry(|r| {
                r.entries
                    .get(&key)
                    .is_some_and(|e| !text_create::population_current(shell, e))
            })
        {
            return None;
        }
        Self::capture_read(shell, key, serial)
    }
    pub fn capture_read(shell: &ShellState, key: Key, serial: u64) -> Option<Self> {
        if !readable(shell) {
            return None;
        }
        let view = shell.slack_lists.view.as_ref()?;
        let scope = shell.slack_lists.scope.clone()?;
        Some(Self {
            cookie: shell.core.session.as_ref().map(|s| s.d_cookie.clone()),
            enterprise: enterprise(shell, &scope.team),
            key,
            serial,
            job: next_id(),
            scope,
            visit: view.generation,
            read: view.read,
            revision: view.revision,
            selection: view.layout.selection_generation,
            document: view.loaded.as_ref()?.document.clone(),
            records: if with_registry(|r| {
                r.entries
                    .get(&Key::current(shell)?)
                    .map(|e| e.original_records.is_some())
            })
            .unwrap_or(false)
            {
                Some(text_create::bounded_population(shell)?)
            } else {
                None
            },
        })
    }
    pub fn current(&self, shell: &ShellState, stage: Stage) -> bool {
        (if stage == Stage::Checking {
            readable(shell)
        } else {
            available(shell)
        }) && self.scope.current(shell)
            && Key::current(shell).as_ref() == Some(&self.key)
            && shell.core.session.as_ref().map(|s| &s.d_cookie) == self.cookie.as_ref()
            && enterprise(shell, &self.scope.team) == self.enterprise
            && shell.slack_lists.view.as_ref().is_some_and(|v| {
                v.generation == self.visit
                    && v.read == self.read
                    && v.revision == self.revision
                    && v.layout.selection_generation == self.selection
                    && v.loaded.as_ref().is_some_and(|l| {
                        l.document == self.document
                            && self.records.as_ref().is_none_or(|r| l.records == **r)
                    })
            })
            && with_registry(|r| {
                r.entries.get(&self.key).is_some_and(|e| {
                    e.serial == self.serial && e.job == Some(self.job) && e.stage == stage
                })
            })
    }
}
/// Read-only dismissal proof. No request authority and no retained credentials.
#[derive(Clone)]
pub(super) struct Review {
    scope: std::sync::Weak<()>,
    selection: u64,
    key: Key,
    epoch: u64,
    visit: u64,
    read: u64,
    revision: u64,
    document: Document,
    records: Option<std::sync::Arc<RecordsPage>>,
}
impl Review {
    pub fn navigation_current(&self, shell: &ShellState) -> bool {
        self.current(shell)
            && shell.slack_lists.scope.as_ref().is_some_and(|s| {
                self.scope
                    .upgrade()
                    .is_some_and(|p| std::sync::Arc::ptr_eq(&p, &s.identity))
            })
            && shell
                .slack_lists
                .view
                .as_ref()
                .is_some_and(|v| v.layout.selection_generation == self.selection)
    }
    pub fn capture(shell: &ShellState) -> Option<Self> {
        let v = shell.slack_lists.view.as_ref()?;
        Some(Self {
            scope: std::sync::Arc::downgrade(&shell.slack_lists.scope.as_ref()?.identity),
            selection: v.layout.selection_generation,
            key: Key::current(shell)?,
            epoch: shell.core.account_epoch,
            visit: v.generation,
            read: v.read,
            revision: v.revision,
            document: v.loaded.as_ref()?.document.clone(),
            records: if with_registry(|r| {
                r.entries
                    .get(&Key::current(shell)?)
                    .map(|e| e.original_records.is_some())
            })
            .unwrap_or(false)
            {
                Some(text_create::bounded_population(shell)?)
            } else {
                None
            },
        })
    }
    pub fn current(&self, shell: &ShellState) -> bool {
        readable(shell)
            && Key::current(shell).as_ref() == Some(&self.key)
            && self.epoch == shell.core.account_epoch
            && shell.slack_lists.view.as_ref().is_some_and(|v| {
                v.generation == self.visit
                    && v.read == self.read
                    && v.revision == self.revision
                    && v.loaded.as_ref().is_some_and(|l| {
                        l.document == self.document
                            && self.records.as_ref().is_none_or(|r| l.records == **r)
                    })
            })
    }
}
