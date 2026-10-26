//! Nested title members have separate policy/source custody from files.info.
use super::*;
use crate::directory_member_fetches::Context;
use std::collections::BTreeSet;
use super_platinum_core::{
    block_actions::name_policy,
    state::{directory_members::Subset, posting_metadata::authority::Authority},
};

pub(super) struct Policy {
    context: Context,
    authority: Authority,
    revision: u64,
    names: name_policy::Snapshot,
    raw_user: Value,
    raw_enterprise: Option<Value>,
}
impl Policy {
    fn current(&self, s: &ShellState, token: &Token) -> bool {
        token.current(s)
            && self.context.current(s)
            && self.authority.current(&s.core)
            && self.revision == crate::app_home::autocomplete::preferences_revision(s)
    }
}

#[derive(Clone)]
pub(super) struct Members {
    policy: Arc<Policy>,
    subset: Arc<Subset>,
    generation: String,
}
impl Members {
    pub(super) fn capture(
        s: &ShellState,
        token: &Token,
        ids: &BTreeSet<String>,
    ) -> Result<Option<Self>, Error> {
        if ids.is_empty() {
            return Ok(None);
        }
        let policy = s
            .profile_edit
            .sections
            .file_titles
            .as_ref()
            .and_then(|c| c.policy.clone())
            .filter(|p| p.current(s, token))
            .ok_or_else(unavailable)?;
        let w = s
            .core
            .workspaces
            .get(&token.owner.team)
            .ok_or_else(unavailable)?;
        let generation = "profile-canvas-title-users".to_owned();
        let subset = w.directory_members.subset(
            &token.owner.account,
            &policy.context.session,
            policy.context.epoch,
            &generation,
            ids,
            &w.users,
        )?;
        Ok(Some(Self {
            policy,
            subset: Arc::new(subset),
            generation,
        }))
    }
    pub(super) fn current(&self, s: &ShellState, token: &Token) -> bool {
        self.policy.current(s, token)
            && s.core.workspaces.get(&token.owner.team).is_some_and(|w| {
                self.subset.current(
                    &w.directory_members,
                    &token.owner.account,
                    &self.policy.context.session,
                    self.policy.context.epoch,
                    &self.generation,
                    &w.users,
                ) && self.subset.current_rows(
                    &w.directory_members,
                    &token.owner.account,
                    &self.policy.context.session,
                    self.policy.context.epoch,
                    &self.generation,
                    &w.users,
                )
            })
    }
    pub(super) fn label(&self, id: &str) -> Result<String, file_titles::Unavailable> {
        let row = self
            .subset
            .rows()
            .iter()
            .find(|r| r.source["id"] == id)
            .ok_or(file_titles::Unavailable::InvalidMetadata)?;
        // The native application currently renders English copy. No locale is
        // inferred from member metadata or substituted for an unavailable row.
        file_titles::member_label(&row.source, &self.policy.names, "Hidden user")
    }
    pub(super) fn snapshot(&self) -> Value {
        serde_json::json!({"members":self.subset.rows().iter().map(|r|serde_json::json!({"source":r.source,"effective":r.native})).collect::<Vec<_>>(),"policy":{"user":self.policy.raw_user,"enterprise":self.policy.raw_enterprise,"revision":self.policy.revision},"generation":self.generation})
    }
}

pub(in crate::profile_edit::sections::rich) async fn acquire_policy(
    mut state: Signal<ShellState>,
    token: &Token,
    current: impl Fn(&ShellState) -> bool,
) -> Result<(), Error> {
    if state
        .peek()
        .profile_edit
        .sections
        .file_titles
        .as_ref()
        .and_then(|c| c.policy.as_ref())
        .is_some_and(|p| p.current(&state.peek(), token))
    {
        return Ok(());
    }
    let context = Context::capture(&state.peek(), &token.owner.team)?;
    let authority =
        Authority::capture(&state.peek().core, &token.owner.team).map_err(|_| unavailable())?;
    let (membership, enterprise) = authority
        .name_policy_membership(&state.peek().core, name_policy::OrdinaryClientV2)
        .map_err(|_| unavailable())?;
    let revision = crate::app_home::autocomplete::preferences_revision(&state.peek());
    let admitted = || {
        let s = state.peek();
        current(&s)
            && token.current(&s)
            && context.current(&s)
            && authority.current(&s.core)
            && revision == crate::app_home::autocomplete::preferences_revision(&s)
    };
    if !admitted() {
        return Err(unavailable());
    }
    let raw_user = read(
        state,
        &context,
        name_policy::request::user(&context.client, &context.session),
        &admitted,
    )
    .await?;
    let user = name_policy::UserPrefs::decode(&raw_user).map_err(|_| unavailable())?;
    let raw_enterprise = if enterprise.is_some() {
        Some(
            read(
                state,
                &context,
                name_policy::request::enterprise(&context.client, &context.session),
                &admitted,
            )
            .await?,
        )
    } else {
        None
    };
    let org = enterprise
        .as_ref()
        .zip(raw_enterprise.as_ref())
        .map(|(id, raw)| name_policy::EnterprisePrefs::decode(id, raw))
        .transpose()
        .map_err(|_| unavailable())?;
    let names = name_policy::Snapshot::new(name_policy::OrdinaryClientV2, membership, user, org)
        .map_err(|_| unavailable())?;
    if !admitted() {
        return Err(unavailable());
    }
    let policy = Arc::new(Policy {
        context,
        authority,
        revision,
        names,
        raw_user,
        raw_enterprise,
    });
    cache(&mut state.write(), token)?.policy = Some(policy);
    Ok(())
}

async fn read(
    state: Signal<ShellState>,
    context: &Context,
    request: super_platinum_core::slack::PreparedRequest,
    current: &impl Fn() -> bool,
) -> Result<Value, Error> {
    if !current() {
        return Err(unavailable());
    }
    let result = if crate::fixture::is_fixture() {
        if fixture::section_reference::named() {
            fixture::section_reference::rich::policy_read(state, &request).await
        } else {
            fixture::user_title::policy_read(state, &request).await
        }
    } else {
        context.transport.execute_current(request, current).await
    };
    if !current() {
        return Err(unavailable());
    }
    result
}
