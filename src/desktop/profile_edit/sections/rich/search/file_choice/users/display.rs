//! Existing-token recovery retains stale source facts without treating them as
//! a query-ready inventory. Only an accepted canonical pair can advance custody.
use super::*;
use std::collections::BTreeMap;
use super_platinum_core::state::directory_members::Observations;

pub(crate) struct Source {
    owner: Context,
    generation: String,
    observed: Observations,
    native: BTreeMap<String, Option<Value>>,
}
impl Source {
    pub(crate) fn capture(
        s: &ShellState,
        team: &str,
        generation: &str,
        required: &BTreeSet<String>,
    ) -> Result<Self, Error> {
        let owner = Context::capture(s, team)?;
        let w = s.core.workspaces.get(team).ok_or_else(unavailable)?;
        let observed = w.directory_members.observations(
            &owner.account,
            &owner.session,
            owner.epoch,
            generation,
        )?;
        let native = native(s, team, required.iter())?;
        Ok(Self {
            owner,
            generation: generation.into(),
            observed,
            native,
        })
    }
    pub(crate) fn current(&self, s: &ShellState) -> bool {
        self.owner.current(s)
            && s.core
                .workspaces
                .get(&self.owner.session.team_id)
                .is_some_and(|w| {
                    self.observed.current(
                        &w.directory_members,
                        &self.owner.account,
                        &self.owner.session,
                        self.owner.epoch,
                        &self.generation,
                    ) && native(s, &self.owner.session.team_id, self.native.keys())
                        .is_ok_and(|n| n == self.native)
                })
    }
    pub(super) fn unavailable_known(
        &self,
        s: &ShellState,
        required: &BTreeSet<String>,
    ) -> Result<Vec<String>, Error> {
        if !self.current(s) {
            return Err(unavailable());
        }
        let known: BTreeSet<_> = self.observed.source_ids().collect();
        Ok(required
            .iter()
            .filter(|id| {
                known.contains(id.as_str())
                    && Users::capture_owned(
                        s,
                        &self.owner.session.team_id,
                        &self.generation,
                        &BTreeSet::from([(*id).clone()]),
                    )
                    .is_err()
            })
            .cloned()
            .collect())
    }
}
// Only actual title dependencies are cloned/compared. Explicit None retains
// absence, so a typed-only arrival cannot slip through the original guard.
fn native<'a>(
    s: &ShellState,
    team: &str,
    ids: impl IntoIterator<Item = &'a String>,
) -> Result<BTreeMap<String, Option<Value>>, Error> {
    let users = &s.core.workspaces.get(team).ok_or_else(unavailable)?.users;
    ids.into_iter()
        .map(|id| {
            let row = users
                .get(id)
                .map(serde_json::to_value)
                .transpose()
                .map_err(|_| unavailable())?;
            Ok((id.clone(), row))
        })
        .collect()
}

/// Captured before policy IO. Invalidated source rows are recovery preimages,
/// never labels. Final availability comes only from the required fresh Subset.
pub(crate) async fn acquire(
    mut state: Signal<ShellState>,
    team: &str,
    generation: &str,
    current: impl Fn(&ShellState) -> bool,
    mut source: Source,
    required: &BTreeSet<String>,
) -> Result<Option<Arc<Users>>, Error> {
    if !current(&state.peek()) || !source.current(&state.peek()) {
        return Err(unavailable());
    }
    if let Ok(users) = Users::capture_owned(
        &state.peek(),
        team,
        &format!("{generation}-file-title-users"),
        required,
    ) {
        return Ok(Some(Arc::new(users)));
    }
    let requested: Vec<_> = required.iter().cloned().collect();
    // Client fetches only source-absent members. Invalidated known members use
    // the existing unconditional known-member read, never unknown placeholders.
    if let Some(mut client) = Client::begin(state, team, &requested)? {
        while let Some(mut election) = client.next(|s| current(s) && source.current(s)).await? {
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
                    |s| current(s) && source.current(s) && flags.current(s) && election.current(s),
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
            let mut s = state.write();
            stage.install(&mut s, &mut election, |s| {
                current(s) && source.current(s) && flags.current(s)
            })?;
            if !current(&s) {
                return Err(unavailable());
            }
            // This serial write admits only our elected pair; no unrelated source
            // replacement can be recaptured as successful recovery.
            source = Source::capture(&s, team, generation, required)?;
        }
    }
    let known = source.unavailable_known(&state.peek(), required)?;
    if !known.is_empty() {
        crate::directory_member_fetches::known::recover(state, team, &known, |s| {
            current(s) && source.current(s)
        })
        .await?;
        // recover returns directly from its guarded atomic canonical install.
        // No await separates that accepted result from the advanced witness.
        let s = state.peek();
        if !current(&s) || !source.owner.current(&s) {
            return Err(unavailable());
        }
        source = Source::capture(&s, team, generation, required)?;
    }
    if !current(&state.peek()) || !source.current(&state.peek()) {
        return Err(unavailable());
    }
    Ok(Some(Arc::new(Users::capture_owned(
        &state.peek(),
        team,
        generation,
        required,
    )?)))
}
