//! A rendered List reply owns a file click; neither the URL nor a later visit
//! may supply replacement authority while its original draft is being saved.
use super::*;
use crate::model::{MessageVm, RichNode};
use crate::profile_edit::sections::rich::search::FileTarget;
use serde_json::Value;

#[cfg(test)]
mod tests;

#[derive(Clone)]
pub(crate) struct Source {
    owner: Owner,
    target: Target,
    socket: u64,
    row: Arc<MessageVm>,
    occurrences: Arc<Vec<(String, Value)>>,
}

#[derive(Clone)]
pub(crate) struct Token {
    source: Source,
    ordinal: usize,
    target: Arc<FileTarget>,
}

impl Source {
    pub(crate) fn cold_occurrence(
        &self,
        shell: &ShellState,
        team: &str,
        raw: &Value,
        ordinal: usize,
    ) -> Option<file_acquisition::Demand> {
        if !self.current(shell) || !self.matches_occurrence(team, raw, ordinal) {
            return None;
        }
        file_acquisition::Demand::capture(shell, self.clone(), team, raw, ordinal)
    }

    pub(super) fn cold_demands(&self, shell: &ShellState) -> Vec<file_acquisition::Demand> {
        self.occurrences
            .iter()
            .enumerate()
            .filter_map(|(index, (team, raw))| self.cold_occurrence(shell, team, raw, index))
            .collect()
    }
    pub(super) fn same_source(&self, other: &Self) -> bool {
        self.owner.serial == other.owner.serial
            && Arc::ptr_eq(&self.owner.scope.identity, &other.owner.scope.identity)
            && self.target == other.target
            && self.socket == other.socket
            && same_body(&self.row, &other.row)
    }
    pub(super) fn context(&self, shell: &ShellState) -> Option<crate::scheduled::Context> {
        self.current(shell)
            .then(|| self.owner.scope.request(shell))
            .flatten()
    }
    pub(super) fn receipt_scope(&self) -> Scope {
        self.owner.scope.receipt_only()
    }
    pub(crate) fn capture(shell: &ShellState, generation: u64, row: &MessageVm) -> Option<Self> {
        let e = shell.slack_lists.comments.entry.as_ref()?;
        if e.owner.serial != generation
            || !e.owner.active(shell)
            || row.ts.is_empty()
            || e.target.root_ts() == Some(row.ts.as_str())
            || e.deleted.contains(&row.ts)
            || e.rows.iter().filter(|r| same_body(r, row)).count() != 1
        {
            return None;
        }
        let mut occurrences = Vec::new();
        collect(&row.body, &mut occurrences);
        if occurrences.is_empty() {
            return None;
        }
        let source = Self {
            owner: e.owner.clone(),
            target: e.target.clone(),
            socket: shell
                .core
                .workspaces
                .get(&e.owner.scope.team)?
                .rt_generation,
            row: Arc::new(row.clone()),
            occurrences: Arc::new(occurrences),
        };
        source.current(shell).then_some(source)
    }

    pub(crate) fn current(&self, shell: &ShellState) -> bool {
        self.owner.active(shell)
            && shell
                .core
                .workspaces
                .get(&self.owner.scope.team)
                .is_some_and(|w| w.rt_generation == self.socket)
            && shell.slack_lists.comments.entry.as_ref().is_some_and(|e| {
                e.target == self.target
                    && !e.deleted.contains(&self.row.ts)
                    && e.page
                        .as_ref()
                        .is_some_and(|p| !accepted::deleted(p, &self.row.ts))
                    && e.rows.iter().filter(|r| same_body(r, &self.row)).count() == 1
            })
    }

