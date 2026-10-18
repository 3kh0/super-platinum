//! Refresh names on retained details without re-fetching the execution or
//! borrowing picker search authority. Native custody precedes every render.
use super::*;
use super_platinum_core::slack::PreparedRequest;
pub(super) mod native;
#[cfg(test)]
mod tests;

const FAILURE: &str =
    "Names could not be refreshed. Close the run details and reopen them to try again.";

#[derive(Clone, PartialEq, Eq)]
pub(super) struct Key {
    detail: u64,
    policy: u64,
}

#[derive(Clone)]
struct Ticket {
    owner: Owner,
    visit: u64,
    document: u64,
    revision: u64,
    editor: Editor,
    title: String,
    description: String,
    pending: Option<u64>,
    credentials: String,
    permission: policy::Snapshot,
    key: Key,
    row: Value,
    detail: ActivityDetail,
    filters: Filters,
    breadcrumbs: Vec<Value>,
    search: String,
    member_generation: u64,
}
impl Ticket {
    fn capture(s: &ShellState) -> Option<Self> {
        let h = s.app_home.about.management.surface.as_ref()?;
        if !permitted(s, h.id) || h.read.is_some() {
            return None;
        }
        let d = document(s, h.id).filter(|d| d.activity.active)?;
        let a = &d.activity;
        let detail = a.detail.as_ref().filter(|v| reads::detail_uses_names(v))?;
        let key = Key {
            detail: a.detail_revision,
            policy: s.app_home.workflow_names_revision,
        };
        if a.member_composing
            || a.member_pending.is_some()
            || a.name_refresh_attempted.as_ref() == Some(&key)
            || members::names::observed_policy(s, h.id).is_some()
        {
            return None;
        }
        Some(Self {
            owner: h.owner.clone(),
            visit: h.id,
            document: d.id,
            revision: d.revision,
            editor: d.editor.clone(),
            title: d.title.clone(),
            description: d.description.clone(),
            pending: d.pending,
            credentials: serde_json::to_string(s.core.session.as_ref()?).ok()?,
            permission: h.policy.clone()?,
            key,
            row: a.detail_row.clone()?,
            detail: detail.clone(),
            filters: a.filters.clone(),
            breadcrumbs: a.breadcrumbs.clone(),
            search: a.user_search.clone(),
            member_generation: a.member_generation,
        })
    }
    fn source_current(&self, s: &ShellState) -> bool {
        self.owner.current(s)
            && visible(s, self.visit)
            && s.overlay.is_none()
            && s.core
                .session
                .as_ref()
                .and_then(|v| serde_json::to_string(v).ok())
                .as_ref()
                == Some(&self.credentials)
            && s.app_home
                .about
                .management
                .surface
                .as_ref()
                .is_some_and(|h| {
                    h.policy
                        .as_ref()
                        .is_some_and(|p| p.current(s) && p.can_enter() && p.same(&self.permission))
                })
            && document(s, self.visit).is_some_and(|d| {
                let a = &d.activity;
                d.id == self.document
                    && d.revision == self.revision
                    && d.editor.same_revision(&self.editor)
                    && d.title == self.title
                    && d.description == self.description
                    && d.pending == self.pending
                    && a.active
                    && a.detail_revision == self.key.detail
                    && a.detail_row.as_ref() == Some(&self.row)
                    && a.detail.as_ref() == Some(&self.detail)
                    && a.filters == self.filters
                    && a.breadcrumbs == self.breadcrumbs
                    && a.user_search == self.search
                    && a.member_generation == self.member_generation
                    && !a.member_composing
                    && a.member_pending.is_none()
            })
    }
    fn current(&self, s: &ShellState) -> bool {
        self.source_current(s) && s.app_home.workflow_names_revision == self.key.policy
    }
    fn expected(&self) -> Value {
        let mut filters = json!(self.filters);
        filters["revision"] = json!(self.filters.revision.to_string());
        json!({"visit":self.visit.to_string(),"document":self.document.to_string(),
            "documentRevision":self.revision.to_string(),"detailRevision":self.key.detail.to_string(),
            "filters":filters,"search":self.search,"memberGeneration":self.member_generation.to_string()})
    }
}

