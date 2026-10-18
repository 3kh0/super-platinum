use super::*;
#[derive(Clone)]
pub(super) struct Guard {
    pub key: Key,
    pub serial: u64,
    pub job: u64,
    pub scope: Scope,
    pub draft: u64,
    visit: u64,
    revision: u64,
    read: u64,
    selection: u64,
    layout: u64,
}
impl Guard {
    pub fn capture(shell: &ShellState, e: &Entry) -> Result<Self, &'static str> {
        let key = Key::current(shell).ok_or("Open the List again.")?;
        if owner_blocked(&key) {
            return Err("Wait for the sharing cooldown before checking or assigning again.");
        }
        let view = shell
            .slack_lists
            .view
            .as_ref()
            .ok_or("Open the List again.")?;
        let draft = view.editor.as_ref().ok_or("Open the cell again.")?;
        if e.cooldown_unknown
            || draft.row_id != e.row
            || draft.column_id != e.column
            || draft.attempt.is_some()
            || draft.visit != view.generation
            || draft.revision != view.revision
            || !draft.authority.admits(view, &e.row, &e.column)
            || view.stale
            || view.loading
            || view.unavailable
            || view.retry_at > now()
            || rename::blocks_mutation(shell)
            || create::blocks_mutation(shell)
            || delete::blocks_mutation(shell)
            || reminder::blocks_mutation(shell)
            || completion::blocks_mutation(shell)
            || shell.slack_lists.operations.contains_key(&view.id)
        {
            return Err("The editor changed. No assignment or share was sent.");
        }
        cell_receipts::capacity(shell)?;
        let loaded = view.loaded.as_ref().ok_or("Wait for the List to load.")?;
        if loaded.records != *e.records || loaded.document.raw != *draft.document_raw {
            return Err("The List or its loaded page changed. No assignment was sent.");
        }
        e.intent
            .revalidate(
                &loaded.document,
                &loaded.records,
                loaded.allow_lists.as_deref(),
                loaded
                    .personal_task_lists
                    .as_ref()
                    .ok_or("Refresh task preferences.")?,
            )
            .map_err(|_| "The assignment or your access changed.")?;
        Ok(Self {
            key,
            serial: e.serial,
            job: next_id(),
            scope: shell.slack_lists.scope.clone().unwrap(),
            draft: draft.generation,
            visit: view.generation,
            revision: view.revision,
            read: view.read,
            selection: view.layout.selection_generation,
            layout: view.layout.generation,
        })
    }
    pub fn entry(&self) -> Option<Entry> {
        registry(|r| {
            r.entries
                .get(&self.key)
                .filter(|e| e.serial == self.serial && (e.job.is_none() || e.job == Some(self.job)))
                .cloned()
        })
    }
    pub fn current(&self, shell: &ShellState) -> bool {
        self.scope.current(shell)
            && shell
                .slack_lists
                .scope
                .as_ref()
                .is_some_and(|s| Arc::ptr_eq(&s.identity, &self.scope.identity))
            && checkbox_visit_current(shell, self.visit)
            && self.entry().is_some_and(|e| {
                e.deadline.is_none_or(|d| Instant::now() >= d) && Guard::capture(shell, &e).is_ok()
            })
            && shell.slack_lists.view.as_ref().is_some_and(|v| {
                v.generation == self.visit
                    && v.revision == self.revision
                    && v.read == self.read
                    && v.layout.selection_generation == self.selection
                    && v.layout.generation == self.layout
                    && v.editor
                        .as_ref()
                        .is_some_and(|d| d.generation == self.draft)
            })
    }
    pub fn update(&self, f: impl FnOnce(&mut Entry)) {
        registry(|r| {
            if let Some(e) = r
                .entries
                .get_mut(&self.key)
                .filter(|e| e.serial == self.serial && (e.job.is_none() || e.job == Some(self.job)))
            {
                f(e);
            }
        });
    }
}
