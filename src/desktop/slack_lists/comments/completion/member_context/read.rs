use super::*;
use crate::directory_member_fetches::search as intake;
use super_platinum_core::{
    block_actions::directory_endpoint::{DirectoryEndpoint, member_info},
    slack::PreparedRequest,
    slack::transport::{ChannelReadAttempt, ChannelReadResponse},
};
mod interaction;
const GENERATION: &str = "list-comment-context-members";
pub(super) enum Kind {
    Discovery,
    Generic(BTreeMap<String, u64>),
    Channel,
    Member,
    Membership,
}

pub(super) async fn execute(
    state: Signal<ShellState>,
    owner: &Fence,
    request: PreparedRequest,
    kind: Kind,
    current: &impl Fn(&ShellState) -> bool,
) -> Result<(Value, Option<Vec<String>>), Error> {
    if !owner.current(&state.peek()) || !current(&state.peek()) {
        return Err(unavailable());
    }
    let admitted = || owner.current(&state.peek()) && current(&state.peek());
    let work = async {
        if crate::fixture::is_fixture() {
            if !super::super::super::fixture::completion::enabled(&state.peek()) {
                return Err(unavailable());
            }
            return super::super::super::fixture::completion::request(state, &request).await;
        }
        match kind {
            Kind::Discovery | Kind::Generic(_) => {
                let attempt = match kind {
                    Kind::Discovery => ChannelReadAttempt::discovery(
                        &owner.context.client,
                        &owner.context.session,
                    )?,
                    Kind::Generic(ref versions) => ChannelReadAttempt::generic(
                        &owner.context.client,
                        &owner.context.session,
                        versions,
                    ),
                    _ => unreachable!(),
                };
                match owner
                    .context
                    .transport
                    .execute_channel_read_current(attempt, &admitted)
                    .await?
                {
                    ChannelReadResponse::Setup(raw) | ChannelReadResponse::Generic(raw) => {
                        Ok((raw, None))
                    }
                    _ => Err(unavailable()),
                }
            }
            Kind::Channel => owner
                .context
                .transport
                .execute_directory_channel_info_current(request, &admitted)
                .await
                .map(|r| r.into_parts()),
            Kind::Member => owner
                .context
                .transport
                .execute_directory_member_info_current(request, &admitted)
                .await
                .map(|r| r.into_parts()),
            Kind::Membership => owner
                .context
                .transport
                .execute_list_membership_current(request, &admitted)
                .await
                .map(|r| r.into_parts()),
        }
    };
    let retired = async {
        loop {
            if !admitted() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    };
    let result =
        tokio::select! { biased; _ = retired => Err(unavailable()), result = work => result };
    if !admitted() {
        return Err(unavailable());
    }
    result
}
pub(super) async fn endpoint(
    state: Signal<ShellState>,
    owner: &Fence,
    current: &impl Fn(&ShellState) -> bool,
) -> Result<DirectoryEndpoint, Error> {
    let ctx = &owner.context;
    let at = Instant::now();
    let (raw, _) = execute(
        state,
        owner,
        DirectoryEndpoint::discovery(&ctx.client, &ctx.session)?,
        Kind::Discovery,
        current,
    )
    .await?;
    DirectoryEndpoint::decode(&ctx.session, &raw, at)
}

// Dropping an in-flight original operation clears only its pending cache IDs;
// a successor slot cannot receive its late result or cleanup.
struct Pending {
    state: Signal<ShellState>,
    channel: String,
    slot: Arc<()>,
    read: membership::Read,
    settled: bool,
}
impl Drop for Pending {
    fn drop(&mut self) {
        if self.settled {
            return;
        }
        let mut shell = self.state.write();
        if let Some(slot) = shell
            .slack_lists
            .comments
            .completion
            .member_context
            .slots
            .get_mut(&self.channel)
            .filter(|s| Arc::ptr_eq(&s.identity, &self.slot))
        {
            let _ = slot.cache.clear(&self.read);
            slot.revision = slot.revision.wrapping_add(1);
        }
    }
}
pub(super) async fn prefetch(
    mut state: Signal<ShellState>,
    owner: &Arc<Fence>,
    channel: &str,
    row: &Value,
    slot: &Arc<()>,
    current: &impl Fn(&ShellState) -> bool,
) -> Result<(), Error> {
    let ctx = &owner.context;
    let receipt = intake::Read::capture(&state.peek(), &ctx.session.team_id, GENERATION)?;
    let source = intake::snapshot(&state.peek(), &ctx.session.team_id, GENERATION)?;
    let admitted = |s: &ShellState| current(s) && receipt.current(s) && source.current(s);
    let endpoint = endpoint(state, owner, &admitted).await?;
    let first = membership::Read::first_members(&ctx.session, channel)?;
    let request = endpoint.list_membership(&ctx.client, &ctx.session, Instant::now(), &first)?;
    let scoped = |s: &ShellState| admitted(s) && endpoint.current(&ctx.session, Instant::now());
    let (raw, keys) = execute(state, owner, request, Kind::Membership, &scoped).await?;
    let first = membership::FirstMembers::decode(&first, &ctx.session, raw, keys.as_deref())?;
    let ids = first.ids().to_vec();
    // The returned members and canInteract are not membership booleans.
    let (rows, interactions) = interaction::acquire(
        state,
        owner,
        &endpoint,
        &source,
        first.rows(),
        truth(&row["is_file"]),
        &scoped,
    )
    .await?;
    receipt
        .publish_rows(state, &rows, &interactions, current)
        .await?;
    if ids.is_empty() {
        return Ok(());
    }
    // Original mVvn batches pending IDs on a 200ms trailing debounce. This
    // scoped first-N acquisition has one unique bounded batch, not an app tick.
    for chunk in ids.chunks(membership::MEMBERSHIP_LIMIT) {
        if !current(&state.peek()) {
            return Err(unavailable());
        }
        let read = membership::Read::membership(&ctx.session, channel, chunk.to_vec())?;
        {
            let mut shell = state.write();
            let stored = shell
                .slack_lists
                .comments
                .completion
                .member_context
                .slots
                .get_mut(channel)
                .filter(|s| Arc::ptr_eq(&s.identity, slot))
                .ok_or_else(unavailable)?;
            stored.cache.begin(&read)?;
            stored.revision = stored.revision.wrapping_add(1);
        }
        let mut pending = Pending {
            state,
            channel: channel.into(),
            slot: slot.clone(),
            read,
            settled: false,
        };
        tokio::time::sleep(Duration::from_millis(200)).await;
        if !current(&state.peek()) {
            return Err(unavailable());
        }
        let request =
            endpoint.list_membership(&ctx.client, &ctx.session, Instant::now(), &pending.read)?;
        let scoped = |s: &ShellState| current(s) && endpoint.current(&ctx.session, Instant::now());
        let (raw, _) = execute(state, owner, request, Kind::Membership, &scoped).await?;
        let response = membership::Membership::decode(&pending.read, &ctx.session, raw)?;
        {
            let mut shell = state.write();
            if !current(&shell) {
                return Err(unavailable());
            }
            let stored = shell
                .slack_lists
                .comments
                .completion
                .member_context
                .slots
                .get_mut(channel)
                .filter(|s| Arc::ptr_eq(&s.identity, slot))
                .ok_or_else(unavailable)?;
            stored.cache.accept(&response)?;
            stored.revision = stored.revision.wrapping_add(1);
        }
        pending.settled = true;
    }
    Ok(())
}
