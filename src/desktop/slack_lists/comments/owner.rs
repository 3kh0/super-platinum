use super::*;

#[derive(Clone)]
pub(super) struct Owner {
    pub scope: Scope,
    pub serial: u64,
    visit: u64,
    detail: u64,
    open: u64,
    original: Target,
}
impl Owner {
    pub fn capture(shell: &ShellState, generation: u64) -> Option<(Self, Target, String)> {
        let scope = shell.slack_lists.scope.clone()?;
        let view = shell.slack_lists.view.as_ref()?;
        if !scope.current(shell)
            || scope.deleted(shell, &view.id)
            || shell.overlay != Some(Overlay::SlackList)
        {
            return None;
        }
        let (open, target, title) = record_detail::comments_target(view, generation)?;
        Some((
            Self {
                scope,
                serial: next_id(),
                visit: view.generation,
                detail: generation,
                open,
                original: target.clone(),
            },
            target,
            title,
        ))
    }
    pub fn current(&self, shell: &ShellState) -> bool {
        self.scope.current(shell)
            && !self.scope.deleted(shell, self.original.list_id())
            && shell.overlay == Some(Overlay::SlackList)
            && shell
                .slack_lists
                .scope
                .as_ref()
                .is_some_and(|s| Arc::ptr_eq(&s.identity, &self.scope.identity))
            && shell.slack_lists.view.as_ref().is_some_and(|v| {
                v.generation == self.visit
                    && record_detail::comments_target(v, self.detail).is_some_and(
                        |(open, target, _)| open == self.open && target == self.original,
                    )
            })
    }
    pub fn active(&self, shell: &ShellState) -> bool {
        self.current(shell)
            && shell.slack_lists.comments.open
            && shell
                .slack_lists
                .comments
                .entry
                .as_ref()
                .is_some_and(|e| e.owner.serial == self.serial)
    }
    pub fn reading(&self, shell: &ShellState, serial: u64) -> bool {
        self.active(shell)
            && shell
                .slack_lists
                .comments
                .entry
                .as_ref()
                .is_some_and(|e| e.read == serial && e.loading)
    }
    pub fn same_record(&self, other: &Self) -> bool {
        self.scope.stable_owner() == other.scope.stable_owner()
            && self.original.list_id() == other.original.list_id()
            && self.original.record_id() == other.original.record_id()
    }
}
