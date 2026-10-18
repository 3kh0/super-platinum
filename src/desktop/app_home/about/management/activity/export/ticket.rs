use super::*;
#[derive(Clone)]
pub(super) struct Ticket {
    pub(super) owner: Owner,
    pub(super) account: Option<String>,
    pub(super) visit: u64,
    pub(super) document: u64,
    pub(super) workflow: String,
    revision: u64,
    editor: Editor,
    title: String,
    description: String,
    pending: Option<u64>,
    credentials: String,
    pub(super) permission: policy::Snapshot,
    pub(super) filters: Filters,
    search: String,
    generation: u64,
}
impl Ticket {
    pub(super) fn capture(s: &ShellState, visit: u64) -> Option<Self> {
        if !permitted(s, visit) {
            return None;
        }
        let h = s.app_home.about.management.surface.as_ref()?;
        let permission = h.policy.clone().filter(|p| p.can_export_activity())?;
        let d = document(s, visit).filter(|d| d.activity.active)?;
        if h.read.is_some()
            || d.activity.member_composing
            || d.activity.member_pending.is_some()
            || blocked(s, d)
            || s.app_home.about.management.exports.len() >= MAX_EXPORTS
        {
            return None;
        }
        Some(Self {
            owner: h.owner.clone(),
            account: d.account.clone(),
            visit,
            document: d.id,
            workflow: d.editor.workflow_id().into(),
            revision: d.revision,
            editor: d.editor.clone(),
            title: d.title.clone(),
            description: d.description.clone(),
            pending: d.pending,
            credentials: serde_json::to_string(s.core.session.as_ref()?).ok()?,
            permission,
            filters: d.activity.filters.clone(),
            search: d.activity.user_search.clone(),
            generation: d.activity.member_generation,
        })
    }
    pub(super) fn source_current(&self, s: &ShellState) -> bool {
        self.owner.current(s)
            && visible(s, self.visit)
            && s.overlay.is_none()
            && s.core
                .session
                .as_ref()
                .and_then(|v| serde_json::to_string(v).ok())
                .as_ref()
                == Some(&self.credentials)
            && document(s, self.visit).is_some_and(|d| {
                d.id == self.document
                    && d.account == self.account
                    && d.revision == self.revision
                    && d.editor.same_revision(&self.editor)
                    && d.title == self.title
                    && d.description == self.description
                    && d.pending == self.pending
                    && d.activity.active
                    && d.activity.filters == self.filters
                    && d.activity.user_search == self.search
                    && d.activity.member_generation == self.generation
                    && !d.activity.member_composing
                    && d.activity.member_pending.is_none()
            })
    }
    pub(super) fn current(&self, s: &ShellState) -> bool {
        self.source_current(s)
            && permitted(s, self.visit)
            && s.app_home
                .about
                .management
                .surface
                .as_ref()
                .and_then(|h| h.policy.as_ref())
                .is_some_and(|p| p.same(&self.permission) && p.can_export_activity())
    }
    pub(super) fn expected(&self) -> Value {
        let mut filters = json!(self.filters);
        filters["revision"] = json!(self.filters.revision.to_string());
        json!({"visit":self.visit.to_string(),"document":self.document.to_string(),"documentRevision":self.revision.to_string(),"filters":filters,"search":self.search,"memberGeneration":self.generation.to_string()})
    }
}
pub(super) struct Preparation {
    pub(super) ticket: Ticket,
    pub(super) lease: read::Lease,
    pub(super) native: native::Guard,
}
impl Preparation {
    pub(super) fn preparing(&self, s: &ShellState) -> bool {
        self.lease.current(s)
            && self.ticket.current(s)
            && s.app_home
                .about
                .management
                .exports
                .iter()
                .any(|a| a.id == self.lease.id && a.phase == Phase::Preparing)
    }
    pub(super) fn cleanup_current(&self, s: &ShellState) -> bool {
        self.ticket.source_current(s)
            && s.app_home
                .about
                .management
                .surface
                .as_ref()
                .is_some_and(|h| h.id == self.ticket.visit && h.read == Some(self.lease.id))
    }
    pub(super) async fn retired(&self, state: Signal<ShellState>) {
        loop {
            if !self.preparing(&state.peek()) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
}
impl policy::Query for Preparation {
    fn owner(&self) -> &Owner {
        &self.ticket.owner
    }
    fn current(&self, s: &ShellState) -> bool {
        self.preparing(s)
    }
    async fn request(
        &self,
        mut state: Signal<ShellState>,
        request: PreparedRequest,
    ) -> Result<Value, Error> {
        if !self.preparing(&state.peek()) {
            return Err(unavailable());
        }
        let guard = self.native.lock().await.ok_or_else(unavailable)?;
        if !self.preparing(&state.peek()) {
            return Err(unavailable());
        }
        let synthetic = super::super::super::fixture::enabled(&state.peek());
        let result = if synthetic {
            let result = super::super::super::fixture::response(&mut state.write(), &request);
            drop(guard);
            result
        } else if crate::fixture::is_fixture() {
            drop(guard);
            Err(unavailable())
        } else {
            drop(guard);
            self.ticket.owner.ctx.transport.execute(request).await
        };
        if let Err(Error::RateLimited { retry_after_secs }) = &result {
            #[allow(deprecated)]
            navigation::retain_limit(
                &mut state.write_silent(),
                &self.ticket.owner.ctx,
                *retry_after_secs,
            );
        }
        if !self.preparing(&state.peek()) {
            return Err(unavailable());
        }
        result
    }
}
