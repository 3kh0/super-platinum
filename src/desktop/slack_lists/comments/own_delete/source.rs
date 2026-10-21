use super::*;
use super_platinum_core::slack_lists::comments::OwnEditSeed;
#[derive(Clone)]
pub(super) struct Source {
    pub owner: Owner,
    pub target: Target,
    pub ts: String,
    pub original: Message,
    socket: u64,
    events: u64,
}
impl Source {
    pub fn capture(shell: &ShellState, generation: u64, ts: &str) -> Option<Self> {
        let e = shell.slack_lists.comments.entry.as_ref()?;
        let w = shell.core.workspaces.get(&e.owner.scope.team)?;
        let page = e.page.as_ref()?;
        let original = page
            .messages()
            .iter()
            .find(|m| m.ts.as_deref() == Some(ts))?;
        if e.owner.serial != generation
            || !e.owner.active(shell)
            || e.loading
            || !OwnDeleteIntent::candidate(page, original, &w.self_user_id)
        {
            return None;
        }
        let value = Self {
            owner: e.owner.clone(),
            target: e.target.clone(),
            ts: ts.into(),
            original: original.clone(),
            socket: w.rt_generation,
            events: shell.slack_lists.comments.own_delete.events,
        };
        value.current(shell).then_some(value)
    }
    pub fn same(&self, message: &Message) -> bool {
        OwnEditSeed::same_body(&self.target, &self.original, message)
    }
    pub fn owner_current(&self, shell: &ShellState) -> bool {
        self.owner.active(shell)
            && shell
                .core
                .workspaces
                .get(&self.owner.scope.team)
                .is_some_and(|w| w.rt_generation == self.socket)
            && shell
                .slack_lists
                .comments
                .entry
                .as_ref()
                .is_some_and(|e| e.target == self.target)
    }
    pub fn current(&self, shell: &ShellState) -> bool {
        self.owner_current(shell)
            && self.events == shell.slack_lists.comments.own_delete.events
            && shell.slack_lists.comments.entry.as_ref().is_some_and(|e| {
                !e.deleted.contains(&self.ts)
                    && e.rows.iter().filter(|r| r.ts == self.ts).count() == 1
                    && e.page.as_ref().is_some_and(|p| {
                        !accepted::deleted(p, &self.ts)
                            && p.messages()
                                .iter()
                                .any(|m| m.ts.as_deref() == Some(&self.ts) && self.same(m))
                    })
            })
    }
}
