//! Query-owned nested Canvas title members. No existing-token cache is borrowed.
use super::*;
use crate::directory_member_fetches::{Context, driver::Client, search as intake};
use dioxus::prelude::{ReadableExt, Signal, WritableExt};
use std::collections::BTreeSet;
use super_platinum_core::{
    block_actions::{
        directory_endpoint::member_info::unknown::PlaceholderAsset,
        workflow_management::member_hydration::RendererReply,
    },
    state::directory_members::Subset,
};
mod display;
pub(crate) use display::{Source as DisplaySource, acquire as acquire_display};

pub(crate) struct Users {
    owner: Arc<Context>,
    subset: Subset,
    generation: String,
}
impl Users {
    fn capture(
        shell: &ShellState,
        search: &Search<impl Host>,
        ids: &BTreeSet<String>,
    ) -> Result<Self, Error> {
        if !search.current(shell) {
            return Err(unavailable());
        }
        Self::capture_owned(
            shell,
            search.lease.team(),
            &format!("{}-file-title-users", search.lease.member_generation()),
            ids,
        )
    }
    pub(crate) fn capture_owned(
        shell: &ShellState,
        team: &str,
        generation: &str,
        ids: &BTreeSet<String>,
    ) -> Result<Self, Error> {
        let owner = Arc::new(Context::capture(shell, team)?);
        let w = shell.core.workspaces.get(team).ok_or_else(unavailable)?;
        let generation = generation.to_owned();
        let subset = w.directory_members.subset(
            &owner.account,
            &owner.session,
            owner.epoch,
            &generation,
            ids,
            &w.users,
        )?;
        Ok(Self {
            owner,
            subset,
            generation,
        })
    }
    pub(crate) fn current(&self, shell: &ShellState) -> bool {
        self.owner.current(shell)
            && shell
                .core
                .workspaces
                .get(&self.owner.session.team_id)
                .is_some_and(|w| {
                    self.subset.current(
                        &w.directory_members,
                        &self.owner.account,
                        &self.owner.session,
                        self.owner.epoch,
                        &self.generation,
                        &w.users,
                    ) && self.subset.current_rows(
                        &w.directory_members,
                        &self.owner.account,
                        &self.owner.session,
                        self.owner.epoch,
                        &self.generation,
                        &w.users,
                    )
                })
    }
    pub(crate) fn label(
        &self,
        id: &str,
        policy: &super_platinum_core::block_actions::name_policy::Snapshot,
    ) -> Result<String, file_titles::Unavailable> {
        let row = self
            .subset
            .rows()
            .iter()
            .find(|row| row.source["id"] == id)
            .ok_or(file_titles::Unavailable::InvalidMetadata)?;
        file_titles::member_label(&row.source, policy, "Hidden user")
    }
    pub(crate) fn snapshot(&self, shell: &ShellState) -> Value {
        json!({"current":self.current(shell),"generation":self.generation,
            "members":self.subset.rows().iter().map(|row|json!({"source":row.source,"effective":row.native})).collect::<Vec<_>>()})
    }
}

/// Retain only this file's title dependencies. A shared acquisition batch is
/// not authority to attach user invalidation to an independent List/text title.
pub(super) fn for_title(
    shell: &ShellState,
    raw: &Value,
    available: Option<&Users>,
) -> Result<Option<Arc<Users>>, Error> {
    if kind(raw)? != file_titles::Kind::Canvas {
        return Ok(None);
    }
    let ids = file_titles::user_ids(raw).map_err(|_| unavailable())?;
    if ids.is_empty() {
        return Ok(None);
    }
    let available = available
        .filter(|u| u.current(shell))
        .ok_or_else(unavailable)?;
    if ids.iter().any(|id| {
        !available
            .subset
            .rows()
            .iter()
            .any(|r| r.source["id"] == id.as_str())
    }) {
        return Err(unavailable());
    }
    let w = shell
        .core
        .workspaces
        .get(&available.owner.session.team_id)
        .ok_or_else(unavailable)?;
    let subset = w.directory_members.subset(
        &available.owner.account,
        &available.owner.session,
        available.owner.epoch,
        &available.generation,
        &ids,
        &w.users,
    )?;
    Ok(Some(Arc::new(Users {
        owner: available.owner.clone(),
        subset,
        generation: available.generation.clone(),
    })))
}

