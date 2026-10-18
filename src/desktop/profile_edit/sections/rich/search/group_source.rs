//! Current effective group history, independently guarded from a Profile query.
//! Typed/legacy rows never manufacture a source record or renderer projection.
use crate::{directory_member_fetches::Context, state::ShellState};
use serde_json::Value;
use std::{collections::BTreeMap, sync::Arc};
use super_platinum_core::{
    slack::{Error, api::custom::texty::groups},
    state::directory_usergroups as source_core,
};

fn unavailable() -> Error {
    Error::Api("profile_group_source_unavailable".into())
}

pub(in crate::profile_edit::sections::rich) struct Snapshot {
    owner: Arc<Context>,
    source: source_core::Snapshot,
    rows: Vec<groups::Snapshot>,
}
impl Snapshot {
    /// Captures only the currently accepted local collection, not an assertion
    /// that every group on the server has been acquired.
    pub(in crate::profile_edit::sections::rich) fn capture(
        shell: &ShellState,
        team: &str,
    ) -> Result<Self, Error> {
        Self::under(shell, Arc::new(Context::capture(shell, team)?))
    }
    /// Publication uses its original owner; it cannot borrow a successor socket.
    pub(super) fn under(shell: &ShellState, owner: Arc<Context>) -> Result<Self, Error> {
        if !owner.current(shell) {
            return Err(unavailable());
        }
        let w = shell
            .core
            .workspaces
            .get(&owner.session.team_id)
            .ok_or_else(unavailable)?;
        let source = w
            .directory_usergroups
            .snapshot(&owner.account, &owner.session, owner.epoch, &w.usergroups)
            .map_err(|_| unavailable())?;
        let rows = source
            .rows()
            .iter()
            .map(|row| groups::Snapshot::capture(row.source.clone(), row.effective.clone()))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            owner,
            source,
            rows,
        })
    }
    pub(in crate::profile_edit::sections::rich) fn rows(&self) -> &[groups::Snapshot] {
        &self.rows
    }
    pub(in crate::profile_edit::sections::rich) fn membership(&self) -> Option<&[String]> {
        self.source.membership()
    }
    pub(in crate::profile_edit::sections::rich) fn current(&self, shell: &ShellState) -> bool {
        let owner = &self.owner;
        owner.current(shell)
            && shell
                .core
                .workspaces
                .get(&owner.session.team_id)
                .is_some_and(|w| {
                    self.source.current(
                        &w.directory_usergroups,
                        &owner.account,
                        &owner.session,
                        owner.epoch,
                        &w.usergroups,
                    )
                })
    }
    /// D8Wa reselects each original response ID after its complete ordered sink.
    /// Retain duplicate occurrences and order; same-version rows resolve to the
    /// accepted predecessor rather than to the unadopted incoming raw payload.
    pub(in crate::profile_edit::sections::rich) fn reselect(
        &self,
        rows: &[Value],
    ) -> Result<Vec<groups::Snapshot>, Error> {
        let by_id: BTreeMap<_, _> = self.rows.iter().map(|row| (row.id(), row)).collect();
        let mut selected = Vec::with_capacity(rows.len());
        for row in rows {
            let id = row["id"].as_str().ok_or_else(unavailable)?;
            // D8Wa filters IDs without a resulting record; it does not mint an
            // unknown row. Existing IDs preserve their response occurrences.
            if let Some(row) = by_id.get(id) {
                selected.push((**row).clone());
            }
        }
        Ok(selected)
    }
}
