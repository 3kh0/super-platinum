use super::*;

pub(super) struct Group {
    requested: Vec<String>,
    pending_subscribed: Vec<String>,
    unsubscribe: VecDeque<Vec<String>>,
    subscribe: Option<Vec<String>>,
    pub(super) deadline: u64,
    issued: bool,
}
pub(super) struct Running {
    pub(super) ticket: Cycle,
    initial: VecDeque<Vec<String>>,
    cursor: usize,
    pub(super) group: Option<Group>,
    pub(super) operation: Option<(Operation, bool)>,
    detached: bool,
    pub(super) done: bool,
}
impl Running {
    pub(super) fn new(ticket: Cycle, initial: Vec<String>) -> Self {
        Self {
            ticket,
            initial: chunks(&initial),
            cursor: 0,
            group: None,
            operation: None,
            detached: false,
            done: false,
        }
    }
    pub(super) fn retire_into(
        self,
        pending: &mut Vec<String>,
        stale: &mut Vec<String>,
        subscribed: &[String],
    ) {
        for id in &self.ticket.ids {
            if !subscribed.contains(id) {
                add(pending, id);
            }
        }
        for batch in self.initial {
            for id in batch {
                add(stale, &id);
            }
        }
        if self.group.is_none() {
            if let Some((op, _)) = self.operation {
                if op.kind == Kind::Unsubscribe {
                    for id in op.ids {
                        add(stale, &id);
                    }
                }
            }
        }
    }
}

