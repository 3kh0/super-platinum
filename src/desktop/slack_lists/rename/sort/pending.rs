//! Reserve synchronously before renderer work, so Save cannot admit an older vector.
use super::*;
use std::time::{Duration, Instant};
pub(crate) struct Pending {
    key: Key,
    serial: u64,
    visit: u64,
    choice: u64,
    rendered: ViewSortChoice,
    token: u64,
    deadline: Instant,
}
pub(crate) fn reserve(
    shell: &ShellState,
    serial: u64,
    visit: u64,
    choice: u64,
    rendered: &ViewSortChoice,
) -> Option<Pending> {
    let key = Key::current(shell)?;
    let token = next_id();
    edit(shell, serial, visit, choice, rendered, |e| {
        e.sort_pending = Some(token);
        true
    })
    .then(|| Pending {
        key,
        serial,
        visit,
        choice,
        rendered: rendered.clone(),
        token,
        deadline: Instant::now() + Duration::from_secs(1),
    })
}
impl Pending {
    pub(crate) fn token(&self) -> u64 {
        self.token
    }
    pub(crate) fn remaining(&self) -> Duration {
        self.deadline.saturating_duration_since(Instant::now())
    }
    pub(crate) fn commit(&self, shell: &ShellState, action: Action) -> Option<(u64, String)> {
        if Instant::now() >= self.deadline || Key::current(shell).as_ref() != Some(&self.key) {
            return None;
        }
        rows::apply(
            shell,
            self.serial,
            self.visit,
            self.choice,
            &self.rendered,
            Some(self.token),
            action,
        )
    }
    pub(crate) fn release(&self) -> bool {
        with_registry(|r| {
            let Some(e) = r
                .entries
                .get_mut(&self.key)
                .filter(|e| e.serial == self.serial && e.sort_pending == Some(self.token))
            else {
                return false;
            };
            e.sort_pending = None;
            true
        })
    }
    #[cfg(test)]
    pub(crate) fn expire_for_test(&mut self) {
        self.deadline = Instant::now();
    }
}
impl Drop for Pending {
    fn drop(&mut self) {
        self.release();
    }
}
