//! Closed, disconnected fixture admission. Never a production host constructor.
use super::super::catalog::Choice;
use super::*;
use crate::drafts::store::protected::Review;

pub(crate) struct FixtureContext {
    owner: Owner,
    catalog: Catalog,
    emoji: Arc<Value>,
    transport: Arc<Transport>,
    alive: Arc<AtomicBool>,
}
impl FixtureContext {
    pub(crate) fn new(owner: Owner) -> Result<Self, Failure> {
        let catalog = Catalog::decode(
            owner.clone(),
            serde_json::json!({
                "ok":true,"commands":[{"name":"/example","canonical_name":"/canonical",
                "type":"app","app":"A1","teams":["T1"],"is_runnable_from_thread":true}],
                "app_actions":[],"cache_ts":"slash-owned-fixture"
            }),
        )?;
        Ok(Self {
            owner,
            catalog,
            emoji: Arc::new(serde_json::json!({})),
            transport: Arc::new(Transport::disconnected_fixture(
                "slash-owned-fixture-cookie",
            )),
            alive: Arc::new(AtomicBool::new(true)),
        })
    }
    pub(crate) fn catalog(&self) -> &Catalog {
        &self.catalog
    }
    pub(crate) fn transport(&self) -> &Arc<Transport> {
        &self.transport
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
        choice: &Choice,
        token: &str,
    ) -> Result<ClaimPlan, Failure> {
        let scope = review.scope();
        if !self.alive.load(Ordering::Acquire)
            || scope.account_id != self.owner.account
            || scope.key.team != self.owner.team
            || scope.key.channel != self.owner.channel
            || scope.key.thread != self.owner.thread
        {
            return Err(Failure::Stale);
        }
        let desired = review.row().state().desired().ok_or(Failure::Unsupported)?;
        let paired =
            super::super::paired::derive(&scope.key, review.local(), desired, self.emoji.clone())?;
        let frozen = paired.freeze(
            self.owner.clone(),
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
            session: WorkspaceSession {
                team_id: "T1".into(),
                enterprise_id: None,
                user_id: "U1".into(),
                name: "Slash fixture".into(),
                url: "https://slash-owned-fixture.slack.com".into(),
                token: "slash-owned-fixture-token".into(),
            },
            client: SlackClient::default(),
            alive: self.alive.clone(),
        };
        controller.plan_claim(&paired, &self.emoji, admission, token)
    }
}
impl Drop for FixtureContext {
    fn drop(&mut self) {
        self.revoke();
    }
}
