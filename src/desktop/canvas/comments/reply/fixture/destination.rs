//! Read-only custody evidence captured on the original fixture thread. This
//! cannot bind an editor or authorize a destination read/write.
use super::*;

pub(in crate::canvas) struct DestinationWitness {
    id: String,
    disk: Disk,
    root: String,
    epoch: u64,
    completion: Option<std::sync::Arc<tokio::sync::Notify>>,
}

impl DestinationWitness {
    pub(in crate::canvas) fn capture(state: Signal<ShellState>) -> Result<Self, String> {
        let shell = state.try_read().map_err(|_| "Original shell disposed")?;
        if !super::super::super::fixture::canonical_files::destinations_requested()
            || !super::super::super::fixture::canonical_files::enabled(&shell)
        {
            return Err("Original destination fixture missing".into());
        }
        let private = path(&shell).ok_or("Original private reply path missing")?;
        let e = shell
            .canvas_checklist
            .comment_replies
            .entries
            .iter()
            .find(|e| {
                e.disk.account == ACCOUNT
                    && e.disk.scope.file == "Ffixture"
                    && e.root == TS
                    && e.disk.path.as_ref() == Some(&private)
            })
            .ok_or("Original reply custody missing")?;
        Ok(Self {
            id: e.id.clone(),
            disk: e.disk.clone(),
            root: e.root.clone(),
            epoch: shell.core.account_epoch,
            completion: e.native.as_ref().map(native::Mounted::fixture_completion),
        })
    }

    fn current<'a>(&self, shell: &'a ShellState) -> Result<&'a Entry, String> {
        if !super::super::super::fixture::canonical_files::destinations_requested()
            || shell.core.session.is_some()
            || shell.core.transport.is_some()
            || shell.core.active_account.as_deref() != Some(self.disk.account.as_str())
            || shell.core.account_epoch != self.epoch
            || shell.core.active_team.as_deref() != Some(self.disk.scope.workspace.as_str())
            || shell.self_account.user_id != self.disk.scope.user
            || self.disk.path.is_none()
        {
            return Err("Original reply witness retired".into());
        }
        entry(shell, &self.id)
            .filter(|e| e.disk == self.disk && e.root == self.root)
            .ok_or_else(|| "Original reply entry replaced".into())
    }

    pub(in crate::canvas) fn runtime(&self, state: Signal<ShellState>) -> Result<Value, String> {
        let shell = state.try_read().map_err(|_| "Original shell disposed")?;
        let e = self.current(&shell)?;
        Ok(
            json!({"text":e.text,"blocks":e.blocks,"capture":e.capture,"serial":e.serial,
            "saving":e.saving,"loading":e.loading,"sending":e.sending,"error":e.error,
            "nativePresent":e.native.is_some()}),
        )
    }

    pub(in crate::canvas) fn private_path(
        &self,
        state: Signal<ShellState>,
    ) -> Result<std::path::PathBuf, String> {
        let shell = state.try_read().map_err(|_| "Original shell disposed")?;
        self.current(&shell)?;
        self.disk
            .path
            .clone()
            .ok_or_else(|| "Original private reply path missing".into())
    }

    pub(in crate::canvas) fn late_runtime(
        &self,
        state: Signal<ShellState>,
    ) -> Result<Value, String> {
        let mut value = self.runtime(state)?;
        let shell = state.try_read().map_err(|_| "Original shell disposed")?;
        let e = self.current(&shell)?;
        value["nativeBusy"] = json!(e.native_busy);
        value["owner"] = e
            .native
            .as_ref()
            .map(|n| n.fixture_owner(self.completion.as_ref()))
            .unwrap_or(Value::Null);
        Ok(value)
    }

    pub(in crate::canvas) async fn snapshot(
        &self,
        state: Signal<ShellState>,
    ) -> Result<Value, String> {
        let runtime = self.runtime(state)?;
        let scope = self.disk.scope.clone();
        let root = self.root.clone();
        let journal = self
            .disk
            .work(move |j| {
                let inventory = j.reply_inventory(&scope, &root)?;
                let history = j.reply_history(&scope, None)?;
                Ok(
                    json!({"drafts":inventory.drafts(),"attempt":inventory.attempt(),
                "history":history.iter().map(|h|json!({"receipt":h.receipt,
                    "status":h.status,"observed":h.observed})).collect::<Vec<_>>()}),
                )
            })
            .await
            .map_err(|_| "Original private reply journal read failed")?;
        {
            let shell = state.try_read().map_err(|_| "Original shell disposed")?;
            self.current(&shell)?;
        }
        Ok(json!({"runtime":runtime,"journal":journal}))
    }
}
