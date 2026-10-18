//! Private ScqY publication followed by fresh-response-only native repair.
use super::*;
use super_platinum_core::{
    block_actions::workflow_management::member_hydration::RendererReply, slack::models::User,
    state::directory_members::Store,
};
#[cfg(test)]
#[path = "tests.rs"]
mod tests;

struct Stage {
    store: Store,
    native: HashMap<String, User>,
    fresh: Vec<Value>,
}
impl Stage {
    fn capture(
        shell: &ShellState,
        attempt: &Attempt,
        received: &read::Received,
    ) -> Result<Self, Error> {
        if !attempt.current(shell) || received.requested.is_empty() {
            return Err(unavailable());
        }
        let allowed: BTreeSet<_> = attempt.ids.iter().map(String::as_str).collect();
        let requested: BTreeSet<_> = received.requested.iter().map(String::as_str).collect();
        if requested.len() != received.requested.len()
            || !requested.is_subset(&allowed)
            || received
                .rows
                .iter()
                .any(|row| row["id"].as_str().is_none_or(|id| !requested.contains(id)))
            || received
                .interactions
                .keys()
                .any(|id| !requested.contains(id.as_str()))
            || received
                .disconnected
                .iter()
                .any(|id| !requested.contains(id.as_str()))
        {
            return Err(unavailable());
        }
        let ctx = &attempt.context;
        let workspace = shell
            .core
            .workspaces
            .get(&ctx.session.team_id)
            .ok_or_else(unavailable)?;
        Self::from_parts(
            workspace.directory_members.clone(),
            workspace.users.clone(),
            ctx,
            received,
        )
    }
    fn from_parts(
        mut store: Store,
        native: HashMap<String, User>,
        ctx: &Context,
        received: &read::Received,
    ) -> Result<Self, Error> {
        store.enqueue_member_info(
            &ctx.account,
            &ctx.session,
            received.rows.clone(),
            serde_json::to_value(&received.interactions).map_err(|_| unavailable())?,
            received.requested.clone(),
            received.disconnected.clone(),
        )?;
        Ok(Self {
            store,
            native,
            fresh: received.rows.clone(),
        })
    }
    fn repair_rows(&self) -> Result<Vec<Value>, Error> {
        let required = self.store.required_observations(&self.native);
        let mut seen = BTreeMap::new();
        let mut repair = Vec::new();
        for row in self.fresh.iter().rev() {
            let id = row["id"].as_str().ok_or_else(unavailable)?;
            if required.contains(id) {
                if let Some(previous) = seen.insert(id.to_owned(), row) {
                    if previous != row {
                        return Err(unavailable());
                    }
                } else {
                    repair.push(row.clone());
                }
            }
        }
        repair.reverse();
        Ok(repair)
    }
    fn install(
        self,
        shell: &mut ShellState,
        attempt: &Attempt,
        current: &impl Fn(&ShellState) -> bool,
    ) -> Result<(), Error> {
        if !attempt.current(shell)
            || !current(shell)
            || self
                .store
                .has_pending(&attempt.context.account, &attempt.context.session)?
        {
            return Err(unavailable());
        }
        let workspace = shell
            .core
            .workspaces
            .get_mut(&attempt.context.session.team_id)
            .ok_or_else(unavailable)?;
        workspace.directory_members = self.store;
        workspace.users = self.native;
        Ok(())
    }
}
async fn render(
    state: Signal<ShellState>,
    attempt: &Attempt,
    current: &impl Fn(&ShellState) -> bool,
    generation: &str,
    fields: &[String],
) -> Result<RendererReply, Error> {
    let lowered = channel_fetches::member_fields(
        state,
        |shell| attempt.current(shell) && current(shell),
        generation,
        fields,
    )
    .await?;
    Ok(RendererReply {
        generation: generation.into(),
        operation: "ordinary-toLowerCase".into(),
        fields: fields.to_vec(),
        lowered,
    })
}
pub(super) async fn apply(
    mut state: Signal<ShellState>,
    attempt: &Attempt,
    received: read::Received,
    current: &impl Fn(&ShellState) -> bool,
) -> Result<(), Error> {
    if !current(&state.peek()) {
        return Err(unavailable());
    }
    let mut stage = Stage::capture(&state.peek(), attempt, &received)?;
    let ctx = &attempt.context;
    let flags = attempt.flags.flags();
    let mut count = 0;
    while stage.store.has_pending(&ctx.account, &ctx.session)? {
        if !attempt.current(&state.peek()) || !current(&state.peek()) || count >= 64 {
            return Err(unavailable());
        }
        let pending = stage.store.prepare(
            &ctx.account,
            &ctx.session,
            ctx.epoch,
            GENERATION,
            flags.clone(),
            &stage.native,
        )?;
        let reply = render(
            state,
            attempt,
            current,
            pending.generation(),
            pending.fields(),
        )
        .await?;
        let commit = stage.store.adopt(
            pending,
            reply,
            flags,
            &ctx.account,
            &ctx.session,
            ctx.epoch,
            GENERATION,
            &stage.native,
        )?;
        stage.native = commit.into_parts().1;
        count += 1;
    }
    // ScqY can skip a positive equal-version row while the typed copy drifted.
    // Only a freshly returned row can repair it; retained wire is never replayed.
    let repair = stage.repair_rows()?;
    if !repair.is_empty() {
        let pending = stage.store.prepare_native_repair(
            &ctx.account,
            &ctx.session,
            ctx.epoch,
            GENERATION,
            flags.clone(),
            repair,
            &stage.native,
        )?;
        let reply = render(
            state,
            attempt,
            current,
            pending.generation(),
            pending.fields(),
        )
        .await?;
        stage.native = stage
            .store
            .adopt_native_repair(
                pending,
                reply,
                flags,
                &ctx.account,
                &ctx.session,
                ctx.epoch,
                GENERATION,
                &stage.native,
            )?
            .into_native();
    }
    // Keep original pages/errors alive until the entire private candidate is
    // either installed or refused. They are never reconstructed from rows.
    let _evidence = (&received.pages, &received.errors);
    stage.install(&mut state.write(), attempt, current)
}
