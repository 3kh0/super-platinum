//! Same-team file display facts without constructing an editor token.
use super::*;

/// Inert original title/source observation, not route or permission authority.
/// The caller retains its own occurrence and applies its own display contract.
#[derive(Clone)]
pub(crate) struct FileObservation {
    team: String,
    id: String,
    snapshot: Arc<Snapshot>,
    title: source::Observation,
}
impl FileObservation {
    /// Read-only: absent/unavailable metadata does not initialize or fetch.
    pub(crate) fn capture(shell: &ShellState, team: &str, id: &str) -> Result<Self, Error> {
        if shell.core.active_team.as_deref() != Some(team) {
            return Err(unavailable());
        }
        let snapshot = Snapshot::capture(shell, team)?;
        let candidate = snapshot
            .rows()
            .iter()
            .find(|row| row.id() == id)
            .ok_or_else(unavailable)?;
        let title = snapshot.title(id)?;
        if title.source() != candidate.source() {
            return Err(unavailable());
        }
        let observed = Self {
            team: team.into(),
            id: id.into(),
            snapshot,
            title,
        };
        observed
            .current(shell)
            .then_some(observed)
            .ok_or_else(unavailable)
    }
    pub(crate) fn id(&self) -> &str {
        &self.id
    }
    /// Exact original raw file, not a complete processed File assertion.
    pub(crate) fn source(&self) -> &Value {
        self.title.source()
    }
    /// Supported title/status projection only. No caller-specific text decoding.
    pub(crate) fn effective(&self) -> &Value {
        self.title.effective()
    }
    /// Original accepted carrier; singleton callers retain the files.info envelope.
    pub(crate) fn receipt(&self) -> &Value {
        self.title.receipt()
    }
    /// Preserve the existing full collection, account, socket and event fences.
    /// After another publication consumers recapture; they do not refetch solely
    /// because this original observation retired.
    pub(crate) fn current(&self, shell: &ShellState) -> bool {
        shell.core.active_team.as_deref() == Some(self.team.as_str())
            && self.snapshot.current(shell)
    }
}

#[cfg(test)]
mod tests;