fn ids(files: &file_source::Snapshot, selected: &[String]) -> Result<BTreeSet<String>, Error> {
    let mut ids = BTreeSet::new();
    for id in selected {
        let row = files
            .rows()
            .iter()
            .find(|r| r.id() == id)
            .ok_or_else(unavailable)?;
        let title = files.title(row.id())?;
        let raw = title.effective();
        if kind(raw)? == file_titles::Kind::Canvas {
            ids.extend(file_titles::user_ids(raw).map_err(|_| unavailable())?);
        }
    }
    Ok(ids)
}

/// Capture before any dependency request. Only an accepted original election
/// can advance the member snapshot; file/policy/query owners never get recaptured.
pub(in crate::profile_edit::sections::rich::search) async fn acquire(
    state: Signal<ShellState>,
    search: &Search<impl Host>,
    names: &member_names::Observation,
    files: &file_source::Snapshot,
    source: Arc<intake::Snapshot>,
    selected: &[String],
) -> Result<(Arc<intake::Snapshot>, Option<Arc<Users>>), Error> {
    let required = ids(files, selected)?;
    if required.is_empty() {
        return Ok((source, None));
    }
    let work = acquire_inner(state, search, names, files, source, &required);
    tokio::select! {biased;
        _=search.lease.cancelled()=>Err(unavailable()),
        result=work=>result,
    }
}
async fn acquire_inner(
    mut state: Signal<ShellState>,
    search: &Search<impl Host>,
    names: &member_names::Observation,
    files: &file_source::Snapshot,
    mut source: Arc<intake::Snapshot>,
    required: &BTreeSet<String>,
) -> Result<(Arc<intake::Snapshot>, Option<Arc<Users>>), Error> {
    let owner = Context::capture(&state.peek(), search.lease.team())?;
    let current = |shell: &ShellState| {
        search.current(shell)
            && names.current(shell, search)
            && files.current(shell)
            && owner.current(shell)
    };
    acquire_owned(
        state,
        search.lease.team(),
        &search.lease.member_generation(),
        current,
        source,
        required,
    )
    .await
}

/// Separately owned display consumers supply their original immutable guards.
pub(crate) async fn acquire_owned(
    mut state: Signal<ShellState>,
    team: &str,
    generation: &str,
    current: impl Fn(&ShellState) -> bool,
    mut source: Arc<intake::Snapshot>,
    required: &BTreeSet<String>,
) -> Result<(Arc<intake::Snapshot>, Option<Arc<Users>>), Error> {
    if !current(&state.peek()) || !source.current(&state.peek()) {
        return Err(unavailable());
    }
    if let Ok(users) = Users::capture_owned(
        &state.peek(),
        team,
        &format!("{generation}-file-title-users"),
        required,
    ) {
        return Ok((source, Some(Arc::new(users))));
    }
    // The existing query's full canonical snapshot is required before recovery.
    // Unknown/pending source history never gets replaced by a convenient page.
    let requested: Vec<_> = required.iter().cloned().collect();
    let Some(mut client) = Client::begin(state, team, &requested)? else {
        return Err(unavailable());
    };
    while let Some(mut election) = client
        .next(|shell| current(shell) && source.current(shell))
        .await?
    {
        let flags = crate::app_home::channel_fetches::member_flags(&state.peek(), team)?;
        let renderer_generation = format!("{generation}-title-recovery");
        let asset = PlaceholderAsset::observed("https://a.slack-edge.com/bv1-13/")?;
        let mut stage = election.stage(
            &state.peek(),
            flags.flags().clone(),
            renderer_generation.clone(),
            &asset,
        )?;
        loop {
            let Some(pending) = stage.prepare(&state.peek(), &election)? else {
                break;
            };
            let fields = pending.fields().to_vec();
            let lowered = crate::app_home::channel_fetches::member_fields(
                state,
                |shell| {
                    current(shell)
                        && source.current(shell)
                        && flags.current(shell)
                        && election.current(shell)
                },
                &renderer_generation,
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
        // Install and snapshot the accepted pair under one serial shell write.
        // No successor event can slip between admission and the owned rebuild.
        let mut shell = state.write();
        stage.install(&mut shell, &mut election, |shell| {
            current(shell) && source.current(shell) && flags.current(shell)
        })?;
        if !current(&shell) {
            return Err(unavailable());
        }
        source = Arc::new(intake::snapshot(&shell, team, generation)?);
    }
    if !current(&state.peek()) || !source.current(&state.peek()) {
        return Err(unavailable());
    }
    let users = Arc::new(Users::capture_owned(
        &state.peek(),
        team,
        generation,
        required,
    )?);
    Ok((source, Some(users)))
}

#[cfg(test)]
mod tests;
