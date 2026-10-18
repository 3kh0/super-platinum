//! Actual split startup/resync reads and synchronous branch settlement.
use super::*;
use super_platinum_core::{
    slack::{
        api,
        transport::{ChannelReadAttempt, ChannelReadResponse},
    },
    state::{
        cfm_payload::PayloadKind,
        directory_channels::{
            freshness::{
                Scope,
                refresh::{Branch, Refresh},
            },
            resync::{Action, ChannelsData},
        },
    },
};

pub(super) struct Attempt {
    pub(super) token: Arc<()>,
    scope: Scope,
    refresh: Refresh,
    boot: bool,
    version_all_channels: bool,
}
#[derive(Clone)]
pub(super) struct Read {
    pub(super) owner: Owner,
    pub(super) token: Arc<()>,
    pub(super) channel_revision: u64,
}
impl Read {
    pub(super) fn current(&self, shell: &ShellState) -> bool {
        self.owner.current(shell)
            && shell.app_home.channel_fetches.slots[&self.owner.context.key()]
                .initial
                .as_ref()
                .is_some_and(|a| Arc::ptr_eq(&a.token, &self.token) && a.refresh.current(&a.scope))
    }
}

/// Ordinary native host has no p8lb query/localStorage override. Its actual
/// noGroup source selector yields false; this is not an experiment assignment.
pub(crate) async fn initialize(state: Signal<ShellState>, context: Arc<Context>, warm: bool) {
    run(state, context, true, warm, false).await;
}
pub(crate) fn resync(state: Signal<ShellState>, team: String) {
    let Ok(context) = Context::capture(&state.peek(), &team) else {
        return;
    };
    dioxus::core::spawn_forever(run(state, Arc::new(context), false, false, false));
}
async fn run(
    mut state: Signal<ShellState>,
    context: Arc<Context>,
    boot: bool,
    warm: bool,
    version: bool,
) {
    if crate::fixture::is_fixture() || !context.current(&state.peek()) {
        return;
    }
    let team = context.session.team_id.clone();
    // Assignments and raw member initialization are dependencies, not positive
    // major authority and not a reason to replace retained warm channels.
    let assignments = context
        .transport
        .execute(api::list_experiments(&context.client, &context.session))
        .await;
    if !context.current(&state.peek()) {
        return;
    }
    let transformations = match assignments.and_then(|raw| members::Transformations::decode(&raw)) {
        Ok(value) => Arc::new(value),
        Err(error) => {
            state.write().report_failure(&error, "", || {
                "Channel initialization could not refresh metadata.".into()
            });
            return;
        }
    };
    let owner = match begin(&mut state.write(), context, transformations, boot, version) {
        Ok(owner) => owner,
        Err(_) => return,
    };
    let pause = boot
        .then(|| crate::app_home::cfm_lifecycle::boot_started(&mut state.write(), &team))
        .flatten();
    if members::drain(state, &owner).await.is_err() || drain::wait(state, &owner).await.is_err() {
        retire(&mut state.write(), &owner);
        crate::app_home::cfm_lifecycle::boot_settled(&mut state.write(), pause);
        return;
    }
    let read = {
        let s = state.peek();
        let a = s.app_home.channel_fetches.slots[&owner.context.key()]
            .initial
            .as_ref()
            .unwrap();
        Read {
            owner: owner.clone(),
            token: a.token.clone(),
            channel_revision: s.core.workspaces[&team].channel_lifecycle.revision(),
        }
    };
    let mut changed = state.peek().app_home.channel_fetches.slots[&owner.context.key()]
        .changes
        .subscribe();
    let c = &owner.context;
    let requests = if boot {
        [
            ChannelReadAttempt::initial_init(&c.client, &c.session),
            ChannelReadAttempt::initial_channels(&c.client, &c.session, version, warm, None),
            ChannelReadAttempt::initial_extras(&c.client, &c.session),
        ]
    } else {
        [
            ChannelReadAttempt::resync_init(&c.client, &c.session),
            ChannelReadAttempt::resync_channels(&c.client, &c.session, version),
            ChannelReadAttempt::resync_extras(&c.client, &c.session),
        ]
    };
    for (branch, request) in [Branch::Init, Branch::Channels, Branch::Extras]
        .into_iter()
        .zip(requests)
    {
        let read = read.clone();
        dioxus::core::spawn_forever(branch_read(state, read, branch, request));
    }
    // The outer deadline retires publication; issued branch tasks retain their
    // request receipts and can account for late failures without replaying IO.
    let completion = async {
        loop {
            if !read.current(&state.peek()) {
                return;
            }
            if changed.changed().await.is_err() {
                return;
            }
        }
    };
    // Startup owns blocking/delaying registrations; only SonicResynced has
    // the source's outer thirty-second deadline. Each transport read is bounded.
    let timed_out = if boot {
        completion.await;
        false
    } else {
        tokio::time::timeout(Duration::from_secs(30), completion)
            .await
            .is_err()
    };
    if timed_out {
        let mut s = state.write();
        if read.current(&s) {
            let a = s
                .app_home
                .channel_fetches
                .slots
                .get_mut(&c.key())
                .unwrap()
                .initial
                .as_mut()
                .unwrap();
            let effects = a.refresh.timeout(&a.scope).ok();
            if !boot && effects.is_some_and(|e| e.advance_major) {
                crate::app_home::cfm_lifecycle::advance_major(&mut s, c);
            }
            retire(&mut s, &owner);
        }
    }
    crate::app_home::cfm_lifecycle::boot_settled(&mut state.write(), pause);
}

