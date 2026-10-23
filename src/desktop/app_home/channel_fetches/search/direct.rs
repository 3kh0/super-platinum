//! A single requested conversations.info row enters the same actual queue.
pub(crate) use super::posting::PostingRead;
use super::*;

pub(crate) struct Read(pub(super) super::Read);
impl Read {
    pub(crate) fn capture(
        mut state: Signal<ShellState>,
        team: &str,
        id: &str,
        current: &impl Fn(&ShellState) -> bool,
    ) -> Result<Self, Error> {
        if id.is_empty() || !current(&state.peek()) {
            return Err(unavailable());
        }
        // This optional navigation seam cannot invent source initialization.
        canonical::capture(&mut state.write(), team)?;
        let mut read = super::Read::capture(state, team, current)?;
        read.receipt.origin = Origin::Direct;
        read.receipt.requested = Some(id.to_owned());
        Ok(Self(read))
    }
    pub(crate) fn current(&self, shell: &ShellState) -> bool {
        self.0.current(shell)
    }
    pub(crate) async fn publish(
        self,
        state: Signal<ShellState>,
        raw: Value,
        current: impl Fn(&ShellState) -> bool,
    ) -> Result<Arc<canonical::Canonical>, Error> {
        self.0.publish(state, vec![raw], current).await
    }
}
