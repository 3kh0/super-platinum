//! Process-wide complements to the durable journal. No lock spans an await.
use super::Target;
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::{Mutex, OnceLock},
};
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct Key(String, String, String, String, Option<PathBuf>);
impl From<&Target> for Key {
    fn from(t: &Target) -> Self {
        Self(
            t.account.clone(),
            t.team.clone(),
            t.user.clone(),
            t.id.clone(),
            t.path.clone(),
        )
    }
}
#[derive(Default)]
struct State {
    active: HashSet<Key>,
    deadlines: HashMap<Key, u64>,
}
fn state() -> &'static Mutex<State> {
    static STATE: OnceLock<Mutex<State>> = OnceLock::new();
    STATE.get_or_init(Mutex::default)
}
pub(super) struct Active(Key);
impl Active {
    pub fn claim(target: &Target) -> Result<Self, String> {
        let key = Key::from(target);
        if !state().lock().unwrap().active.insert(key.clone()) {
            return Err("Another change or check for this file is still in progress.".into());
        }
        Ok(Self(key))
    }
}
impl Drop for Active {
    fn drop(&mut self) {
        state().lock().unwrap().active.remove(&self.0);
    }
}
pub(super) fn remember(target: &Target, until: u64) {
    let mut state = state().lock().unwrap();
    let deadline = state.deadlines.entry(target.into()).or_default();
    *deadline = (*deadline).max(until);
}
pub(super) fn deadline(target: &Target, now: u64) -> Option<u64> {
    let mut state = state().lock().unwrap();
    let key = Key::from(target);
    let until = *state.deadlines.get(&key)?;
    if until <= now {
        state.deadlines.remove(&key);
        None
    } else {
        Some(until)
    }
}
pub(super) fn persisted(target: &Target, until: u64) {
    let mut state = state().lock().unwrap();
    let key = Key::from(target);
    if state
        .deadlines
        .get(&key)
        .is_some_and(|current| *current <= until)
    {
        state.deadlines.remove(&key);
    }
}
