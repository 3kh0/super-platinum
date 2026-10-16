use super::*;
/// Shared credential/capability identity plus the original realtime fence.
#[derive(Clone)]
pub(super) struct Scope {
    pub context: super::super::admission::Context,
    pub generation: u64,
}
impl Scope {
    pub fn capture(shell: &ShellState) -> Option<Self> {
        let context = super::super::admission::Context::capture(shell)?;
        let generation = shell.core.workspaces.get(&context.key.team)?.rt_generation;
        Some(Self {
            context,
            generation,
        })
    }
    pub fn current(&self, shell: &ShellState) -> bool {
        self.context.current(shell)
            && shell
                .core
                .workspaces
                .get(&self.context.key.team)
                .is_some_and(|w| w.rt_generation == self.generation)
    }
}