pub(super) fn begin(
    shell: &mut ShellState,
    context: Arc<Context>,
    transformations: Arc<members::Transformations>,
    boot: bool,
    version: bool,
) -> Result<Owner, Error> {
    if !context.current(shell) {
        return Err(unavailable());
    }
    let key = context.key();
    if shell
        .app_home
        .channel_fetches
        .slots
        .get(&key)
        .is_some_and(|s| {
            s.context.current(shell)
                && s.initial
                    .as_ref()
                    .is_some_and(|a| a.refresh.current(&a.scope))
        })
    {
        return Err(unavailable());
    }
    let publication = Arc::new(observe(shell, &context)?);
    let scope =
        Scope::new(&context.account, &context.session, context.epoch).map_err(|_| unavailable())?;
    let token = Arc::new(());
    let owner = Owner {
        context: context.clone(),
        token: Arc::new(()),
    };
    let retained = shell
        .app_home
        .channel_fetches
        .slots
        .get(&key)
        .is_some_and(|s| s.context.account_current(shell));
    let payload = if retained {
        std::mem::take(
            &mut shell
                .app_home
                .channel_fetches
                .slots
                .get_mut(&key)
                .unwrap()
                .payload,
        )
    } else {
        payload::State::default()
    };
    shell.app_home.channel_fetches.slots.insert(
        key,
        Slot {
            serial: super::super::next(),
            context,
            token: owner.token.clone(),
            changes: tokio::sync::watch::channel(0).0,
            publication,
            transformations,
            drain: None,
            retry: None,
            retry_at: None,
            initial: Some(Attempt {
                token,
                refresh: Refresh::new(scope.clone()),
                scope,
                boot,
                version_all_channels: version,
            }),
            payload,
            errors: vec![],
        },
    );
    Ok(owner)
}
pub(super) fn retire(shell: &mut ShellState, owner: &Owner) {
    if owner.current(shell) {
        let slot = shell
            .app_home
            .channel_fetches
            .slots
            .get_mut(&owner.context.key())
            .unwrap();
        slot.initial = None;
        slot.notify();
    }
}
async fn branch_read(
    mut state: Signal<ShellState>,
    read: Read,
    branch: Branch,
    request: ChannelReadAttempt,
) {
    let receipt = request.receipt();
    let result = read
        .owner
        .context
        .transport
        .execute_channel_read_current(request, || read.current(&state.peek()))
        .await;
    if let Err(error) = &result {
        if receipt.entered_send() || read.current(&state.peek()) {
            failed(&mut state.write(), &read.owner, error.clone());
        }
    }
    if !read.current(&state.peek()) {
        return;
    }
    let result = match result {
        Ok(ChannelReadResponse::Split { raw, bytes }) => {
            if branch == Branch::Channels {
                if state.peek().core.workspaces[&read.owner.context.session.team_id]
                    .channel_lifecycle
                    .revision()
                    != read.channel_revision
                {
                    Err(unavailable())
                } else {
                    settle_channels(&mut state.write(), &read, &raw)
                }
            } else {
                let kind = if branch == Branch::Init {
                    PayloadKind::Init
                } else {
                    PayloadKind::Extras
                };
                match payload::prepare(state, &read.owner, kind, &raw, bytes).await {
                    Ok(prepared) if read.current(&state.peek()) => {
                        settle_payload(&mut state.write(), &read, branch, prepared)
                    }
                    _ => Err(unavailable()),
                }
            }
        }
        _ => Err(unavailable()),
    };
    // A preceding timer effect remains applied even if a later payload sink fails.
    payload::kick(state, read.owner.clone());
    if result.is_err() && read.current(&state.peek()) {
        branch_failed(&mut state.write(), &read, branch);
    }
    drain::kick(state, read.owner.clone());
}
fn before(
    shell: &mut ShellState,
    read: &Read,
    branch: Branch,
) -> Result<super_platinum_core::state::directory_channels::freshness::refresh::Publication, Error>
{
    if !read.current(shell) {
        return Err(unavailable());
    }
    let ctx = &read.owner.context;
    let a = shell
        .app_home
        .channel_fetches
        .slots
        .get_mut(&ctx.key())
        .unwrap()
        .initial
        .as_mut()
        .unwrap();
    let (permit, effects) = a
        .refresh
        .publication(&a.scope, branch)
        .map_err(|_| unavailable())?;
    let boot = a.boot;
    if !boot && effects.advance_major {
        crate::app_home::cfm_lifecycle::advance_major(shell, ctx);
    }
    Ok(permit)
}
fn after(
    shell: &mut ShellState,
    read: &Read,
    permit: super_platinum_core::state::directory_channels::freshness::refresh::Publication,
    branch: Branch,
    success: bool,
) {
    let ctx = &read.owner.context;
    let a = shell
        .app_home
        .channel_fetches
        .slots
        .get_mut(&ctx.key())
        .unwrap()
        .initial
        .as_mut()
        .unwrap();
    let effects = a.refresh.published(&a.scope, permit, success).ok();
    let boot = a.boot;
    if boot && branch == Branch::Init && success {
        crate::app_home::cfm_lifecycle::initialized(shell, ctx);
    }
    if !boot && effects.as_ref().is_some_and(|e| e.advance_major) {
        crate::app_home::cfm_lifecycle::advance_major(shell, ctx);
    }
    if effects.is_some_and(|e| e.clear) {
        retire(shell, &read.owner);
    } else {
        shell.app_home.channel_fetches.slots[&ctx.key()].notify();
    }
}
pub(super) fn branch_failed(shell: &mut ShellState, read: &Read, branch: Branch) {
    let ctx = &read.owner.context;
    let a = shell
        .app_home
        .channel_fetches
        .slots
        .get_mut(&ctx.key())
        .unwrap()
        .initial
        .as_mut()
        .unwrap();
    let effects = a.refresh.failed(&a.scope, branch).ok();
    let boot = a.boot;
    if !boot && effects.as_ref().is_some_and(|e| e.advance_major) {
        crate::app_home::cfm_lifecycle::advance_major(shell, ctx);
    }
    if effects.is_some_and(|e| e.clear) {
        retire(shell, &read.owner);
    } else {
        shell.app_home.channel_fetches.slots[&ctx.key()].notify();
    }
}
pub(super) fn settle_payload(
    shell: &mut ShellState,
    read: &Read,
    branch: Branch,
    prepared: payload::Prepared,
) -> Result<(), Error> {
    let permit = before(shell, read, branch)?;
    let _dispatch = crate::app_home::cfm_lifecycle::dispatch();
    let result = payload::dispatch(shell, &read.owner, prepared);
    after(shell, read, permit, branch, result.is_ok());
    result
}
pub(super) fn settle_channels(
    shell: &mut ShellState,
    read: &Read,
    raw: &Value,
) -> Result<(), Error> {
    let ctx = &read.owner.context;
    if !read.current(shell)
        || shell.core.workspaces[&ctx.session.team_id]
            .channel_lifecycle
            .revision()
            != read.channel_revision
    {
        return Err(unavailable());
    }
    let observed = observe(shell, ctx)?;
    let version = shell.app_home.channel_fetches.slots[&ctx.key()]
        .initial
        .as_ref()
        .unwrap()
        .version_all_channels;
    let data = ChannelsData::decode(raw, observed.rows(), version).map_err(|_| unavailable())?;
    let permit = before(shell, read, Branch::Channels)?;
    let _dispatch = crate::app_home::cfm_lifecycle::dispatch();
    let result = channels(shell, &read.owner, data);
    after(shell, read, permit, Branch::Channels, result.is_ok());
    result
}
fn channels(shell: &mut ShellState, owner: &Owner, data: ChannelsData) -> Result<(), Error> {
    let ctx = &owner.context;
    for action in data.into_actions() {
        if !owner.publication_current(shell) {
            return Err(unavailable());
        }
        let observed = observe(shell, ctx)?;
        let mut continuation = conditional::continuation::capture(shell, owner);
        match action {
            Action::Channels(rows) => {
                let fresh = conditional::source_values(shell, owner, &rows);
                let facts = members::receipt(shell, owner, &rows)?;
                let w = shell.core.workspaces.get_mut(&ctx.session.team_id).unwrap();
                let ingress = w
                    .directory_channels
                    .capture_receipt_ingress(
                        &observed,
                        &ctx.account,
                        &ctx.session,
                        ctx.epoch,
                        GENERATION,
                        &w.channels,
                        Origin::Direct,
                        rows,
                        facts.facts.clone(),
                    )
                    .map_err(|_| unavailable())?;
                let boot = ingress.boot_in_flight();
                w.directory_channels
                    .enqueue_ingress(ingress, &ctx.account, &ctx.session, ctx.epoch)
                    .map_err(|_| unavailable())?;
                match fresh.and_then(|values| {
                    conditional::source_fresh(shell, owner, values, &facts.facts, boot)
                }) {
                    Ok(next) => continuation = next,
                    Err(error) => {
                        continuation = None;
                        failed(shell, owner, error)
                    }
                }
            }
            Action::Unchanged(ids) => {
                let w = shell.core.workspaces.get_mut(&ctx.session.team_id).unwrap();
                let (store, native) = w
                    .directory_channels
                    .queue_resync_unchanged(
                        &observed,
                        &ctx.account,
                        &ctx.session,
                        ctx.epoch,
                        GENERATION,
                        &w.channels,
                        &ids,
                    )
                    .map_err(|_| unavailable())?
                    .into_parts();
                w.directory_channels = store;
                w.channels = native;
            }
            Action::Priority(raw) => {
                shell
                    .core
                    .workspaces
                    .get_mut(&ctx.session.team_id)
                    .unwrap()
                    .priority_scores = serde_json::from_value(raw).map_err(|_| unavailable())?;
            }
            Action::Sections(raw) => {
                shell
                    .core
                    .workspaces
                    .get_mut(&ctx.session.team_id)
                    .unwrap()
                    .apply_channel_sections(
                        serde_json::from_value(raw).map_err(|_| unavailable())?,
                    );
            }
            Action::Stars(ids) => {
                let w = shell.core.workspaces.get_mut(&ctx.session.team_id).unwrap();
                w.starred_order = ids
                    .iter()
                    .map(|id| id.as_str().map(str::to_owned))
                    .collect::<Option<Vec<_>>>()
                    .ok_or_else(unavailable)?;
                for c in w.channels.values_mut() {
                    c.is_starred = w.starred_order.contains(&c.id);
                }
            }
            Action::HasMoreMpdms(value) => {
                shell
                    .app_home
                    .channel_fetches
                    .slots
                    .get_mut(&ctx.key())
                    .unwrap()
                    .payload
                    .has_more_mpdms = Some(value);
            }
            Action::Visitors(value) => {
                let rows = value
                    .as_array()
                    .ok_or_else(unavailable)?
                    .iter()
                    .map(|id| {
                        id.as_str()
                            .map(|id| (id.to_owned(), serde_json::json!({"isVisitor":true})))
                    })
                    .collect::<Option<BTreeMap<_, _>>>()
                    .ok_or_else(unavailable)?;
                shell
                    .app_home
                    .channel_fetches
                    .slots
                    .get_mut(&ctx.key())
                    .unwrap()
                    .payload
                    .visitors = rows;
            }
            Action::TemporaryExpirations(value) => {
                shell
                    .app_home
                    .channel_fetches
                    .slots
                    .get_mut(&ctx.key())
                    .unwrap()
                    .payload
                    .temporary_expirations = Some(value);
            }
        }
        conditional::continuation::advance(shell, owner, continuation)?;
    }
    Ok(())
}
