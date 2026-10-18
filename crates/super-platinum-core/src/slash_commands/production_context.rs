//! Captured production discovery identity; no caller-supplied command catalog.
//! This proves content/session correspondence, not a native composer lease.
use super::*;
use crate::drafts::store::protected::Review;

pub(crate) enum DiscoveryFailure {
    Admission(Failure),
    Transport(crate::slack::Error),
}

pub(crate) struct ProductionContext {
    catalog: Catalog,
    emoji: Arc<Value>,
    transport: Arc<Transport>,
    session: WorkspaceSession,
    client: SlackClient,
    alive: Arc<AtomicBool>,
}
impl ProductionContext {
    #[cfg(test)]
    pub(crate) fn synthetic_response(
        owner: Owner,
        session: WorkspaceSession,
        transport: Arc<Transport>,
        raw: Value,
    ) -> Result<Self, Failure> {
        Self::from_response(
            owner,
            session,
            SlackClient::default(),
            transport,
            Arc::new(serde_json::json!({})),
            raw,
        )
    }
    pub(crate) async fn discover(
        owner: Owner,
        session: WorkspaceSession,
        client: SlackClient,
        transport: Arc<Transport>,
        emoji: Arc<Value>,
    ) -> Result<Self, DiscoveryFailure> {
        Self::validate(&owner, &session, &emoji).map_err(DiscoveryFailure::Admission)?;
        let raw = transport
            .execute(api::discovery(&client, &session))
            .await
            .map_err(DiscoveryFailure::Transport)?;
        Self::from_response(owner, session, client, transport, emoji, raw)
            .map_err(DiscoveryFailure::Admission)
    }
    fn validate(owner: &Owner, session: &WorkspaceSession, emoji: &Value) -> Result<(), Failure> {
        owner.validate()?;
        if owner.team != session.team_id
            || owner.user != session.user_id
            || session.token.is_empty()
            || !emoji.is_object()
            || !bounded(emoji, 2 * 1024 * 1024)
        {
            return Err(Failure::Stale);
        }
        Ok(())
    }
    // Private decoding boundary: production reaches it only after actual
    // client.appCommands. Unit tests may supply synthetic retained responses.
    fn from_response(
        owner: Owner,
        session: WorkspaceSession,
        client: SlackClient,
        transport: Arc<Transport>,
        emoji: Arc<Value>,
        raw: Value,
    ) -> Result<Self, Failure> {
        Self::validate(&owner, &session, &emoji)?;
        Ok(Self {
            catalog: Catalog::decode(owner, raw)?,
            emoji,
            transport,
            session,
            client,
            alive: Arc::new(AtomicBool::new(true)),
        })
    }
    pub(crate) fn catalog(&self) -> &Catalog {
        &self.catalog
    }
    pub(crate) fn is_live(&self) -> bool {
        self.alive.load(Ordering::Acquire)
    }
    pub(crate) fn revoke(&self) {
        self.alive.store(false, Ordering::Release);
    }
    pub(crate) fn plan(
        &self,
        review: &Review,
        choice: &super::super::catalog::Choice,
        token: &str,
    ) -> Result<ClaimPlan, Failure> {
        let scope = review.scope();
        let owner = self.catalog.owner();
        if !self.is_live()
            || scope.account_id != owner.account
            || scope.key.team != owner.team
            || scope.key.channel != owner.channel
            || scope.key.thread != owner.thread
        {
            return Err(Failure::Stale);
        }
        let desired = review.row().state().desired().ok_or(Failure::Unsupported)?;
        let paired =
            super::super::paired::derive(&scope.key, review.local(), desired, self.emoji.clone())?;
        let frozen = paired.freeze(
            owner.clone(),
            review.row().revision(),
            serde_json::json!({"format":1,"local":review.local(),"desired":desired}),
            &self.emoji,
            choice,
            &self.catalog,
        )?;
        let controller = Controller::stage_managed(frozen.clone(), self.catalog.clone())?;
        let admission = Admission {
            intent: controller.record.id.clone(),
            frozen,
            catalog: self.catalog.clone(),
            transport: self.transport.clone(),
            session: self.session.clone(),
            client: self.client.clone(),
            alive: self.alive.clone(),
        };
        controller.plan_claim(&paired, &self.emoji, admission, token)
    }
}
impl Drop for ProductionContext {
    fn drop(&mut self) {
        self.revoke();
    }
}
