use super::*;
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
        let value = self.lease.request(state, request).await?;
        if !self.current(&state.peek()) {
            return Err(unavailable());
        }
        Ok(value)
    }
}
impl Query<'_> {
    pub(super) async fn plan(
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
    pub(super) async fn editor(&self, state: Signal<ShellState>) -> Result<Editor, Error> {
        let mut load = EditorLoad::new(self.original.editor.workflow_id())?;
        while let Some(request) = load.requests().into_iter().next() {
            let raw = self.plan(state, &request.plan).await?;
            load.accept(request.kind, raw)?;
        }
        load.editor()
    }
}
pub(super) async fn acquire(mut state: Signal<ShellState>, id: u64) -> Result<bool, Error> {
    if !authorized(state, id).await {
        return Err(unavailable());
    }
    let (o, lease) = {
        let s = state.peek();
        let a = attempt(&s, id).ok_or_else(unavailable)?;
        (a.original.clone(), a.lease.clone())
    };
    let query = Query {
        original: &o,
        lease: &lease,
        pending: id,
    };
    let policy = policy::acquire(state, &query).await?;
    let fresh = query.editor(state).await?;
    if !exact(&state.peek(), id)
        || !policy.current(&state.peek())
        || !policy.same(&o.policy)
        || !policy.can_create()
        || !o.editor.same_revision(&fresh)
        || !supported_first(&fresh)
        || !allowed_editor(&fresh, &o.owner.ctx.session.user_id)
    {
        return Err(unavailable());
    }
    let apps = query
        .plan(state, &protocol::ConnectorApps::request())
        .await?;
    attempt_mut(&mut state.write(), id)
        .ok_or_else(unavailable)?
        .validation["certifiedApps"] = apps.clone();
    let permissions =
        protocol::ConnectorApps::decode(apps)?.permissions(fresh.raw_workflow(), None)?;
    let inputs = protocol::PublicationInputs::static_inputs(&fresh, &permissions)?;
    let assessment = inputs.assess()?;
    let types = query
        .plan(state, &protocol::TriggerTypes::request(None)?)
        .await?;
    attempt_mut(&mut state.write(), id)
        .ok_or_else(unavailable)?
        .validation["triggerTypes"] = types.clone();
    let available = protocol::TriggerTypes::decode(types)?.availability(fresh.trigger())?;
    if !authorized(state, id).await || !inputs.current(&fresh) {
        return Err(unavailable());
    }
    let ready = assessment.can_be_published && assessment.unavailable_steps.is_empty() && available;
    let mut s = state.write();
    let a = attempt_mut(&mut s, id).ok_or_else(unavailable)?;
    a.validation["status"] = json!(if ready { "ready" } else { "blocked" });
    a.validation["assessment"] = json!({"blockingErrors":assessment.blocking_errors,"hasSteps":assessment.has_steps,
        "publishRestricted":assessment.publish_restricted,"canBePublished":assessment.can_be_published,
        "unavailableSteps":assessment.unavailable_steps,"triggerAvailable":available});
    Ok(ready)
}