    /// The renderer supplies only an occurrence index into the original body.
    /// The privately retained pair and current canonical file supply authority.
    pub(crate) fn occurrence(
        &self,
        shell: &ShellState,
        team: &str,
        raw: &Value,
        ordinal: usize,
    ) -> Option<Token> {
        if !self.current(shell) || !self.matches_occurrence(team, raw, ordinal) {
            return None;
        }
        let target = FileTarget::capture(shell, team, raw).ok()?;
        Some(Token {
            source: self.clone(),
            ordinal,
            target: Arc::new(target),
        })
    }
    fn matches_occurrence(&self, team: &str, raw: &Value, ordinal: usize) -> bool {
        team == self.owner.scope.team
            && self
                .occurrences
                .get(ordinal)
                .is_some_and(|(t, r)| t == team && r == raw)
    }
    fn refused(&self, shell: &mut ShellState) {
        if !self.owner.active(shell)
            || shell
                .slack_lists
                .comments
                .entry
                .as_ref()
                .is_none_or(|e| e.target != self.target)
        {
            return;
        }
        let message = "This file could not be opened. Your draft is retained.";
        if let Some(binding) = shell
            .slack_lists
            .comments
            .drafts
            .active
            .clone()
            .filter(|b| b.owner.serial == self.owner.serial && b.current(shell))
        {
            if let Some(e) = draft::entry_mut(shell, binding.id()) {
                e.error.get_or_insert_with(|| message.into());
            }
        } else if let Some(e) = shell.slack_lists.comments.entry.as_mut() {
            e.error.get_or_insert_with(|| message.into());
        }
    }
}
impl Token {
    pub(crate) fn current(&self, shell: &ShellState) -> bool {
        self.source.current(shell)
            && self.target.current(shell)
            && self.source.matches_occurrence(
                &self.source.owner.scope.team,
                self.target.raw(),
                self.ordinal,
            )
    }
}

pub(crate) fn open(mut state: Signal<ShellState>, token: Token) {
    if own_edit::blocks_departure(&state.peek(), token.source.owner.serial) {
        return;
    }
    if !token.current(&state.peek()) {
        token.source.refused(&mut state.write());
        return;
    }
    dioxus::core::spawn_forever(async move {
        let Ok(departure) = draft::guarded_departure(state, token.source.owner.serial, |shell| {
            token.current(shell)
        })
        .await
        else {
            token.source.refused(&mut state.write());
            return;
        };
        // These checks, the existing native opener, and committed() are one
        // synchronous turn. No unlocked frame or await can admit another draft.
        if !token.current(&state.peek()) || !departure.current(&state.peek()) {
            token.source.refused(&mut state.write());
            return;
        }
        let file = token.target.file();
        let Some(id) = file.id.clone() else {
            return;
        };
        let list = super_platinum_core::slack_lists::is_list(&file);
        let old_visit = state.peek().slack_lists.view.as_ref().map(|v| v.generation);
        if list {
            crate::slack_lists::open(state, file);
        } else {
            crate::canvas::open(state, file);
        }
        let committed = {
            let shell = state.peek();
            if list {
                shell.overlay == Some(Overlay::SlackList)
                    && shell
                        .slack_lists
                        .view
                        .as_ref()
                        .is_some_and(|v| v.id == id && Some(v.generation) != old_visit)
            } else {
                shell.overlay == Some(Overlay::Canvas)
                    && shell.canvas.as_ref().is_some_and(|v| v.id == id)
            }
        };
        if committed {
            departure.committed(&mut state.write());
        } else {
            token.source.refused(&mut state.write());
        }
        // Failed opener: dropping departure releases the exact original editor.
    });
}

fn same_body(a: &MessageVm, b: &MessageVm) -> bool {
    a.id == b.id
        && a.ts == b.ts
        && a.thread_ts == b.thread_ts
        && a.user_id == b.user_id
        && a.body == b.body
}

fn collect(nodes: &[RichNode], output: &mut Vec<(String, Value)>) {
    for node in nodes {
        match node {
            RichNode::FileMention { team, raw, .. } => output.push((team.clone(), raw.clone())),
            RichNode::Paragraph(c)
            | RichNode::Quote(c)
            | RichNode::Header(c)
            | RichNode::Context(c)
            | RichNode::Actions(c) => collect(c, output),
            RichNode::Section {
                text,
                fields,
                accessory,
            } => {
                collect(text, output);
                for field in fields {
                    collect(field, output);
                }
                if let Some(node) = accessory {
                    collect(std::slice::from_ref(node.as_ref()), output);
                }
            }
            RichNode::List { items, .. } => {
                for item in items {
                    collect(item, output);
                }
            }
            _ => {}
        }
    }
}