/// p3xJ/HyoU positive 0..10% jitter, including source floating upper epsilon.
fn jitter(delay: u64, entropy: f64) -> Result<u64, Error> {
    if !entropy.is_finite() || !(0.0..1.0).contains(&entropy) {
        return Err(unavailable());
    }
    if delay == 0 {
        return Ok(0);
    }
    let text = if entropy == 0.0 {
        "0".into()
    } else if entropy < 0.000001 {
        format!("{entropy:e}")
    } else {
        entropy.to_string()
    };
    let epsilon = format!("1e-{}", text.len() - 1)
        .parse::<f64>()
        .map_err(|_| unavailable())?;
    let random = (entropy * (0.1 + epsilon)).min(0.1);
    Ok((delay as f64 + delay as f64 * random).ceil() as u64)
}
impl Manager {
    fn finish_group(&mut self, index: usize) {
        if let Some(group) = self.cycles[index].group.take() {
            // Deliberately source finally semantics, also after failed issued
            // subscribe. This set is not a receipt or confirmed remote state.
            self.subscribed = group.pending_subscribed;
        }
    }
    /// Returns a reserved operation once due; repeated calls retain its exact
    /// token. Call started only when the native driver actually dispatches it.
    pub fn next_operation(
        &mut self,
        scope: &Scope,
        cycle: &Cycle,
        now: u64,
        entropy: f64,
    ) -> Result<Option<Operation>, Error> {
        valid_time(now)?;
        // Validate caller entropy before consuming any owned queue element.
        jitter(0, entropy)?;
        let index = self.index(scope, cycle)?;
        loop {
            if let Some((op, issued)) = &self.cycles[index].operation {
                return Ok((!*issued && op.due <= now).then(|| op.clone()));
            }
            if self.cycles[index].done {
                return Ok(None);
            }
            let candidate = if let Some(group) = &mut self.cycles[index].group {
                if let Some(ids) = group.unsubscribe.pop_front() {
                    Some((Kind::Unsubscribe, ids, Some(group.deadline)))
                } else if let Some(ids) = group.subscribe.take() {
                    Some((Kind::Subscribe, ids, Some(group.deadline)))
                } else {
                    self.finish_group(index);
                    None
                }
            } else if let Some(ids) = self.cycles[index].initial.pop_front() {
                Some((Kind::Unsubscribe, ids, None))
            } else if self.cycles[index].cursor < cycle.ids.len() && !self.cycles[index].detached {
                let running = &mut self.cycles[index];
                let end = (running.cursor + MAX_BATCH).min(cycle.ids.len());
                let requested = cycle.ids[running.cursor..end].to_vec();
                running.cursor = end;
                if cycle.reconnect {
                    Some((Kind::Subscribe, requested, None))
                } else {
                    let subscribe: Vec<_> = requested
                        .iter()
                        .filter(|id| !self.subscribed.contains(id))
                        .cloned()
                        .collect();
                    let mut pending_subscribed = requested.clone();
                    for id in &self.subscribed {
                        add(&mut pending_subscribed, id);
                    }
                    running.group = Some(Group {
                        requested,
                        pending_subscribed,
                        unsubscribe: chunks(&self.stale),
                        subscribe: (!subscribe.is_empty()).then_some(subscribe),
                        deadline: now + TIMEOUT_MS,
                        issued: false,
                    });
                    None
                }
            } else {
                self.cycles[index].done = true;
                return Ok(None);
            };
            if let Some((kind, ids, deadline)) = candidate {
                let delay = self
                    .last_call
                    .saturating_add(OPERATION_CADENCE_MS)
                    .saturating_sub(now);
                let due = now + jitter(delay, entropy)?;
                let op = Operation {
                    token: Arc::new(()),
                    cycle: cycle.token.clone(),
                    kind,
                    ids,
                    due,
                    deadline,
                };
                self.cycles[index].operation = Some((op.clone(), false));
                return Ok((due <= now).then_some(op));
            }
        }
    }
    pub fn next_operation_due_ms(
        &self,
        scope: &Scope,
        cycle: &Cycle,
    ) -> Result<Option<u64>, Error> {
        Ok(self.cycles[self.index(scope, cycle)?]
            .operation
            .as_ref()
            .and_then(|(op, issued)| (!issued).then_some(op.due)))
    }
    /// Zero means synchronous progress/cleanup is available. Issued work is
    /// driven by its completion or original deadline, never a polling tick.
    pub fn next_wake_ms(&self) -> Option<u64> {
        let mut wake = self.due;
        let mut include = |value| wake = Some(wake.map_or(value, |old| old.min(value)));
        for running in &self.cycles {
            if !running.detached && !running.ticket.reconnect {
                include(running.ticket.parent_deadline);
            }
            if let Some(group) = &running.group {
                if group.issued {
                    include(group.deadline);
                }
            }
            match &running.operation {
                Some((op, false)) => include(op.due),
                Some((_, true)) => {}
                None => include(0),
            }
        }
        wake
    }
    pub fn started(&mut self, scope: &Scope, op: &Operation, now: u64) -> Result<(), Error> {
        valid_time(now)?;
        if !self.operation_current(scope, op)
            || now < op.due
            || op.deadline.is_some_and(|v| now >= v)
        {
            return Err(unavailable());
        }
        let running = self
            .cycles
            .iter_mut()
            .find(|v| Arc::ptr_eq(&v.ticket.token, &op.cycle))
            .unwrap();
        let (_, issued) = running.operation.as_mut().unwrap();
        if *issued {
            return Err(unavailable());
        }
        *issued = true;
        if let Some(group) = &mut running.group {
            group.issued = true;
        }
        self.last_call = now;
        Ok(())
    }
    /// Actual source completion, not local refusal or owner cancellation.
    pub fn settle(
        &mut self,
        scope: &Scope,
        op: &Operation,
        now: u64,
        outcome: Outcome,
    ) -> Result<(), Error> {
        valid_time(now)?;
        if !self.operation_current(scope, op) {
            return Err(unavailable());
        }
        let index = self
            .cycles
            .iter()
            .position(|v| Arc::ptr_eq(&v.ticket.token, &op.cycle))
            .unwrap();
        if !self.cycles[index].operation.as_ref().unwrap().1 {
            return Err(unavailable());
        }
        self.cycles[index].operation = None;
        if op.kind == Kind::Unsubscribe && outcome == Outcome::Failure {
            for id in &op.ids {
                add(&mut self.stale, id);
            }
        }
        if op.kind == Kind::Subscribe {
            if self.cycles[index].ticket.reconnect {
                if outcome == Outcome::Failure {
                    self.cycles[index].done = true;
                }
            } else {
                self.finish_group(index);
            }
        }
        Ok(())
    }
    /// Per-requested-chunk timeout; the captured pending set is installed after
    /// issued work even if no ACK or row arrived. Future late receipts refuse.
    /// A wholly unsent group must instead be returned locally via return_unsent.
    pub fn timeout_chunk(&mut self, scope: &Scope, cycle: &Cycle, now: u64) -> Result<(), Error> {
        valid_time(now)?;
        let index = self.index(scope, cycle)?;
        let group = self.cycles[index].group.as_ref().ok_or_else(unavailable)?;
        if !group.issued || now < group.deadline {
            return Err(unavailable());
        }
        self.cycles[index].operation = None;
        self.finish_group(index);
        Ok(())
    }
    /// Returns whether the source parent-finally notification is due. Parent
    /// timeout does not cancel an issued current chunk: keep driving that token
    /// under its original native guards, then finish Completed for cleanup.
    /// Native adaptation: future/wholly unsent chunks requeue without a grant.
    pub fn finish_parent(
        &mut self,
        scope: &Scope,
        cycle: &Cycle,
        now: u64,
        outcome: ParentOutcome,
    ) -> Result<bool, Error> {
        valid_time(now)?;
        let index = self.index(scope, cycle)?;
        if cycle.reconnect {
            if outcome != ParentOutcome::Completed || !self.cycles[index].done {
                return Err(unavailable());
            }
            self.cycles.remove(index);
            return Ok(false);
        }
        if outcome == ParentOutcome::Completed {
            if !self.cycles[index].done {
                return Err(unavailable());
            }
            let running = self.cycles.remove(index);
            if running.detached {
                return Ok(false);
            }
            self.pending.retain(|id| !cycle.ids.contains(id));
        } else {
            if now < cycle.parent_deadline || self.cycles[index].detached {
                return Err(unavailable());
            }
            let running = &mut self.cycles[index];
            running.detached = true;
            for id in &cycle.ids[running.cursor..] {
                add(&mut self.pending, id);
            }
            let keep_group = running.group.as_ref().is_some_and(|g| g.issued);
            let keep_initial = running.group.is_none()
                && running
                    .operation
                    .as_ref()
                    .is_some_and(|(_, issued)| *issued);
            if !keep_group {
                if let Some(group) = running.group.take() {
                    for id in group.requested {
                        add(&mut self.pending, &id);
                    }
                }
            }
            if !keep_group && !keep_initial {
                // Re-arm unsent initial unsubscriptions too; no remote claim.
                for batch in &running.initial {
                    for id in batch {
                        add(&mut self.stale, id);
                    }
                }
                if let Some((op, _)) = &running.operation {
                    if op.kind == Kind::Unsubscribe {
                        for id in &op.ids {
                            add(&mut self.stale, id);
                        }
                    }
                }
                self.cycles.remove(index);
            } else {
                // Initial unsubscribe snapshots not yet issued remain stale.
                for batch in running.initial.drain(..) {
                    for id in batch {
                        add(&mut self.stale, &id);
                    }
                }
            }
        }
        if self
            .parent
            .as_ref()
            .is_some_and(|v| Arc::ptr_eq(v, &cycle.token))
        {
            self.parent = None;
        }
        self.schedule(now);
        Ok(true)
    }
    /// Native admission/ownership refusal. Drops local tokens only, preserves
    /// pending IDs and makes no source failure, ACK or remote-unsubscribe claim.
    /// Deliberately does not auto-arm another retry on the same refusal.
    pub fn return_unsent(&mut self, scope: &Scope, cycle: &Cycle) -> Result<(), Error> {
        let index = self.index(scope, cycle)?;
        self.cycles
            .remove(index)
            .retire_into(&mut self.pending, &mut self.stale, &self.subscribed);
        if self
            .parent
            .as_ref()
            .is_some_and(|v| Arc::ptr_eq(v, &cycle.token))
        {
            self.parent = None;
            self.due = None;
        }
        Ok(())
    }
    pub fn resume(
        &mut self,
        scope: &Scope,
        now: u64,
        observation: Observation,
    ) -> Result<(), Error> {
        self.observe(scope, now, observation)?;
        self.schedule(now);
        Ok(())
    }
}
