use super::*;
#[derive(Clone)]
pub(super) struct Proof {
    pub scope: std::sync::Weak<()>,
    pub epoch: u64,
    pub visit: u64,
    pub open: u64,
    pub detail: u64,
    pub read: u64,
    pub revision: u64,
    pub selection: u64,
    pub selected: Option<String>,
    pub parent: String,
    pub document: Arc<Document>,
    pub page: Arc<RecordsPage>,
}
impl Proof {
    pub fn capture(shell: &ShellState, parent: &str) -> Option<Self> {
        let l = shell.slack_lists.view.as_ref()?.loaded.as_ref()?;
        subitem_input_bytes(&l.document, &l.records).ok()?;
        Self::capture_bounded(shell, parent)
    }
    pub fn capture_bounded(shell: &ShellState, parent: &str) -> Option<Self> {
        let scope = shell
            .slack_lists
            .scope
            .as_ref()
            .filter(|s| s.current(shell))?;
        let view = shell.slack_lists.view.as_ref()?;
        let loaded = view.loaded.as_ref()?;
        // Caller counted and reserved before this sole bounded deep copy.
        let (open, detail) = record_detail::subitem_origin(view, parent)?;
        Some(Self {
            scope: Arc::downgrade(&scope.identity),
            epoch: shell.core.account_epoch,
            visit: view.generation,
            open,
            detail,
            read: view.read,
            revision: view.revision,
            selection: view.layout.selection_generation,
            selected: view.layout.selected.clone(),
            parent: parent.into(),
            document: Arc::new(loaded.document.clone()),
            page: Arc::new(loaded.records.clone()),
        })
    }
    pub fn current(&self, shell: &ShellState) -> bool {
        shell.overlay == Some(Overlay::SlackList)
            && self.epoch == shell.core.account_epoch
            && shell.slack_lists.scope.as_ref().is_some_and(|s| {
                s.current(shell)
                    && self
                        .scope
                        .upgrade()
                        .is_some_and(|p| Arc::ptr_eq(&p, &s.identity))
                    && !s.deleted(shell, &self.document.id)
            })
            && shell.slack_lists.view.as_ref().is_some_and(|v| {
                v.generation == self.visit
                    && v.read == self.read
                    && v.revision == self.revision
                    && v.layout.selection_generation == self.selection
                    && v.layout.selected == self.selected
                    && !v.loading
                    && !v.unavailable
                    && v.editor.is_none()
                    && record_detail::subitem_origin(v, &self.parent)
                        == Some((self.open, self.detail))
                    && v.loaded
                        .as_ref()
                        .is_some_and(|l| l.document == *self.document && l.records == *self.page)
            })
    }
}
#[derive(Clone)]
pub(super) struct Guard {
    pub key: Key,
    pub serial: u64,
    pub job: u64,
    pub scope: Scope,
    pub proof: Proof,
}
impl Guard {
    pub fn current(&self, shell: &ShellState, stage: Stage) -> bool {
        self.scope.current(shell)
            && self.proof.current(shell)
            && (stage == Stage::Checking
                || shell.slack_lists.view.as_ref().is_some_and(|v| !v.stale))
            && !delete::blocks_mutation(shell)
            && !rename::blocks_mutation(shell)
            && !shell.slack_lists.operations.contains_key(&self.key.list)
            && with(|r| {
                r.entries.get(&self.key).is_some_and(|e| {
                    e.serial == self.serial && e.job == self.job && e.stage == stage
                })
            })
    }
}

/// Authority only to dismiss a displayed receipt, never dispatch or project.
/// No population clone is needed: this exact scope/visit/revision stamp is
/// invalidated by navigation, reads, canonical updates or credential rotation.
#[derive(Clone)]
pub(super) struct Retirement {
    scope: std::sync::Weak<()>,
    epoch: u64,
    list: String,
    visit: u64,
    read: u64,
    revision: u64,
    selection: u64,
}
impl Retirement {
    pub fn capture(shell: &ShellState) -> Option<Self> {
        let s = shell
            .slack_lists
            .scope
            .as_ref()
            .filter(|s| s.current(shell))?;
        let v = shell.slack_lists.view.as_ref()?;
        if shell.overlay != Some(Overlay::SlackList) || v.loading || v.editor.is_some() {
            return None;
        }
        Some(Self {
            scope: Arc::downgrade(&s.identity),
            epoch: shell.core.account_epoch,
            list: v.id.clone(),
            visit: v.generation,
            read: v.read,
            revision: v.revision,
            selection: v.layout.selection_generation,
        })
    }
    pub fn current(&self, shell: &ShellState) -> bool {
        Self::capture(shell).is_some_and(|p| {
            self.epoch == p.epoch
                && self.list == p.list
                && self.visit == p.visit
                && self.read == p.read
                && self.revision == p.revision
                && self.selection == p.selection
                && self.scope.ptr_eq(&p.scope)
        })
    }
}
