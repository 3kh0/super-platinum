//! Complete ordered group search under the original host and source owner.
//! HTTP pages do not publish independently and cannot initialize legacy history.
use super::*;
use crate::{directory_member_fetches::Context, usergroups::source as intake};
use super_platinum_core::{
    slack::api::custom::texty::groups, state::directory_usergroups as source_core,
};
#[cfg(test)]
mod tests;

pub(super) struct Acquired {
    pub(super) pages: groups::Pages,
    pub(super) source: Arc<group_source::Snapshot>,
}

struct Read {
    owner: Arc<Context>,
    observed: source_core::Observation,
}
impl Read {
    fn capture(shell: &ShellState, team: &str) -> Result<Self, Error> {
        let owner = Arc::new(Context::capture(shell, team)?);
        let w = shell.core.workspaces.get(team).ok_or_else(unavailable)?;
        // A queued predecessor has not published its effective record yet.
        // Do not dispatch against an incomplete source/native preimage.
        if w.directory_usergroups.has_pending() {
            return Err(unavailable());
        }
        let observed = w
            .directory_usergroups
            .observe(&owner.account, &owner.session, owner.epoch, &w.usergroups)
            .map_err(|_| unavailable())?;
        Ok(Self { owner, observed })
    }
    fn current(&self, shell: &ShellState) -> bool {
        let owner = &self.owner;
        owner.current(shell)
            && shell
                .core
                .workspaces
                .get(&owner.session.team_id)
                .is_some_and(|w| {
                    self.observed.current(
                        &w.directory_usergroups,
                        &owner.account,
                        &owner.session,
                        owner.epoch,
                        &w.usergroups,
                    )
                })
    }
    fn enqueue(
        &self,
        shell: &mut ShellState,
        pages: &groups::Pages,
    ) -> Result<intake::Work, Error> {
        if !self.current(shell) {
            return Err(unavailable());
        }
        let operation = intake::Operation::Metadata {
            origin: intake::Origin::Fetch,
            rows: pages.rows()?.to_vec(),
            native_members: intake::NativeMembers::Preserve,
        };
        match intake::enqueue(shell, self.owner.clone(), operation) {
            intake::Admission::Tracked(work) => Ok(work),
            intake::Admission::Unavailable => Err(unavailable()),
        }
    }
    async fn publish(
        &self,
        mut state: Signal<ShellState>,
        search: &Search<impl Host>,
        names: &member_names::Observation,
        policy: &group_policy::Observation,
        pages: &groups::Pages,
    ) -> Result<group_source::Snapshot, Error> {
        let current = |shell: &ShellState| {
            search.current(shell) && names.current(shell, search) && policy.current(shell, search)
        };
        let work = {
            let mut shell = state.write();
            if !current(&shell) {
                return Err(unavailable());
            }
            self.enqueue(&mut shell, pages)?
        };
        // Our own FIFO reservation retires the HTTP observation's tail. From
        // this point Work's exact ticket/native preimage owns publication, while
        // the original host/name owner still guards every renderer await.
        tokio::select! {
            biased;
            _ = search.lease.cancelled() => return Err(unavailable()),
            result = work.run(state, &current) => result?,
        }
        if !current(&state.peek()) {
            return Err(unavailable());
        }
        group_source::Snapshot::under(&state.peek(), self.owner.clone())
    }
}

pub(super) async fn remote(
    mut state: Signal<ShellState>,
    search: &Search<impl Host>,
    names: &member_names::Observation,
    policy: &group_policy::Observation,
    raw_query: &str,
) -> Result<Acquired, Error> {
    let result = read(state, search, names, policy, raw_query).await;
    if let Err(error) = &result {
        search.lease.retain_failure(&mut state.write(), error);
    }
    result
}
async fn read(
    state: Signal<ShellState>,
    search: &Search<impl Host>,
    names: &member_names::Observation,
    policy: &group_policy::Observation,
    raw_query: &str,
) -> Result<Acquired, Error> {
    if !policy.current(&state.peek(), search)
        || !policy.policy().enabled()
        || !names.current(&state.peek(), search)
        || !search.can_dispatch(&state.peek())
    {
        return Err(unavailable());
    }
    let ctx = &search.context;
    let query = groups::Query::capture(
        &ctx.session,
        raw_query,
        policy.policy().include_from_another_team(),
    )?;
    // Capture once after policy Boot acquisition and BEFORE discovery/pages.
    let receipt = Read::capture(&state.peek(), search.lease.team())?;
    let current = |shell: &ShellState| {
        search.can_dispatch(shell)
            && names.current(shell, search)
            && policy.current(shell, search)
            && receipt.current(shell)
    };
    let mut endpoint: Option<DirectoryEndpoint> = None;
    let mut pages = groups::Pages::default();
    loop {
        if !current(&state.peek()) {
            return Err(unavailable());
        }
        if endpoint
            .as_ref()
            .is_none_or(|e| !e.current(&ctx.session, Instant::now()))
        {
            let raw = search
                .request(
                    state,
                    DirectoryEndpoint::discovery(&ctx.client, &ctx.session)?,
                )
                .await?;
            if !current(&state.peek()) {
                return Err(unavailable());
            }
            endpoint = Some(DirectoryEndpoint::decode(
                &ctx.session,
                &raw,
                Instant::now(),
            )?);
        }
        let endpoint = endpoint.as_ref().ok_or_else(unavailable)?;
        let request = query.request(
            &ctx.client,
            &ctx.session,
            endpoint,
            Instant::now(),
            pages.marker(),
        )?;
        let response = if search.fixture {
            tokio::select! {
                biased;
                _ = search.lease.cancelled() => Err(unavailable()),
                result = search.lease.fixture_request(state, &request, true) => result,
            }
        } else {
            tokio::select! {
                biased;
                _ = search.lease.cancelled() => Err(unavailable()),
                result = ctx.transport.execute_profile_group_search_current(request, || current(&state.peek()) && endpoint.current(&ctx.session, Instant::now())) => result.map(|r| r.into_parts()),
            }
        }?;
        if !current(&state.peek()) || !endpoint.current(&ctx.session, Instant::now()) {
            return Err(unavailable());
        }
        pages.accept(response.0, response.1.as_deref())?;
        if pages.complete() {
            let source = receipt
                .publish(state, search, names, policy, &pages)
                .await?;
            if !search.current(&state.peek())
                || !names.current(&state.peek(), search)
                || !source.current(&state.peek())
            {
                return Err(unavailable());
            }
            // Reselect actual records only; D8Wa filters absent result IDs.
            source.reselect(pages.rows()?)?;
            return Ok(Acquired {
                pages,
                source: Arc::new(source),
            });
        }
    }
}
