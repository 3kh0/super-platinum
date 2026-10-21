use super::*;
#[derive(Clone)]
pub(super) struct Source {
    pub owner: Owner,
    pub target: Target,
    pub ts: String,
    pub socket: u64,
    events: u64,
    original: Message,
}
pub(super) fn message<'a>(entry: &'a Entry, ts: &str) -> Option<&'a Message> {
    entry
        .page
        .as_ref()?
        .messages()
        .iter()
        .find(|m| m.ts.as_deref() == Some(ts))
}
impl Source {
    #[cfg(test)]
    pub(super) fn original_for_test(&self) -> Message {
        self.original.clone()
    }
    pub fn same(&self, message: &Message) -> bool {
        OwnEditSeed::same_body(&self.target, &self.original, message)
    }
    pub fn capture(shell: &ShellState, generation: u64, ts: &str) -> Option<Self> {
        let e = shell.slack_lists.comments.entry.as_ref()?;
        let known = message(e, ts)?;
        let user = &shell.core.workspaces.get(&e.owner.scope.team)?.self_user_id;
        if e.owner.serial != generation
            || !e.owner.active(shell)
            || e.loading
            || known.user.as_ref() != Some(user)
            || known.blocks.is_empty()
            || !known.files.is_empty()
            || !known.attachments.is_empty()
            || !matches!(
                known.subtype.as_deref(),
                None | Some("me_message" | "thread_broadcast")
            )
            || !known.blocks.iter().any(|b| b["type"] == "rich_text")
            || known
                .blocks
                .iter()
                .any(|b| !matches!(b["type"].as_str(), Some("rich_text" | "context")))
        {
            return None;
        }
        let source = Self {
            owner: e.owner.clone(),
            target: e.target.clone(),
            ts: ts.into(),
            socket: shell
                .core
                .workspaces
                .get(&e.owner.scope.team)?
                .rt_generation,
            events: shell.slack_lists.comments.own_edit.events,
            original: known.clone(),
        };
        source.current(shell).then_some(source)
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
            && shell.slack_lists.comments.own_edit.events == self.events
            && shell.slack_lists.comments.entry.as_ref().is_some_and(|e| {
                e.target.root_ts() != Some(self.ts.as_str())
                    && !e.deleted.contains(&self.ts)
                    && e.page
                        .as_ref()
                        .is_some_and(|p| !accepted::deleted(p, &self.ts))
                    && e.rows.iter().filter(|r| r.ts == self.ts).count() == 1
                    && message(e, &self.ts).is_some_and(|m| self.same(m))
            })
    }
}
