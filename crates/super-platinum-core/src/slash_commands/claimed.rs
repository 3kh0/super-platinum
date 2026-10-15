//! Private managed split. A plan is not a Permit; only the real SQLite Claim arms it.
use super::*;
use crate::drafts::store::protected::{SlashClaim, SlashTarget};

pub(crate) struct ClaimPlan {
    prior: Record,
    next: Record,
    admission: Admission,
    request: std::sync::Mutex<Option<PreparedRequest>>,
}
impl ClaimPlan {
    pub(crate) fn prior(&self) -> &Record {
        &self.prior
    }
    pub(crate) fn next(&self) -> &Record {
        &self.next
    }
    pub(crate) fn current(&self) -> bool {
        self.admission.alive.load(Ordering::Acquire)
    }
    pub(crate) fn finish(&self, report: &Report) -> Result<ManagedTerminal, Failure> {
        managed_terminal::derive_retained(&self.next, report)
    }
    pub(crate) fn accepts(&self, report: &Report) -> bool {
        report.id == self.next.id && report.version == self.next.version
    }
    pub(crate) fn transport(&self) -> &Arc<Transport> {
        &self.admission.transport
    }
}
pub(crate) struct ArmRefused {
    pub(crate) failure: Failure,
    pub(crate) claim: SlashClaim,
}
impl std::fmt::Debug for ArmRefused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ArmRefused(..)")
    }
}
impl Controller {
    /// Uncommitted immutable input. The closed queue reserves this exact record
    /// before claiming; no Permit is issued by staging or journal restoration.
    pub(crate) fn stage_managed(frozen: Frozen, catalog: Catalog) -> Result<Self, Failure> {
        frozen.validate()?;
        if catalog.owner() != &frozen.owner || !catalog.commands().contains(&frozen.command) {
            return Err(Failure::Stale);
        }
        Ok(Self {
            record: Record {
                id: uuid::Uuid::new_v4().to_string(),
                version: 1,
                frozen,
                phase: Phase::Prepared,
                retry_after_secs: None,
                http_status: None,
                retired: false,
            },
            catalog: Some(catalog),
            pending: None,
            recovered: false,
        })
    }
    /// Consumes the staged controller and original sealed admission. There is no
    /// production Admission constructor in this cohort. Native/auth/visit/IME
    /// authority must eventually supply it; paired content alone cannot.
    pub(crate) fn plan_claim(
        self,
        paired: &super::super::paired::Paired,
        emoji: &Arc<Value>,
        admission: Admission,
        token: &str,
    ) -> Result<ClaimPlan, Failure> {
        if self.recovered
            || self.pending.is_some()
            || self.record.retired
            || self.record.phase != Phase::Prepared
        {
            return Err(Failure::NoReplay);
        }
        if admission.intent != self.record.id
            || admission.frozen != self.record.frozen
            || !admission.alive.load(Ordering::Acquire)
            || self
                .catalog
                .as_ref()
                .is_none_or(|c| !c.same_generation(&admission.catalog))
            || !paired.matches_frozen(&self.record.frozen, emoji)
        {
            return Err(Failure::Stale);
        }
        let request = api::prepare(
            &admission.client,
            &admission.session,
            &self.record.frozen,
            token,
        )?;
        let mut next = self.record.clone();
        next.version = next.version.checked_add(1).ok_or(Failure::Invalid)?;
        next.phase = Phase::DispatchPossible;
        Ok(ClaimPlan {
            prior: self.record,
            next,
            admission,
            request: std::sync::Mutex::new(Some(request)),
        })
    }
    /// No Journal parameter and no second CAS. The actual linear storage claim
    /// is consumed exactly once; a refusal returns that same claim to its owner.
    pub(crate) fn arm_claimed(
        plan: &ClaimPlan,
        claim: SlashClaim,
    ) -> Result<(Permit, SlashTarget), ArmRefused> {
        if claim.record() != plan.next() || !claim.stream_live() || !plan.current() {
            return Err(ArmRefused {
                failure: Failure::Stale,
                claim,
            });
        }
        let Some(request) = plan.request.lock().unwrap().take() else {
            return Err(ArmRefused {
                failure: Failure::NoReplay,
                claim,
            });
        };
        let target = claim.into_target();
        Ok((
            Permit {
                id: plan.next.id.clone(),
                version: plan.next.version,
                request,
                transport: plan.admission.transport.clone(),
                alive: plan.admission.alive.clone(),
            },
            target,
        ))
    }
}

#[cfg(test)]
pub(crate) fn fixture_plan(
    review: &crate::drafts::store::protected::Review,
    alive: Arc<AtomicBool>,
) -> ClaimPlan {
    let scope = review.scope();
    let owner = Owner {
        account: scope.account_id.clone(),
        team: scope.key.team.clone(),
        user: "U1".into(),
        channel: scope.key.channel.clone(),
        thread: scope.key.thread.clone(),
    };
    let catalog=Catalog::decode(owner.clone(),serde_json::json!({"ok":true,"commands":[{"name":"/example","canonical_name":"/canonical","type":"app","app":"A1","teams":[scope.key.team],"is_runnable_from_thread":true}],"app_actions":[],"cache_ts":"opaque"})).unwrap();
    let emoji = Arc::new(serde_json::json!({}));
    let paired = super::super::paired::derive(
        &scope.key,
        review.local(),
        review.row().state().desired().unwrap(),
        emoji.clone(),
    )
    .unwrap();
    let original = serde_json::json!({"format":1,"local":review.local(),"desired":review.row().state().desired()});
    let frozen = paired
        .freeze(
            owner,
            review.row().revision(),
            original,
            &emoji,
            &catalog.choice(0).unwrap(),
            &catalog,
        )
        .unwrap();
    let controller = Controller::stage_managed(frozen.clone(), catalog.clone()).unwrap();
    let admission = Admission {
        intent: controller.record.id.clone(),
        frozen,
        catalog,
        transport: Arc::new(Transport::new("fixture-cookie").unwrap()),
        session: WorkspaceSession {
            team_id: scope.key.team.clone(),
            enterprise_id: None,
            user_id: "U1".into(),
            name: "Fixture".into(),
            url: "https://synthetic.slack.com".into(),
            token: "fixture-token".into(),
        },
        client: SlackClient::default(),
        alive,
    };
    controller
        .plan_claim(&paired, &emoji, admission, "fixture-command-token")
        .unwrap()
}
