//! Read-only accepted team facts. This grants no picker or mutation authority.
use super::*;

#[derive(Clone)]
pub(crate) struct Observation {
    context: Arc<Context>,
    token: Option<Arc<()>>,
    rows: BTreeMap<String, Value>,
}
impl Observation {
    pub(crate) fn capture(shell: &ShellState, team: &str) -> Result<Self, Error> {
        let context = Arc::new(Context::capture(shell, team)?);
        let slot = shell.app_home.channel_fetches.slots.get(&context.key());
        if slot.is_some_and(|s| !s.context.current(shell)) {
            return Err(unavailable());
        }
        Ok(Self {
            context,
            token: slot.map(|s| s.token.clone()),
            rows: slot.map(|s| s.payload.teams.clone()).unwrap_or_default(),
        })
    }
    pub(crate) fn rows(&self) -> &BTreeMap<String, Value> {
        &self.rows
    }
    pub(crate) fn current(&self, shell: &ShellState) -> bool {
        if !self.context.current(shell) {
            return false;
        }
        match (
            &self.token,
            shell
                .app_home
                .channel_fetches
                .slots
                .get(&self.context.key()),
        ) {
            (None, None) => true,
            (Some(token), Some(slot)) => {
                Arc::ptr_eq(token, &slot.token)
                    && slot.context.current(shell)
                    && slot.payload.teams == self.rows
            }
            _ => false,
        }
    }
}
