use super::*;
use std::{future::Future, task::Poll, time::Duration};
use super_platinum_core::{
    block_actions::name_policy as policy, slack::PreparedRequest,
    state::posting_metadata::authority::Authority,
};
#[cfg(test)]
mod tests;

pub(super) struct Observation {
    ticket: Ticket,
    token: Arc<()>,
    pub root: Message,
    pub member: member::Member,
    socket: u64,
    revision: u64,
    authority: Authority,
    membership: policy::Membership,
    enterprise: Option<String>,
    pub policy: policy::Snapshot,
    raw_user: Value,
    raw_enterprise: Option<Value>,
    raw_teams: std::collections::BTreeMap<String, Value>,
}
struct Fence {
    ticket: Ticket,
    token: Arc<()>,
    root: Message,
    member: member::Member,
    socket: u64,
    revision: u64,
    authority: Authority,
    membership: policy::Membership,
    enterprise: Option<String>,
}
pub(super) fn current(
    shell: &ShellState,
    model: &Comments,
    ticket: &Ticket,
    token: &Arc<()>,
    root: &Message,
    socket: u64,
    revision: u64,
    authority: &Authority,
    membership: &policy::Membership,
    enterprise: &Option<String>,
) -> bool {
    slot_current(shell, model, ticket, token)
        && model.root.as_ref().is_some_and(|m| same_message(m, root))
        && shell
            .core
            .workspaces
            .get(&ticket.owner.scope.team)
            .is_some_and(|w| w.rt_generation == socket)
        && crate::app_home::autocomplete::preferences_revision(shell) == revision
        && authority
            .name_policy_membership(&shell.core, policy::OrdinaryClientV2)
            .is_ok_and(|(m, e)| &m == membership && &e == enterprise)
}
impl Fence {
    fn capture(
        shell: &ShellState,
        model: &Comments,
        ticket: Ticket,
        root: Message,
        token: Arc<()>,
    ) -> Result<Self, Error> {
        if !slot_current(shell, model, &ticket, &token) {
            return Err(unavailable());
        }
        let editor = source::editor(&root, source::Context::Thread, false)
            .map_err(|_| unavailable())?
            .ok_or_else(unavailable)?;
        let member = member::Member::capture(shell, &ticket.owner, editor.id())?;
        let authority =
            Authority::capture(&shell.core, &ticket.owner.scope.team).map_err(|_| unavailable())?;
        let (membership, enterprise) = authority
            .name_policy_membership(&shell.core, policy::OrdinaryClientV2)
            .map_err(|_| unavailable())?;
        let value = Self {
            socket: shell.core.workspaces[&ticket.owner.scope.team].rt_generation,
            revision: crate::app_home::autocomplete::preferences_revision(shell),
            ticket,
            token,
            root,
            member,
            authority,
            membership,
            enterprise,
        };
        value
            .current(shell, model)
            .then_some(value)
            .ok_or_else(unavailable)
    }
    fn current(&self, shell: &ShellState, model: &Comments) -> bool {
        current(
            shell,
            model,
            &self.ticket,
            &self.token,
            &self.root,
            self.socket,
            self.revision,
            &self.authority,
            &self.membership,
            &self.enterprise,
        ) && self.member.current(shell)
    }
    async fn read(
        &self,
        state: Signal<ShellState>,
        model: Signal<Comments>,
        request: PreparedRequest,
    ) -> Result<Value, Error> {
        let current = || self.current(&state.peek(), &model.peek());
        if !current() {
            return Err(unavailable());
        }
        let ctx = self
            .ticket
            .owner
            .scope
            .request(&state.peek())
            .ok_or_else(unavailable)?;
        let execute = async {
            if crate::fixture::is_fixture() {
                if !super::super::fixture::attribution::enabled(&state.peek()) {
                    return Err(unavailable());
                }
                super::super::fixture::attribution::request(state, &request).await
            } else {
                ctx.transport.execute(request).await
            }
        };
        // Recheck before every carrier poll as well as each await. The bounded
        // cancellation wake also retires a link-held read without new network IO.
        let mut work = std::pin::pin!(execute);
        let guarded = std::future::poll_fn(|cx| {
            if !current() {
                return Poll::Ready(Err(unavailable()));
            }
            work.as_mut().poll(cx)
        });
        let result = tokio::select! {
            biased;
            _=async {loop {if !current(){break;} tokio::time::sleep(Duration::from_millis(100)).await;}}=>Err(unavailable()),
            _=tokio::time::sleep(Duration::from_secs(45))=>Err(Error::Transport("Editor details timed out".into())),
            result=guarded=>result,
        };
        if !current() {
            return Err(unavailable());
        }
        result
    }
}
impl Observation {
    pub(super) fn current(&self, shell: &ShellState, model: &Comments) -> bool {
        self.navigation_current(shell, model) && self.member.current(shell)
    }
    /// Original Canvas read/policy custody, independent of the same selected
    /// profile's ordinary typed member hydration after strict open admission.
    pub(super) fn navigation_current(&self, shell: &ShellState, model: &Comments) -> bool {
        current(
            shell,
            model,
            &self.ticket,
            &self.token,
            &self.root,
            self.socket,
            self.revision,
            &self.authority,
            &self.membership,
            &self.enterprise,
        )
    }
    pub(super) fn proof(&self) -> Value {
        serde_json::json!({"editor":self.member.id,"root":self.root,"member":self.member.raw,
            "team":self.ticket.owner.scope.team,"serial":self.ticket.serial,
            "socket":self.socket,"preferencesRevision":self.revision,
            "userPreferences":self.raw_user,"enterprisePreferences":self.raw_enterprise,
            "teamPreferences":self.raw_teams})
    }
}
pub(super) async fn capture(
    state: Signal<ShellState>,
    model: Signal<Comments>,
    ticket: Ticket,
    root: Message,
    token: Arc<()>,
) -> Result<Observation, Error> {
    let fence = Fence::capture(&state.peek(), &model.peek(), ticket, root, token)?;
    let ctx = fence
        .ticket
        .owner
        .scope
        .request(&state.peek())
        .ok_or_else(unavailable)?;
    let raw_user = fence
        .read(
            state,
            model,
            policy::request::user(&ctx.client, &ctx.session),
        )
        .await?;
    let user = policy::UserPrefs::decode(&raw_user).map_err(|_| unavailable())?;
    let raw_enterprise = if fence.enterprise.is_some() {
        Some(
            fence
                .read(
                    state,
                    model,
                    policy::request::enterprise(&ctx.client, &ctx.session),
                )
                .await?,
        )
    } else {
        None
    };
    let enterprise_prefs = fence
        .enterprise
        .as_ref()
        .zip(raw_enterprise.as_ref())
        .map(|(id, raw)| policy::EnterprisePrefs::decode(id, raw))
        .transpose()
        .map_err(|_| unavailable())?;
    let mut names = policy::Snapshot::new(
        policy::OrdinaryClientV2,
        fence.membership.clone(),
        user,
        enterprise_prefs,
    )
    .map_err(|_| unavailable())?;
    let mut raw_teams = std::collections::BTreeMap::new();
    for id in names.missing_team_preferences() {
        let request =
            policy::request::team_for(&ctx.client, &ctx.session, &id).map_err(|_| unavailable())?;
        let raw = fence.read(state, model, request).await?;
        names
            .observe_team(policy::TeamPrefs::decode(&id, &raw).map_err(|_| unavailable())?)
            .map_err(|_| unavailable())?;
        raw_teams.insert(id, raw);
        if names.should_display_real_names() == Ok(true) {
            break;
        }
    }
    names
        .should_display_real_names()
        .map_err(|_| unavailable())?;
    if !fence.current(&state.peek(), &model.peek()) {
        return Err(unavailable());
    }
    Ok(Observation {
        ticket: fence.ticket,
        token: fence.token,
        root: fence.root,
        member: fence.member,
        socket: fence.socket,
        revision: fence.revision,
        authority: fence.authority,
        membership: fence.membership,
        enterprise: fence.enterprise,
        policy: names,
        raw_user,
        raw_enterprise,
        raw_teams,
    })
}
