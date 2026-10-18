//! A consumer follows the account-owned queue across sockets. Each issued
//! operation still checks its own exact socket before it can publish anything.
use super::*;
use tokio::sync::watch;

struct Observer {
    context: Context,
    identity: Arc<()>,
    changes: watch::Receiver<u64>,
}
impl Observer {
    fn capture(shell: &ShellState, team: &str) -> Result<Option<Self>, Error> {
        let context = Context::capture(shell, team)?;
        mode::observed(shell, team)?;
        let Some(runtime) = shell
            .directory_member_fetches
            .automatic
            .slots
            .get(&context.key())
        else {
            return Ok(None);
        };
        if !runtime.context.account_current(shell) {
            return Err(unavailable());
        }
        Ok(Some(Self {
            context,
            identity: runtime.identity.clone(),
            changes: runtime.changed.subscribe(),
        }))
    }
    fn pending(&self, shell: &ShellState, ids: &[String]) -> Result<bool, Error> {
        if !self.context.account_current(shell) {
            return Err(unavailable());
        }
        mode::observed(shell, &self.context.session.team_id)?;
        let runtime = shell
            .directory_member_fetches
            .automatic
            .slots
            .get(&self.context.key())
            .filter(|runtime| Arc::ptr_eq(&runtime.identity, &self.identity))
            .ok_or_else(unavailable)?;
        // A disconnected worker can return its IDs then stop. Those pending
        // IDs are not completion; only accepted reconnect/resume drives them.
        Ok(runtime.manager.has_work_for(ids))
    }
}

pub(crate) async fn wait(
    state: Signal<ShellState>,
    team: &str,
    ids: &[String],
    current: impl Fn(&ShellState) -> bool,
) -> Result<(), Error> {
    let mut observer = {
        let shell = state.peek();
        if !current(&shell) {
            return Err(unavailable());
        }
        let Some(observer) = Observer::capture(&shell, team)? else {
            return Ok(());
        };
        observer
    };
    loop {
        {
            let shell = state.peek();
            if !current(&shell) {
                return Err(unavailable());
            }
            if !observer.pending(&shell, ids)? {
                return Ok(());
            }
        }
        observer
            .changes
            .changed()
            .await
            .map_err(|_| unavailable())?;
    }
}

#[cfg(test)]
mod tests;
