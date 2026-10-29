//! First-only dialog cache origins; no readiness or posting authority is created.
use super::*;
const LIMIT: usize = 4096;
const NAME_BYTES: usize = 4096;
#[derive(Clone, Default)]
pub(super) struct Origins {
    entries: BTreeMap<String, Option<String>>,
}
pub(super) struct Registration(Vec<(String, Option<String>)>);
impl Origins {
    pub(super) fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    pub(super) fn guarded(&self, id: &str) -> bool {
        self.entries.contains_key(id)
    }
    pub(super) fn ids(&self) -> BTreeSet<String> {
        self.entries.keys().cloned().collect()
    }
    pub(super) fn independent(&self, id: &str, name: &str) -> bool {
        self.entries
            .get(id)
            .and_then(Option::as_ref)
            .is_some_and(|old| old == name)
    }
    pub(super) fn propose(
        &self,
        s: &ShellState,
        ctx: &Context,
        ids: &[String],
    ) -> Result<Registration, Error> {
        if ids.iter().any(|id| !valid_id(id)) {
            return Err(unavailable());
        }
        let fresh: BTreeSet<_> = ids
            .iter()
            .filter(|id| !self.entries.contains_key(*id))
            .collect();
        if self
            .entries
            .len()
            .checked_add(fresh.len())
            .is_none_or(|n| n > LIMIT)
        {
            return Err(unavailable());
        }
        let w = s
            .core
            .workspaces
            .get(&ctx.session.team_id)
            .ok_or_else(unavailable)?;
        Ok(Registration(
            fresh
                .into_iter()
                .map(|id| {
                    let name = w
                        .channels
                        .get(id)
                        .filter(|v| v.id == *id && channel_base(s, ctx, v))
                        .map(|v| load::share_channel_name(s, ctx, v))
                        .filter(|v| v.len() <= NAME_BYTES);
                    (id.clone(), name)
                })
                .collect(),
        ))
    }
    #[cfg(test)]
    pub(super) fn without(&self, id: &str) -> Self {
        let mut result = self.clone();
        result.entries.remove(id);
        result
    }
    pub(super) fn commit(&mut self, registration: Registration) {
        for (id, name) in registration.0 {
            self.entries.entry(id).or_insert(name);
        }
    }
}
