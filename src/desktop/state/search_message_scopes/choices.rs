//! Known native cache choices, never a remote/provider/permission resolution.
use super::*;
use std::collections::{BTreeMap, HashSet};
#[derive(Clone, PartialEq, Eq)]
pub(super) struct Entry {
    pub choice: ScopeChoice,
    pub username: String,
}
impl Entry {
    fn matches(&self, query: &str) -> bool {
        let query = query.trim_start_matches(['#', '@']).to_lowercase();
        self.choice.label().to_lowercase().contains(&query)
            || self.username.to_lowercase().contains(&query)
            || self.choice.id().to_lowercase().contains(&query)
    }
}
#[derive(Clone, Default)]
pub(super) struct Bank {
    entries: [BTreeMap<String, Entry>; 2],
}
fn index(kind: Scope) -> usize {
    match kind {
        Scope::Channels => 0,
        Scope::Authors => 1,
    }
}
fn projected(s: &ShellState) -> HashSet<&str> {
    s.channels
        .iter()
        .filter(|c| !c.is_im && !c.is_mpim)
        .map(|c| c.id.as_str())
        .collect()
}
fn current_entry(s: &ShellState, kind: Scope, id: &str, visible: &HashSet<&str>) -> Option<Entry> {
    let workspace = s.core.workspaces.get(s.core.active_team.as_ref()?)?;
    match kind {
        Scope::Channels => {
            let c = workspace.channels.get(id)?;
            if c.id != id
                || !visible.contains(id)
                || c.is_im
                || c.is_mpim
                || c.is_archived && s.core.active_channel.as_deref() != Some(id)
            {
                return None;
            }
            Some(Entry {
                choice: ScopeChoice::channel(c.id.clone(), c.name.clone().unwrap_or_default())?,
                username: String::new(),
            })
        }
        Scope::Authors => {
            let u = workspace.users.get(id)?;
            if u.id != id || u.deleted {
                return None;
            }
            Some(Entry {
                choice: ScopeChoice::author(
                    u.id.clone(),
                    super_platinum_core::state::display_name(Some(u), id),
                )?,
                username: u.name.clone().unwrap_or_default(),
            })
        }
    }
}
impl Bank {
    pub(super) fn capture(s: &ShellState) -> Option<Self> {
        let workspace = s.core.workspaces.get(s.core.active_team.as_ref()?)?;
        let visible = projected(s);
        let mut bank = Self::default();
        for (kind, ids) in [
            (
                Scope::Channels,
                workspace.channels.keys().collect::<Vec<_>>(),
            ),
            (Scope::Authors, workspace.users.keys().collect::<Vec<_>>()),
        ] {
            for id in ids {
                if let Some(entry) = current_entry(s, kind, id, &visible) {
                    bank.entries[index(kind)].insert(id.clone(), entry);
                }
            }
        }
        Some(bank)
    }
    pub(super) fn any(s: &ShellState) -> bool {
        let Some(team) = s.core.active_team.as_ref() else {
            return false;
        };
        let Some(w) = s.core.workspaces.get(team) else {
            return false;
        };
        let visible = projected(s);
        w.users
            .keys()
            .any(|id| current_entry(s, Scope::Authors, id, &visible).is_some())
            || w.channels
                .keys()
                .any(|id| current_entry(s, Scope::Channels, id, &visible).is_some())
    }
    pub(super) fn choices(&self) -> Vec<ScopeChoice> {
        self.entries
            .iter()
            .flat_map(|m| m.values().map(|e| e.choice.clone()))
            .collect()
    }
    pub(super) fn get(&self, kind: Scope, id: &str) -> Option<&Entry> {
        self.entries[index(kind)].get(id)
    }
    pub(super) fn entry_current(&self, s: &ShellState, kind: Scope, id: &str) -> bool {
        let visible = projected(s);
        self.get(kind, id)
            .is_some_and(|entry| current_entry(s, kind, id, &visible).as_ref() == Some(entry))
    }
    pub(super) fn relevant_current(&self, s: &ShellState, relevant: &[(Scope, String)]) -> bool {
        let visible = projected(s);
        relevant.iter().all(|(kind, id)| {
            self.get(*kind, id)
                .is_some_and(|entry| current_entry(s, *kind, id, &visible).as_ref() == Some(entry))
        })
    }
    pub(super) fn window(
        &self,
        s: &ShellState,
        edit: &ScopeEdit,
        kind: Scope,
        query: &str,
    ) -> Vec<Row> {
        let visible = projected(s);
        let mut rows = self.entries[index(kind)]
            .values()
            .filter(|e| e.matches(query))
            .collect::<Vec<_>>();
        rows.sort_by(|a, b| {
            edit.selected(kind, b.choice.id())
                .cmp(&edit.selected(kind, a.choice.id()))
                .then_with(|| a.choice.label().cmp(b.choice.label()))
                .then_with(|| a.choice.id().cmp(b.choice.id()))
        });
        rows.into_iter()
            .take(30)
            .map(|e| Row {
                id: e.choice.id().into(),
                label: e.choice.label().into(),
                username: e.username.clone(),
                selected: edit.selected(kind, e.choice.id()),
                current: current_entry(s, kind, e.choice.id(), &visible).as_ref() == Some(e),
            })
            .collect()
    }
}
