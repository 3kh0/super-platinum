use super::*;
use std::collections::BTreeSet;
use super_platinum_core::{
    config::WorkspaceSession, slack::models::User, state::directory_members::Subset,
};
const GENERATION: &str = "canvas-root-attribution";
pub(super) struct Member {
    pub id: String,
    pub raw: Value,
    pub native: User,
    source: Subset,
    account: String,
    session: WorkspaceSession,
    epoch: u64,
}
impl Member {
    /// Existing canonical source permits only a known ordinary member refresh.
    /// It is not name/display authority and never repairs from retained bytes.
    pub(super) fn recoverable(shell: &ShellState, owner: &Owner, id: &str) -> Result<(), Error> {
        if !owner.current(shell) {
            return Err(unavailable());
        }
        let ctx = owner.scope.request(shell).ok_or_else(unavailable)?;
        let account = shell
            .core
            .active_account
            .as_deref()
            .ok_or_else(unavailable)?;
        let workspace = shell
            .core
            .workspaces
            .get(&owner.scope.team)
            .ok_or_else(unavailable)?;
        let source = workspace.directory_members.observations(
            account,
            &ctx.session,
            ctx.epoch,
            GENERATION,
        )?;
        let rows = source.rows(&workspace.directory_members, &[id.to_owned()])?;
        let [row] = rows.as_slice() else {
            return Err(unavailable());
        };
        if !ordinary_member(&row.source, id)
            || ["isUnknown", "isNonExistent"].iter().any(|key| {
                !matches!(
                    row.source.get(*key),
                    None | Some(Value::Null) | Some(Value::Bool(false))
                )
            })
        {
            return Err(unavailable());
        }
        Ok(())
    }
    pub(super) fn capture(shell: &ShellState, owner: &Owner, id: &str) -> Result<Self, Error> {
        if !owner.current(shell) {
            return Err(unavailable());
        }
        let ctx = owner.scope.request(shell).ok_or_else(unavailable)?;
        let account = shell.core.active_account.clone().ok_or_else(unavailable)?;
        let workspace = shell
            .core
            .workspaces
            .get(&owner.scope.team)
            .ok_or_else(unavailable)?;
        let source = workspace.directory_members.subset(
            &account,
            &ctx.session,
            ctx.epoch,
            GENERATION,
            &BTreeSet::from([id.to_owned()]),
            &workspace.users,
        )?;
        let [row] = source.rows() else {
            return Err(unavailable());
        };
        if !ordinary_member(&row.source, id) {
            return Err(unavailable());
        }
        let raw = row.source.clone();
        let native = workspace.users.get(id).cloned().ok_or_else(unavailable)?;
        Ok(Self {
            id: id.into(),
            raw,
            native,
            source,
            account,
            session: ctx.session,
            epoch: ctx.epoch,
        })
    }
    pub(super) fn current(&self, shell: &ShellState) -> bool {
        shell.core.active_account.as_deref() == Some(&self.account)
            && shell.core.account_epoch == self.epoch
            && shell
                .core
                .workspaces
                .get(&self.session.team_id)
                .is_some_and(|w| {
                    self.source.current_rows(
                        &w.directory_members,
                        &self.account,
                        &self.session,
                        self.epoch,
                        GENERATION,
                        &w.users,
                    )
                })
    }
}
fn ordinary_member(raw: &Value, id: &str) -> bool {
    raw["id"].as_str() == Some(id)
        && ["is_bot", "is_app_user"].iter().all(|key| {
            matches!(
                raw.get(*key),
                None | Some(Value::Null) | Some(Value::Bool(false))
            )
        })
        && matches!(raw.get("bot_id"), None | Some(Value::Null))
}
