//! Session-owned completion facts and cancellation-safe visible metadata reads.
use super::channel_metadata::Owner;
use crate::state::ShellState;
use dioxus::prelude::{ReadableExt, Signal, WritableExt};
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
};
use super_platinum_core::CoreAppState;
mod emojis;
mod users;
pub(super) use emojis::hydrate as hydrate_emojis;
pub(super) use users::hydrate as hydrate_users;

#[derive(Default)]
pub(crate) struct Hydration {
    scopes: HashMap<String, Arc<Scope>>,
}
struct Scope {
    owner: Arc<Owner>,
    claims: Mutex<Claims>,
}
#[derive(Default)]
struct Claims {
    pending: HashMap<Key, Arc<()>>,
    completed: HashSet<Key>,
}
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Kind {
    User,
    Emoji,
}
type Key = (Kind, String);
struct Lease {
    scope: Arc<Scope>,
    ticket: Arc<()>,
    kind: Kind,
    ids: Vec<String>,
}
impl Lease {
    fn reserve(
        state: &mut Signal<ShellState>,
        owner: Arc<Owner>,
        team: &str,
        kind: Kind,
        mut ids: Vec<String>,
    ) -> Option<Self> {
        let mut shell = state.write();
        if !owner.current(&shell.core) {
            return None;
        }
        let scope = shell
            .surface_hydration
            .scopes
            .get(team)
            .filter(|scope| scope.owner.current(&shell.core))
            .cloned()
            .unwrap_or_else(|| {
                Arc::new(Scope {
                    owner,
                    claims: Mutex::new(Claims::default()),
                })
            });
        shell
            .surface_hydration
            .scopes
            .insert(team.to_owned(), scope.clone());
        let ticket = Arc::new(());
        ids.sort();
        ids.dedup();
        {
            let mut claims = scope.claims.lock().unwrap();
            ids.retain(|id| {
                let key = (kind, id.clone());
                if claims.completed.contains(&key) || claims.pending.contains_key(&key) {
                    return false;
                }
                claims.pending.insert(key, ticket.clone());
                true
            });
        }
        (!ids.is_empty()).then_some(Self {
            scope,
            ticket,
            kind,
            ids,
        })
    }
    fn current(&self, core: &CoreAppState) -> bool {
        self.scope.owner.current(core)
    }
    fn complete(&self, core: &CoreAppState, ids: &[String]) {
        if !self.current(core) {
            return;
        }
        let mut claims = self.scope.claims.lock().unwrap();
        for id in ids {
            let key = (self.kind, id.clone());
            if claims
                .pending
                .get(&key)
                .is_some_and(|ticket| Arc::ptr_eq(ticket, &self.ticket))
            {
                claims.pending.remove(&key);
                claims.completed.insert(key);
            }
        }
    }
}
impl Drop for Lease {
    fn drop(&mut self) {
        // Retain the original scope directly: an old error/drop cannot release
        // replacement-account reservations, even when the team and IDs match.
        let mut claims = self.scope.claims.lock().unwrap();
        for id in &self.ids {
            let key = (self.kind, id.clone());
            if claims
                .pending
                .get(&key)
                .is_some_and(|ticket| Arc::ptr_eq(ticket, &self.ticket))
            {
                claims.pending.remove(&key);
            }
        }
    }
}
#[cfg(test)]
mod tests;
