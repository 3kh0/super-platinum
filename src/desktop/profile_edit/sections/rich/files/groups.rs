//! Canonical effective group labels plus an independently observed self policy.
use super::*;
use crate::{directory_member_fetches::Context, usergroups::unknown};
use std::collections::BTreeSet;
use super_platinum_core::{
    slack::models::User,
    state::{directory_members::Subset, directory_usergroups::Snapshot},
};
const GENERATION: &str = "profile-title-group-self";
#[derive(Clone)]
pub(super) struct Policy {
    owner: Arc<Context>,
    source: Arc<Subset>,
    restricted: bool,
}
impl Policy {
    pub(super) fn capture(shell: &ShellState, token: &Token) -> Result<Self, Error> {
        if !token.current(shell) {
            return Err(unavailable());
        }
        let owner = Arc::new(Context::capture(shell, &token.owner.team)?);
        let w = shell
            .core
            .workspaces
            .get(&owner.session.team_id)
            .ok_or_else(unavailable)?;
        let source = w.directory_members.subset(
            &owner.account,
            &owner.session,
            owner.epoch,
            GENERATION,
            &BTreeSet::from([owner.session.user_id.clone()]),
            &w.users,
        )?;
        let [row] = source.rows() else {
            return Err(unavailable());
        };
        if row.source["id"].as_str() != Some(owner.session.user_id.as_str())
            || truth(&row.source["isUnknown"])
            || truth(&row.source["isNonExistent"])
        {
            return Err(unavailable());
        }
        let user: User = serde_json::from_value(row.source.clone()).map_err(|_| unavailable())?;
        Ok(Self {
            owner,
            source: Arc::new(source),
            restricted: user.is_restricted,
        })
    }
    pub(super) fn restricted(&self) -> bool {
        self.restricted
    }
    fn current(&self, shell: &ShellState) -> bool {
        self.owner.current(shell)
            && shell
                .core
                .workspaces
                .get(&self.owner.session.team_id)
                .is_some_and(|w| {
                    self.source.current_rows(
                        &w.directory_members,
                        &self.owner.account,
                        &self.owner.session,
                        self.owner.epoch,
                        GENERATION,
                        &w.users,
                    )
                })
    }
}
fn truth(v: &Value) -> bool {
    match v {
        Value::Null | Value::Bool(false) => false,
        Value::String(s) => !s.is_empty(),
        Value::Number(n) => n.as_f64().is_some_and(|v| v != 0.0),
        _ => true,
    }
}
#[derive(Clone)]
pub(super) struct Groups {
    policy: Policy,
    source: Option<Snapshot>,
    ids: BTreeSet<String>,
    selected: BTreeMap<String, Arc<unknown::Selection>>,
    presentation: super::super::title_locale::GroupTitle,
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
        let policy = Policy::capture(shell, token)?;
        let presentation = super::super::title_locale::GroupTitle::english();
        let mut selected = BTreeMap::new();
        let mut source = None;
        if !policy.restricted {
            let w = &shell.core.workspaces[&token.owner.team];
            source = Some(
                w.directory_usergroups
                    .snapshot(
                        &policy.owner.account,
                        &policy.owner.session,
                        policy.owner.epoch,
                        &w.usergroups,
                    )
                    .map_err(|_| unavailable())?,
            );
            for id in ids {
                let s = unknown::observed(shell, &token.owner.team, id).ok_or_else(unavailable)?;
                file_titles::group_label_from_lookup(id, s.lookup(), Some(presentation.context()))
                    .map_err(|_| unavailable())?;
                selected.insert(id.clone(), s);
            }
        }
        Ok(Some(Self {
            policy,
            source,
            ids: ids.clone(),
            selected,
            presentation,
        }))
    }
    pub(super) fn current(&self, shell: &ShellState, token: &Token) -> bool {
        token.current(shell)
            && self.policy.current(shell)
            && self.selected.values().all(|s| s.current(shell))
    }
    pub(super) fn label(&self, id: &str) -> Result<String, file_titles::Unavailable> {
        if !self.ids.contains(id) {
            return Err(file_titles::Unavailable::InvalidMetadata);
        }
        if self.policy.restricted {
            return Ok(self.presentation.context().unknown_label().into());
        }
        file_titles::group_label_from_lookup(
            id,
            self.selected
                .get(id)
                .ok_or(file_titles::Unavailable::InvalidMetadata)?
                .lookup(),
            Some(self.presentation.context()),
        )
    }
    pub(super) fn snapshot(&self) -> Value {
        let mut v = serde_json::json!({"ids":self.ids,"rows":self.source.as_ref().map(|source|source.rows().iter().filter(|r|r.source["id"].as_str().is_some_and(|id|self.ids.contains(id))).collect::<Vec<_>>()).unwrap_or_default(),
            "labels":self.ids.iter().map(|id|(id,self.label(id).ok())).collect::<BTreeMap<_,_>>(),"provenance":"shared-group-history-snapshot"});
        // Preserve settled known-row receipt shape. Additional provenance is
        // present precisely for the new placeholder/restricted paths.
        if self.policy.restricted
            || self.selected.values().any(|s| {
                s.lookup().kind()
                    != super_platinum_core::state::directory_usergroups::unknown::LookupKind::Known
            })
        {
            v["selector"] = serde_json::json!({"restricted":self.policy.restricted,"selfRows":self.policy.source.rows().iter().map(|r|serde_json::json!({"source":r.source,"native":r.native,"interaction":r.interaction})).collect::<Vec<_>>(),"presentation":self.presentation.descriptor(),
                "observed":self.selected.iter().map(|(id,s)|serde_json::json!({"id":id,"kind":format!("{:?}",s.lookup().kind()),"transient":s.lookup().is_transient(),"effective":s.lookup().effective()})).collect::<Vec<_>>()});
        }
        v
    }
}
