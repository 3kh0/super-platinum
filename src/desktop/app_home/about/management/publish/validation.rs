use super::*;
// The shared Lease fences the surface/credentials; publication also retains
// the exact document and editor throughout every prerequisite read.
pub(super) struct Query<'a> {
    pub original: &'a Original,
    pub lease: &'a read::Lease,
    pub pending: u64,
}
impl policy::Query for Query<'_> {
    fn owner(&self) -> &Owner {
        &self.lease.owner
    }
    fn current(&self, s: &ShellState) -> bool {
        self.lease.current(s) && self.original.current(s, Some(self.pending))
    }
    async fn request(
        &self,
        state: Signal<ShellState>,
        request: super_platinum_core::slack::PreparedRequest,
    ) -> Result<Value, Error> {
        if !self.current(&state.peek()) {
            return Err(unavailable());
        }
        let result = self.lease.request(state, request).await;
        if !self.current(&state.peek()) {
            return Err(unavailable());
        }
        result
    }
}
impl Query<'_> {
    pub async fn plan(
        &self,
        state: Signal<ShellState>,
        plan: &protocol::RequestPlan,
    ) -> Result<Value, Error> {
        policy::Query::request(
            self,
            state,
            plan.request(&self.lease.owner.ctx.client, &self.lease.owner.ctx.session),
        )
        .await
    }
    pub async fn editor(&self, state: Signal<ShellState>) -> Result<Editor, Error> {
        let mut load = EditorLoad::new(self.original.editor.workflow_id())?;
        while let Some(request) = load.requests().into_iter().next() {
            let raw = self.plan(state, &request.plan).await?;
            load.accept(request.kind, raw)?;
        }
        load.editor()
    }
}
#[derive(Clone)]
pub(super) struct Facts {
    pub status: &'static str,
    pub certified_apps: Option<Value>,
    pub trigger_types: Option<Value>,
    pub assessment: Option<Value>,
    pub notice: Option<&'static str>,
}
impl Default for Facts {
    fn default() -> Self {
        Self {
            status: "pending",
            certified_apps: None,
            trigger_types: None,
            assessment: None,
            notice: None,
        }
    }
}
impl Facts {
    pub fn value(&self) -> Value {
        json!({"status":self.status,"certifiedApps":self.certified_apps,"triggerTypes":self.trigger_types,
            "assessment":self.assessment,"notice":self.notice})
    }
}
pub(super) async fn acquire(mut state: Signal<ShellState>, id: u64) -> Result<bool, Error> {
    let (original, lease) = {
        let s = state.peek();
        let a = attempt(&s, id).ok_or_else(unavailable)?;
        (a.original.clone(), a.lease.clone())
    };
    if !exact(&state.peek(), id) || !native::current(&original).await || !exact(&state.peek(), id) {
        return Err(unavailable());
    }
    let query = Query {
        original: &original,
        lease: &lease,
        pending: id,
    };
    let policy = policy::acquire(state, &query).await?;
    let fresh = query.editor(state).await?;
    if !exact(&state.peek(), id)
        || !policy.current(&state.peek())
        || !policy.same(&original.policy)
        || !policy.can_create()
        || !supported(&fresh)
        || !allowed_editor(&fresh, &lease.owner.ctx.session.user_id)
        || !original.editor.same_revision(&fresh)
    {
        return Err(unavailable());
    }
    let raw = query
        .plan(state, &protocol::ConnectorApps::request())
        .await?;
    attempt_mut(&mut state.write(), id)
        .ok_or_else(unavailable)?
        .validation
        .certified_apps = Some(raw.clone());
    let connectors = protocol::ConnectorApps::decode(raw)?;
    // This first supported path proves all functions builtin. If an actual app
    // needs unacquired visibility, the existing decoder returns unavailable.
    let permissions = connectors.permissions(fresh.raw_workflow(), None)?;
    let inputs = protocol::PublicationInputs::static_inputs(&fresh, &permissions)?;
    let assessment = inputs.assess()?;
    let raw = query
        .plan(state, &protocol::TriggerTypes::request(None)?)
        .await?;
    attempt_mut(&mut state.write(), id)
        .ok_or_else(unavailable)?
        .validation
        .trigger_types = Some(raw.clone());
    let types = protocol::TriggerTypes::decode(raw)?;
    let trigger_available = types.availability(fresh.trigger())?;
    if !inputs.current(&fresh)
        || !exact(&state.peek(), id)
        || !native::current(&original).await
        || !exact(&state.peek(), id)
    {
        return Err(unavailable());
    }
    let ready =
        assessment.can_be_published && assessment.unavailable_steps.is_empty() && trigger_available;
    let value = json!({"blockingErrors":assessment.blocking_errors,"hasSteps":assessment.has_steps,
        "publishRestricted":assessment.publish_restricted,"unchangedPublished":assessment.unchanged_published,
        "canBePublished":assessment.can_be_published,"unavailableSteps":assessment.unavailable_steps,
        "triggerAvailable":trigger_available});
    let mut s = state.write();
    let a = attempt_mut(&mut s, id).ok_or_else(unavailable)?;
    a.validation.assessment = Some(value);
    a.validation.status = if ready { "ready" } else { "blocked" };
    a.validation.notice = (!ready).then_some(
        "Publication is blocked by the current workflow checks. No publication was sent.",
    );
    Ok(ready)
}
