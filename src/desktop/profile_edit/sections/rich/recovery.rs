//! The shared member read owns source publication; this token owns only the
//! original unopened description and its right to resume after publication.
use super::*;
pub(super) mod lease;
use crate::directory_member_fetches::{Context, driver::Client};
use super_platinum_core::block_actions::{
    directory_endpoint::member_info::unknown::PlaceholderAsset,
    workflow_management::member_hydration::RendererReply,
};

pub(super) struct MissingOpening {
    token: Token,
    visit: u64,
    serial: u64,
    opening: u64,
    id: String,
    key: u64,
    before: Vec<Value>,
    ids: Vec<String>,
    groups: Vec<String>,
    absent_groups: Vec<String>,
    files: Vec<String>,
}
impl MissingOpening {
    pub(super) fn capture(
        shell: &ShellState,
        visit: u64,
        serial: u64,
        id: &str,
        key: u64,
    ) -> Option<Self> {
        if !editable(shell, visit, serial) {
            return None;
        }
        let draft = shell.profile_edit.sections.draft.as_ref()?;
        if draft.rich.is_some() {
            return None;
        }
        let SectionValue::LongText(before) = draft.form.as_ref()?.value(id)? else {
            return None;
        };
        let mut ids = Vec::new();
        let mut groups = Vec::new();
        let mut files = Vec::new();
        for raw in collect(before).ok()? {
            if raw["type"] == "user" {
                let id = raw["user_id"].as_str()?.to_owned();
                if !ids.contains(&id) {
                    ids.push(id);
                }
            }
            if raw["type"] == "usergroup" {
                let id = raw["usergroup_id"].as_str()?.to_owned();
                if !groups.contains(&id) {
                    groups.push(id);
                }
            }
            if matches!(raw["type"].as_str(), Some("file" | "canvas")) {
                let id = raw["file_id"].as_str()?.to_owned();
                if !id.is_empty() && !files.contains(&id) {
                    files.push(id);
                }
            }
        }
        let workspace = shell.core.workspaces.get(&draft.token.owner.team)?;
        // rLUS absent admission is narrower than explicit Retry: a legacy,
        // malformed or unavailable *present* row never becomes an auto fetch.
        let absent_groups = groups.iter().filter(|id| {
            !workspace.usergroups.contains_key(*id)
                && super_platinum_core::block_actions::directory_endpoint::usergroup_info::Read::capture(
                    vec![(*id).clone()],
                ).is_ok()
        }).cloned().collect();
        (!ids.is_empty() || !groups.is_empty() || !files.is_empty()).then(|| Self {
            token: draft.token.clone(),
            visit,
            serial,
            opening: draft.rich_opening,
            id: id.into(),
            key,
            before: before.clone(),
            ids,
            groups,
            absent_groups,
            files,
        })
    }
    fn current(&self, shell: &ShellState) -> bool {
        self.token.current(shell)
            && editable(shell, self.visit, self.serial)
            && shell.profile_edit.sections.draft.as_ref().is_some_and(|d| {
                d.token == self.token
                    && d.rich_opening == self.opening
                    && d.rich.is_none()
                    && d.form.as_ref().and_then(|f| f.value(&self.id))
                        == Some(&SectionValue::LongText(self.before.clone()))
            })
    }
    pub(super) fn group_rate(
        &self,
        shell: &ShellState,
    ) -> Option<crate::usergroups::recovery::RateStatus> {
        if self.group_recovery_ids(shell).ok()?.is_empty() {
            return None;
        }
        let status = crate::usergroups::recovery::rate_status(shell, &self.token.owner.team);
        status.observed.then_some(status)
    }
    pub(super) fn start(self, mut state: Signal<ShellState>, known: bool, automatic_members: bool) {
        if !self.current(&state.peek())
            || self
                .group_rate(&state.peek())
                .is_some_and(|status| status.blocked)
        {
            return;
        }
        // Retain the original credentials/socket even when another kind of
        // reference must finish first. GroupRead owns the subsequent source
        // receipt; it must not recapture a successor opening context.
        let automatic_groups = if !known && !self.absent_groups.is_empty() {
            let Ok(owner) = Context::capture(&state.peek(), &self.token.owner.team) else {
                return;
            };
            Some(owner)
        } else {
            None
        };
        let client = if self.ids.is_empty() {
            None
        } else {
            let Ok(client) = Client::begin(state, &self.token.owner.team, &self.ids) else {
                return;
            };
            client
        };
        if client.is_none()
            && !known
            && !automatic_members
            && self.files.is_empty()
            && automatic_groups.is_none()
        {
            return;
        }
        let (identity, mut cancellation) = {
            let mut shell = state.write();
            if !self.current(&shell) {
                return;
            }
            let d = shell.profile_edit.sections.draft.as_mut().unwrap();
            retire_recovery(d);
            let (lease, identity, receiver) = lease::Lease::new();
            d.rich_recovery = Some(lease);
            d.rich_reading =
                known || automatic_members || !self.files.is_empty() || automatic_groups.is_some();
            (identity, receiver)
        };
        // Cancellation drops this opening's group read. Shared member acquisition
        // remains account-owned and may settle for another current subscriber.
        spawn(async move {
            let result = tokio::select! {
                biased;
                _=lease::cancelled(&mut cancellation)=>Err(unavailable()),
                result=self.run(state,client,known,automatic_members,automatic_groups)=>result,
            };
            self.resume(state, result.is_ok(), known, &identity);
        });
    }
    async fn run(
        &self,
        state: Signal<ShellState>,
        client: Option<Client>,
        known: bool,
        automatic_members: bool,
        automatic_groups: Option<Context>,
    ) -> Result<(), Error> {
        if !self.files.is_empty() {
            files::recover(state, &self.token, &self.files, known, |shell| {
                self.current(shell)
            })
            .await?;
        }
        if let Some(client) = client {
            self.absent(state, client).await?;
        }
        if automatic_members && !known {
            crate::directory_member_fetches::automatic::wait(
                state,
                &self.token.owner.team,
                &self.ids,
                |shell| self.current(shell),
            )
            .await?;
        }
        if known {
            if !self.current(&state.peek()) {
                return Err(unavailable());
            }
            let ids = if self.ids.is_empty() {
                Vec::new()
            } else {
                opening::known_recovery_ids(&state.peek(), &self.token, self.opening, &self.ids)?
            };
            if !ids.is_empty() {
                crate::directory_member_fetches::known::recover(
                    state,
                    &self.token.owner.team,
                    &ids,
                    |shell| self.current(shell),
                )
                .await?;
            }
        }
        let ids = if known {
            self.group_recovery_ids(&state.peek())?
        } else if let Some(owner) = &automatic_groups {
            if !self.automatic_group_current(&state.peek(), owner) {
                return Err(unavailable());
            }
            self.absent_group_recovery_ids(&state.peek())?
        } else {
            Vec::new()
        };
        if !ids.is_empty() {
            crate::usergroups::recovery::recover(state, &self.token.owner.team, &ids, |shell| {
                self.current(shell)
                    && automatic_groups
                        .as_ref()
                        .is_none_or(|owner| self.automatic_group_current(shell, owner))
            })
            .await?;
        }
        if automatic_groups
            .as_ref()
            .is_some_and(|owner| !self.automatic_group_current(&state.peek(), owner))
        {
            return Err(unavailable());
        }
        Ok(())
    }
    fn automatic_group_current(&self, shell: &ShellState, owner: &Context) -> bool {
        self.current(shell) && owner.current(shell)
    }
    fn absent_group_recovery_ids(&self, shell: &ShellState) -> Result<Vec<String>, Error> {
        if !self.current(shell) {
            return Err(unavailable());
        }
        let workspace = shell
            .core
            .workspaces
            .get(&self.token.owner.team)
            .ok_or_else(unavailable)?;
        // Source callback excludes IDs installed while the original request
        // waited. Never expand to a group that was present at opening.
        Ok(self
            .absent_groups
            .iter()
            .filter(|id| !workspace.usergroups.contains_key(*id))
            .cloned()
            .collect())
    }
    fn group_recovery_ids(&self, shell: &ShellState) -> Result<Vec<String>, Error> {
        if !self.current(shell) {
            return Err(unavailable());
        }
        let workspace = shell
            .core
            .workspaces
            .get(&self.token.owner.team)
            .ok_or_else(unavailable)?;
        Ok(self
            .groups
            .iter()
            .filter(|id| {
                workspace
                    .usergroups
                    .get(*id)
                    .is_none_or(|group| group.id != **id || group.profile_label().is_none())
            })
            .cloned()
            .collect())
    }
    async fn absent(&self, mut state: Signal<ShellState>, mut client: Client) -> Result<(), Error> {
        while let Some(mut election) = client.next(|shell| self.current(shell)).await? {
            let flags = crate::app_home::channel_fetches::member_flags(
                &state.peek(),
                &self.token.owner.team,
            )?;
            let generation = format!("profile-missing-member-{}", self.opening);
            let asset = PlaceholderAsset::observed("https://a.slack-edge.com/bv1-13/")?;
            let mut stage = election.stage(
                &state.peek(),
                flags.flags().clone(),
                generation.clone(),
                &asset,
            )?;
            loop {
                let pending = stage.prepare(&state.peek(), &election)?;
                let Some(pending) = pending else { break };
                let fields = pending.fields().to_vec();
                let lowered = crate::app_home::channel_fetches::member_fields(
                    state,
                    |shell| self.current(shell) && flags.current(shell) && election.current(shell),
                    &generation,
                    &fields,
                )
                .await?;
                let reply = RendererReply {
                    generation: pending.generation().into(),
                    operation: "ordinary-toLowerCase".into(),
                    fields,
                    lowered,
                };
                stage.apply(&state.peek(), &election, pending, reply)?;
            }
            stage.install(&mut state.write(), &mut election, |shell| {
                self.current(shell) && flags.current(shell)
            })?;
        }
        Ok(())
    }
    fn cleanup(&self, shell: &mut ShellState, identity: &Arc<()>) -> bool {
        if !self.token.current(shell) {
            return false;
        }
        let Some(d) = shell.profile_edit.sections.draft.as_mut() else {
            return false;
        };
        if d.token != self.token
            || d.visit != self.visit
            || d.serial != self.serial
            || d.rich_opening != self.opening
            || d.rich.is_some()
            || !d
                .rich_recovery
                .as_ref()
                .is_some_and(|lease| lease.owns(identity))
        {
            return false;
        }
        retire_recovery(d);
        true
    }
    fn resume(self, mut state: Signal<ShellState>, success: bool, known: bool, identity: &Arc<()>) {
        let hits = {
            let mut shell = state.write();
            let may_resume = self.current(&shell);
            if !self.cleanup(&mut shell, identity) || !may_resume || !success {
                return;
            }
            let (opening, hits) = opening::tracked_with_members(
                &mut shell,
                self.visit,
                self.serial,
                &self.id,
                self.key,
                !known,
            );
            if let Ok(opening) = opening {
                finish(&mut shell, opening);
            }
            hits
        };
        for hit in hits {
            hit.schedule(state);
        }
    }
}

#[cfg(test)]
mod tests;
