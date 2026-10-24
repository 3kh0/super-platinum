//! Nested group titles retain the shared history, not only a typed label cache.
use super::*;
use crate::directory_member_fetches::Context;
use std::collections::BTreeSet;
use super_platinum_core::{slack::models::UserGroup, state::directory_usergroups::Snapshot};

#[derive(Clone)]
pub(super) struct Groups {
    owner: Arc<Context>,
    source: Snapshot,
    rows: BTreeMap<String, UserGroup>,
}
impl Groups {
    pub(super) fn capture(
        shell: &ShellState,
        token: &Token,
        ids: &BTreeSet<String>,
    ) -> Result<Option<Self>, Error> {
        if ids.is_empty() {
            return Ok(None);
        }
        if !token.current(shell) {
            return Err(unavailable());
        }
        let owner = Arc::new(Context::capture(shell, &token.owner.team)?);
        let workspace = shell
            .core
            .workspaces
            .get(&token.owner.team)
            .ok_or_else(unavailable)?;
        let source = workspace
            .directory_usergroups
            .snapshot(
                &owner.account,
                &owner.session,
                owner.epoch,
                &workspace.usergroups,
            )
            .map_err(|_| unavailable())?;
        let mut rows = BTreeMap::new();
        for id in ids {
            let group = workspace.usergroups.get(id).ok_or_else(unavailable)?;
            let observed = source
                .rows()
                .iter()
                .find(|r| r.source["id"].as_str() == Some(id.as_str()))
                .ok_or_else(unavailable)?;
            if observed.effective["id"].as_str() != Some(id.as_str())
                || group.label_source.as_ref() != Some(&observed.source)
            {
                return Err(unavailable());
            }
            file_titles::known_group_label(id, group).map_err(|_| unavailable())?;
            rows.insert(id.clone(), group.clone());
        }
        Ok(Some(Self {
            owner,
            source,
            rows,
        }))
    }
    pub(super) fn hit(&self, id: &str) -> opening::Hit {
        opening::Hit::Group {
            owner: self.owner.clone(),
            id: id.into(),
        }
    }
    pub(super) fn current(&self, shell: &ShellState, token: &Token) -> bool {
        token.current(shell)
            && self.owner.current(shell)
            && shell
                .core
                .workspaces
                .get(&self.owner.session.team_id)
                .is_some_and(|w| {
                    self.source.current(
                        &w.directory_usergroups,
                        &self.owner.account,
                        &self.owner.session,
                        self.owner.epoch,
                        &w.usergroups,
                    )
                })
    }
    pub(super) fn label(&self, id: &str) -> Result<String, file_titles::Unavailable> {
        file_titles::known_group_label(
            id,
            self.rows
                .get(id)
                .ok_or(file_titles::Unavailable::InvalidMetadata)?,
        )
    }
    pub(super) fn snapshot(&self) -> Value {
        serde_json::json!({"ids":self.rows.keys().collect::<Vec<_>>(),
            "rows":self.source.rows().iter().filter(|r|r.source["id"].as_str().is_some_and(|id|self.rows.contains_key(id))).collect::<Vec<_>>(),
            "labels":self.rows.iter().map(|(id,g)|(id,file_titles::known_group_label(id,g).ok())).collect::<BTreeMap<_,_>>(),
            "provenance":"shared-group-history-snapshot"})
    }
}