pub(in crate::app_home::about::management::activity) struct Attempt {
    ticket: Ticket,
    read: reads::Read,
    native: native::Guard,
}
impl Attempt {
    pub(super) fn owner(&self) -> &Owner {
        &self.ticket.owner
    }
    pub(super) fn visit(&self) -> u64 {
        self.ticket.visit
    }
    pub(super) fn document(&self) -> u64 {
        self.ticket.document
    }
    pub(super) fn current(&self, s: &ShellState) -> bool {
        self.ticket.current(s)
            && self.read.current(s)
            && document(s, self.visit())
                .is_some_and(|d| d.activity.name_refresh_read == Some(self.read.id()))
    }
    pub(super) async fn mutation_guard(
        &self,
        state: Signal<ShellState>,
    ) -> Result<native::Release, Error> {
        if !self.current(&state.peek()) {
            return Err(unavailable());
        }
        let guard = self.native.lock().await.ok_or_else(unavailable)?;
        if !self.current(&state.peek()) {
            return Err(unavailable());
        }
        Ok(guard)
    }
    pub(super) async fn request_prepared(
        &self,
        mut state: Signal<ShellState>,
        request: PreparedRequest,
    ) -> Result<Value, Error> {
        // The original controls must still belong to this capture before each
        // dispatch; no controls stay locked while waiting for the response.
        drop(self.mutation_guard(state).await?);
        let synthetic = fixture::enabled(&state.peek());
        let work = async {
            if synthetic {
                fixture::name_request(state, &request).await
            } else if crate::fixture::is_fixture() {
                Err(unavailable())
            } else {
                self.owner().ctx.transport.execute(request).await
            }
        };
        let result = tokio::select! {
            biased;
            _ = self.retired(state) => Err(unavailable()),
            value = work => value,
        };
        if let Err(Error::RateLimited { retry_after_secs }) = &result {
            // Transport policy is internal bookkeeping, not UI publication.
            #[allow(deprecated)]
            navigation::retain_limit(
                &mut state.write_silent(),
                &self.owner().ctx,
                *retry_after_secs,
            );
        }
        if !self.current(&state.peek()) {
            return Err(unavailable());
        }
        drop(self.mutation_guard(state).await?);
        result
    }
    async fn retired(&self, state: Signal<ShellState>) {
        loop {
            if !self.current(&state.peek()) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
}

pub(crate) fn policy_revision(s: &ShellState) -> u64 {
    s.app_home.workflow_names_revision
}

/// Called synchronously after the accepted realtime event releases its borrow.
/// Invalidations while a refresh is pending are coalesced by its final cleanup.
pub(crate) fn after_event(state: Signal<ShellState>, previous_revision: u64) {
    if state.peek().app_home.workflow_names_revision != previous_revision {
        request(state);
    }
}
pub(super) fn request(mut state: Signal<ShellState>) {
    let Some(ticket) = Ticket::capture(&state.peek()) else {
        return;
    };
    let trace = fixture::enabled(&state.peek());
    dioxus::core::spawn_forever(async move {
        let mut expected = ticket.expected();
        expected["trace"] = json!(trace);
        if trace {
            eprintln!(
                "activity-refresh-phase capture policy={}",
                ticket.key.policy
            );
        }
        let Some((native, initial)) = native::capture(&expected).await else {
            if trace {
                eprintln!(
                    "activity-refresh-phase capture-refused policy={}",
                    ticket.key.policy
                );
            }
            return;
        };
        let read = {
            if !ticket.current(&state.peek()) {
                return;
            }
            let mut s = state.write();
            if !ticket.current(&s) {
                return;
            }
            let Some(read) = reads::Read::begin_refresh(&mut s, ticket.visit) else {
                return;
            };
            let a = &mut document_mut(&mut s, ticket.visit).unwrap().activity;
            a.name_refresh_attempted = Some(ticket.key.clone());
            a.name_refresh_read = Some(read.id());
            a.name_notice = None;
            read
        };
        let activated = native.activate(read.id()).await;
        drop(initial);
        let attempt = Attempt {
            ticket,
            read,
            native,
        };
        let result = if activated {
            members::names::capture_refresh(state, &attempt)
                .await
                .map(|_| ())
        } else {
            Err(unavailable())
        };
        // A newer policy revision can retire the request while its native
        // document remains intact. Release that read before scheduling latest.
        let guard = attempt.native.lock().await;
        if trace {
            eprintln!(
                "activity-refresh-phase cleanup policy={} guard={} source={} current={} result_ok={}",
                attempt.ticket.key.policy,
                guard.is_some(),
                attempt.ticket.source_current(&state.peek()),
                attempt.ticket.current(&state.peek()),
                result.is_ok()
            );
        }
        let follow_up = if guard.is_some() && attempt.ticket.source_current(&state.peek()) {
            let mut s = state.write();
            let same_source = attempt.ticket.source_current(&s);
            let owned = s
                .app_home
                .about
                .management
                .surface
                .as_ref()
                .is_some_and(|h| h.id == attempt.visit() && h.read == Some(attempt.read.id()));
            if same_source && owned && attempt.ticket.current(&s) && result.is_err() {
                document_mut(&mut s, attempt.visit())
                    .unwrap()
                    .activity
                    .name_notice = Some(FAILURE.into());
            }
            attempt.read.release(&mut s);
            same_source && owned && s.app_home.workflow_names_revision != attempt.ticket.key.policy
        } else {
            // Retiring an internal read must not render over a native buffer
            // that refused custody, nor touch a successor's lease or document.
            #[allow(deprecated)]
            attempt.read.release(&mut state.write_silent());
            false
        };
        // Drop only enqueues native unlock; its paint can finish after the
        // next Rust render. Await terminal restoration before a fresh capture.
        let closed = match guard {
            Some(guard) => guard.close().await,
            None => false,
        };
        if trace {
            eprintln!(
                "activity-refresh-phase cleaned policy={} follow_up={} closed={} source={}",
                attempt.ticket.key.policy,
                follow_up,
                closed,
                attempt.ticket.source_current(&state.peek())
            );
        }
        let follow_up = follow_up && closed && attempt.ticket.source_current(&state.peek());
        drop(attempt);
        if follow_up {
            dioxus::core::queue_effect(move || request(state));
        }
    });
}
