//! Profile-owned name policy; no Home/Activity owner or policy cache is borrowed.
use super::channel_host::Host;
use super::{Search, facts, ranking};
use crate::state::ShellState;
use dioxus::prelude::*;
use serde_json::Value;
use std::{future::Future, sync::Arc, task::Poll, time::Duration};
use super_platinum_core::{
    block_actions::{name_policy as policy, shortcuts::grid},
    slack::{Error, PreparedRequest},
    state::posting_metadata::authority::{Authority, BootRead},
};
mod save;
#[cfg(test)]
mod tests;
pub(crate) use save::SaveMemberNames;

fn unavailable() -> Error {
    Error::Api("profile_member_name_policy_unavailable".into())
}

pub(super) struct Observation {
    fence: Fence,
    authority: Authority,
    pub(super) policy: policy::Snapshot,
    pub(super) gantry: Value,
    pub(super) enterprise: Option<String>,
    pub(super) default_workspace: Option<String>,
}
impl Observation {
    pub(super) fn title_policy_proof(&self, shell: &ShellState) -> Value {
        serde_json::json!({"user":self.fence.preferences.user_prefs(),"gantry":self.gantry,"enterprise":self.enterprise,"revision":crate::app_home::autocomplete::preferences_revision(shell)})
    }
    pub(super) fn current(&self, shell: &ShellState, search: &Search<impl Host>) -> bool {
        self.fence.current(shell, search) && self.authority.current(&shell.core)
    }
}
struct Fence {
    owner: Arc<dyn Fn(&ShellState) -> bool + Send + Sync>,
    context: Arc<crate::scheduled::Context>,
    preferences: Arc<ranking::Preferences>,
}
impl Fence {
    fn capture(
        shell: &ShellState,
        search: &Search<impl Host>,
        preferences: Arc<ranking::Preferences>,
    ) -> Result<Self, Error> {
        if !search.current(shell) || !preferences.current(shell) {
            return Err(unavailable());
        }
        Ok(Self {
            owner: {
                let lease = search.lease.clone();
                Arc::new(move |shell| lease.current(shell))
            },
            context: search.context.clone(),
            preferences,
        })
    }
    fn current(&self, shell: &ShellState, search: &Search<impl Host>) -> bool {
        (self.owner)(shell)
            && self.preferences.current(shell)
            && search.current(shell)
            && Arc::ptr_eq(&self.context, &search.context)
    }
    async fn request(
        &self,
        state: Signal<ShellState>,
        search: &Search<impl Host>,
        authority: Option<&Authority>,
        request: PreparedRequest,
    ) -> Result<Value, Error> {
        let current = || {
            let shell = state.peek();
            self.current(&shell, search) && authority.is_none_or(|a| a.current(&shell.core))
        };
        // Poll admission also protects the transport's link hold/retry wakes.
        let mut work = std::pin::pin!(search.request(state, request));
        let guarded = std::future::poll_fn(|cx| {
            if !current() {
                return Poll::Ready(Err(unavailable()));
            }
            work.as_mut().poll(cx)
        });
        let retired = async {
            loop {
                if !current() {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        };
        let result = tokio::select! {
            biased;
            _ = search.lease.cancelled() => Err(unavailable()),
            _ = retired => Err(unavailable()),
            result = guarded => result,
        };
        if !current() {
            return Err(unavailable());
        }
        result
    }
}

pub(super) async fn capture(
    state: Signal<ShellState>,
    search: &Search<impl Host>,
    preferences: Arc<ranking::Preferences>,
) -> Result<Observation, Error> {
    let fence = Fence::capture(&state.peek(), search, preferences)?;
    let authority = acquire_authority(state, search, &fence).await?;
    let (membership, enterprise) = {
        let shell = state.peek();
        if !fence.current(&shell, search) {
            return Err(unavailable());
        }
        authority
            .name_policy_membership(&shell.core, policy::OrdinaryClientV2)
            .map_err(|_| unavailable())?
    };
    let ctx = &search.context;
    let gantry = fence
        .request(
            state,
            search,
            Some(&authority),
            grid::discovery(&ctx.client, &ctx.session),
        )
        .await?;
    let expected: Vec<String> = membership.workspace_ids().map(str::to_owned).collect();
    let default_workspace = facts::validate_gantry(&gantry, &ctx.session, Some(&expected))?;
    // Preferences is the existing complete Profile observation, not another
    // user's/picker's prefs and not a second users.prefs.get request.
    let user =
        policy::UserPrefs::decode(fence.preferences.user_prefs()).map_err(|_| unavailable())?;
    let enterprise_prefs = if let Some(id) = &enterprise {
        let raw = fence
            .request(
                state,
                search,
                Some(&authority),
                policy::request::enterprise(&ctx.client, &ctx.session),
            )
            .await?;
        Some(policy::EnterprisePrefs::decode(id, &raw).map_err(|_| unavailable())?)
    } else {
        None
    };
    let mut policy =
        policy::Snapshot::new(policy::OrdinaryClientV2, membership, user, enterprise_prefs)
            .map_err(|_| unavailable())?;
    observe_gantry_prefs(&mut policy, &gantry)?;
    for id in policy.missing_team_preferences() {
        let request =
            policy::request::team_for(&ctx.client, &ctx.session, &id).map_err(|_| unavailable())?;
        let raw = fence
            .request(state, search, Some(&authority), request)
            .await?;
        policy
            .observe_team(policy::TeamPrefs::decode(&id, &raw).map_err(|_| unavailable())?)
            .map_err(|_| unavailable())?;
        if policy.should_display_real_names() == Ok(true) {
            break;
        }
    }
    policy
        .should_display_real_names()
        .map_err(|_| unavailable())?;
    let observation = Observation {
        fence,
        authority,
        policy,
        gantry,
        enterprise,
        default_workspace,
    };
    if !observation.current(&state.peek(), search) {
        return Err(unavailable());
    }
    // Query-local immutable return: no global installation or notifying write.
    Ok(observation)
}

fn observe_gantry_prefs(policy: &mut policy::Snapshot, raw: &Value) -> Result<(), Error> {
    for row in raw["workspaces"].as_array().ok_or_else(unavailable)? {
        let id = row["id"].as_str().ok_or_else(unavailable)?;
        if !policy.workspace_ids().any(|key| key == id) {
            continue;
        }
        match row.get("prefs") {
            Some(Value::Object(prefs)) => {
                policy
                    .observe_team(
                        policy::TeamPrefs::from_complete(id, prefs).map_err(|_| unavailable())?,
                    )
                    .map_err(|_| unavailable())?;
            }
            None | Some(Value::Null) => (),
            _ => return Err(unavailable()),
        }
    }
    Ok(())
}

/// Drop can run after query/DOM retirement. Only the original pending nonce is
/// cleared silently; no boot data or policy is published by cleanup.
struct PendingBoot {
    state: Signal<ShellState>,
    read: Option<BootRead>,
}
impl PendingBoot {
    fn new(state: Signal<ShellState>, read: BootRead) -> Self {
        Self {
            state,
            read: Some(read),
        }
    }
    fn take(&mut self) -> BootRead {
        self.read.take().expect("boot consumed once")
    }
    fn cancel(&mut self) -> bool {
        let Some(read) = self.read.take() else {
            return false;
        };
        if !read.current(&self.state.peek().core) {
            return false;
        }
        #[allow(deprecated)]
        let mut shell = self.state.write_silent();
        read.failed(&mut shell.core)
    }
}
impl Drop for PendingBoot {
    fn drop(&mut self) {
        self.cancel();
    }
}

async fn acquire_authority(
    mut state: Signal<ShellState>,
    search: &Search<impl Host>,
    fence: &Fence,
) -> Result<Authority, Error> {
    let ctx = &search.context;
    if !fence.current(&state.peek(), search) {
        return Err(unavailable());
    }
    if let Some(authority) = name_authority(&state.peek(), &ctx.session.team_id) {
        return Ok(authority);
    }
    let (read, request) = {
        let mut shell = state.write();
        if !fence.current(&shell, search) {
            return Err(unavailable());
        }
        if let Some(authority) = name_authority(&shell, &ctx.session.team_id) {
            return Ok(authority);
        }
        let read = BootRead::begin(&mut shell.core, &ctx.transport, &ctx.session)
            .map_err(|_| unavailable())?;
        let request = match read.request(&shell.core) {
            Ok(request) => request,
            Err(_) => {
                read.failed(&mut shell.core);
                return Err(unavailable());
            }
        };
        (read, request)
    };
    let mut pending = PendingBoot::new(state, read);
    let raw = fence.request(state, search, None, request).await?;
    if !fence.current(&state.peek(), search) {
        return Err(unavailable());
    }
    let mut shell = state.write();
    if !fence.current(&shell, search) {
        return Err(unavailable());
    }
    if !pending.take().apply(&mut shell.core, raw, false)? {
        return Err(unavailable());
    }
    name_authority(&shell, &ctx.session.team_id).ok_or_else(unavailable)
}

// A bound boot can be usable for posting yet lack the observed membership
// needed by name policy. Split Init's separate workspace store does not fill
// that boot observation. Only a purpose-ready authority can skip acquisition.
fn name_authority(shell: &ShellState, team: &str) -> Option<Authority> {
    let authority = Authority::capture(&shell.core, team).ok()?;
    authority
        .name_policy_membership(&shell.core, policy::OrdinaryClientV2)
        .ok()?;
    Some(authority)
}
