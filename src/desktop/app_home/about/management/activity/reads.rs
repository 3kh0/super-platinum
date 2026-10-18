use super::*;
#[cfg(test)]
mod tests;

#[derive(Clone)]
pub(in crate::app_home::about::management::activity) struct Read {
    lease: read::Lease,
    expected: Expected,
    breadcrumbs: Vec<Value>,
    detail: Option<(u64, Value)>,
}
impl Read {
    pub(in crate::app_home::about::management::activity) fn begin(
        s: &mut ShellState,
        visit: u64,
    ) -> Option<Self> {
        Self::begin_inner(s, visit, true)
    }
    fn begin_inner(s: &mut ShellState, visit: u64, clear_notice: bool) -> Option<Self> {
        if !permitted(s, visit) {
            return None;
        }
        let d = document(s, visit).filter(|d| d.activity.active)?;
        let expected = Expected {
            visit,
            document: d.id,
            filters: d.activity.filters.clone(),
        };
        let breadcrumbs = d.activity.breadcrumbs.clone();
        let lease = read::Lease::begin(s, visit)?;
        if clear_notice {
            document_mut(s, visit)?.activity.notice = None;
        }
        Some(Self {
            lease,
            expected,
            breadcrumbs,
            detail: None,
        })
    }
    pub(super) fn begin_refresh(s: &mut ShellState, visit: u64) -> Option<Self> {
        let a = &document(s, visit)?.activity;
        if !a.detail.as_ref().is_some_and(detail_uses_names) {
            return None;
        }
        let selected = (a.detail_revision, a.detail_row.clone()?);
        let mut read = Self::begin_inner(s, visit, false)?;
        document_mut(s, visit)?.activity.detail_read = Some(read.lease.id);
        read.detail = Some(selected);
        Some(read)
    }
    pub(super) fn id(&self) -> u64 {
        self.lease.id
    }
    pub(super) fn release(&self, s: &mut ShellState) {
        self.lease.finish(s, None);
        if let Some(d) = s
            .app_home
            .about
            .management
            .documents
            .iter_mut()
            .find(|d| d.id == self.expected.document)
        {
            if d.activity.detail_read == Some(self.id()) {
                d.activity.detail_read = None;
            }
            if d.activity.name_refresh_read == Some(self.id()) {
                d.activity.name_refresh_read = None;
            }
        }
    }
    fn owns(&self, s: &ShellState) -> bool {
        visible(s, self.lease.visit)
            && s.app_home
                .about
                .management
                .surface
                .as_ref()
                .is_some_and(|h| h.read == Some(self.lease.id))
            && document(s, self.lease.visit).is_some_and(|d| {
                d.id == self.expected.document
                    && d.activity.active
                    && d.activity.filters == self.expected.filters
                    && d.activity.breadcrumbs == self.breadcrumbs
                    && self.detail.as_ref().is_none_or(|(revision, row)| {
                        d.activity.detail_revision == *revision
                            && d.activity.detail_read == Some(self.lease.id)
                            && d.activity.detail_row.as_ref() == Some(row)
                    })
            })
    }
    pub(in crate::app_home::about::management::activity) fn current(&self, s: &ShellState) -> bool {
        self.owns(s) && self.lease.current(s) && permitted(s, self.lease.visit)
    }
    pub(in crate::app_home::about::management::activity) fn owner(&self) -> &Owner {
        &self.lease.owner
    }
    pub(in crate::app_home::about::management::activity) fn visit(&self) -> u64 {
        self.expected.visit
    }
    pub(in crate::app_home::about::management::activity) fn document(&self) -> u64 {
        self.expected.document
    }
    /// Name preferences use their existing prepared endpoints under this same
    /// detail lease, never a picker query or a request from rendering.
    pub(in crate::app_home::about::management::activity) async fn request_prepared(
        &self,
        mut state: Signal<ShellState>,
        request: super_platinum_core::slack::PreparedRequest,
    ) -> Result<Value, Error> {
        if !self.current(&state.peek()) {
            return Err(unavailable());
        }
        let dates = native::snapshot(&self.expected).await?;
        if !self.current(&state.peek()) {
            return Err(unavailable());
        }
        let synthetic = fixture::enabled(&state.peek());
        let work = async {
            if synthetic {
                fixture::name_request(state, &request).await
            } else {
                self.lease.request(state, request).await
            }
        };
        let result =
            tokio::select! {result=work=>result, _=self.retired(state)=>Err(unavailable())};
        if synthetic && let Err(Error::RateLimited { retry_after_secs }) = &result {
            navigation::retain_limit(&mut state.write(), &self.lease.owner.ctx, *retry_after_secs);
        }
        if native::snapshot(&self.expected).await? != dates || !self.current(&state.peek()) {
            return Err(unavailable());
        }
        result
    }
    async fn retired(&self, state: Signal<ShellState>) {
        loop {
            tokio::time::sleep(Duration::from_millis(100)).await;
            if !self.current(&state.peek()) {
                return;
            }
        }
    }
    async fn acquire_policy(&self, state: Signal<ShellState>) -> Result<(), Error> {
        tokio::select! {
            result=read::acquire_policy(state,&self.lease,false)=>result,
            _=self.retired(state)=>Err(unavailable()),
        }
    }
    async fn request(
        &self,
        mut state: Signal<ShellState>,
        plan: &protocol::RequestPlan,
    ) -> Result<Value, Error> {
        if !self.current(&state.peek()) {
            return Err(unavailable());
        }
        let request = plan.request(&self.lease.owner.ctx.client, &self.lease.owner.ctx.session);
        let synthetic = fixture::enabled(&state.peek());
        let work = async {
            if synthetic {
                fixture::request(state, &request).await
            } else {
                self.lease.request(state, request).await
            }
        };
        // Only while this explicit read is alive: dropping its future also
        // retires an offline hold when its document/filter/account changes.
        let result =
            tokio::select! {result=work=>result, _=self.retired(state)=>Err(unavailable())};
        if synthetic && let Err(Error::RateLimited { retry_after_secs }) = &result {
            navigation::retain_limit(&mut state.write(), &self.lease.owner.ctx, *retry_after_secs);
        }
        if !self.current(&state.peek()) {
            return Err(unavailable());
        }
        result
    }
    fn finish(&self, s: &mut ShellState) -> bool {
        if !self.owns(s) {
            return false;
        }
        self.lease.finish(s, None);
        true
    }
}
pub(super) fn load(mut state: Signal<ShellState>, visit: u64, direction: Page) {
    let prepared = {
        let mut s = state.write();
        let Some(d) = document(&s, visit).filter(|d| d.activity.active) else {
            return;
        };
        let selected = match direction {
            Page::First => None,
            Page::Next => {
                let Some(q) = d.activity.page.as_ref().and_then(ActivityPage::next_query) else {
                    return;
                };
                Some(q)
            }
            Page::Previous => {
                let Some(q) = d.activity.previous.last().cloned() else {
                    return;
                };
                Some(q)
            }
        };
        let workflow = d.editor.workflow_id().to_owned();
        let Some(read) = Read::begin(&mut s, visit) else {
            return;
        };
        (read, workflow, selected)
    };
    dioxus::core::spawn_forever(async move {
        let (read, workflow, selected) = prepared;
        let result = async {
            let dates = native::snapshot(&read.expected).await?;
            if !read.current(&state.peek()) {
                return Err(unavailable());
            }
            let parent = read
                .breadcrumbs
                .last()
                .and_then(|r| r["workflow_execution_id"].as_str());
            let first =
                query(&workflow, &read.expected.filters, dates.clone())?.with_parent(parent)?;
            let selected = selected.unwrap_or_else(|| first.clone());
            if selected.filters() != first.filters()
                || selected.limit() != first.limit()
                || selected.workflow_id() != workflow
                || selected.parent_execution_id() != first.parent_execution_id()
            {
                return Err(unavailable());
            }
            read.acquire_policy(state).await?;
            let raw = read.request(state, &selected.request()).await?;
            if native::snapshot(&read.expected).await? != dates || !read.current(&state.peek()) {
                return Err(unavailable());
            }
            ActivityPage::decode(&selected, raw)
        }
        .await;
        let mut s = state.write();
        if !read.finish(&mut s) {
            return;
        }
        let a = &mut document_mut(&mut s, visit).unwrap().activity;
        let accepted = result.is_ok();
        match result {
            Ok(page) => adopt_page(a, page, direction),
            Err(_) => {
                a.notice =
                    Some("Activity could not be loaded. Check the filters and try again.".into())
            }
        }
        drop(s);
        if accepted {
            dioxus::core::queue_effect(move || execution::request(state, visit, read.id()));
        }
    });
}
pub(super) struct DetailRead {
    read: Read,
    request: ActivityDetailRequest,
    revision: u64,
}
impl DetailRead {
    pub(super) fn id(&self) -> u64 {
        self.read.id()
    }
    pub(super) fn revision(&self) -> u64 {
        self.revision
    }
    pub(super) fn release(&self, s: &mut ShellState) {
        self.read.release(s);
    }
}
pub(super) fn prepare_detail(
    s: &mut ShellState,
    visit: u64,
    execution: &str,
) -> Option<DetailRead> {
    let d = document(s, visit).filter(|d| d.activity.active)?;
    let page = d.activity.page.as_ref()?;
    let row = routes::rows(page, &d.activity.breadcrumbs)
        .into_iter()
        .find(|r| r["workflow_execution_id"] == execution)?
        .clone();
    let request = ActivityDetailRequest::new(d.editor.workflow_id(), execution).ok()?;
    let mut read = Read::begin(s, visit)?;
    let a = &mut document_mut(s, visit)?.activity;
    a.name_notice = None;
    a.detail = None;
    a.detail_row = Some(row);
    a.detail_revision = next();
    a.detail_read = Some(read.lease.id);
    read.detail = Some((a.detail_revision, a.detail_row.clone().unwrap()));
    Some(DetailRead {
        read,
        request,
        revision: a.detail_revision,
    })
}
pub(super) fn detail(mut state: Signal<ShellState>, visit: u64, execution: String) {
    let prepared = {
        let mut s = state.write();
        let Some(prepared) = prepare_detail(&mut s, visit, &execution) else {
            return;
        };
        document_mut(&mut s, visit).unwrap().activity.execution = None;
        prepared
    };
    dioxus::core::spawn_forever(run_detail(state, prepared, None));
}
pub(super) async fn run_detail(
    mut state: Signal<ShellState>,
    prepared: DetailRead,
    automatic: Option<execution::native::Guard>,
) {
    let DetailRead {
        read,
        request,
        revision,
    } = prepared;
    let visit = read.visit();
    let result = async {
        let dates = native::snapshot(&read.expected).await?;
        let raw = read.request(state, &request.request()).await?;
        if native::snapshot(&read.expected).await? != dates || !read.current(&state.peek()) {
            return Err(unavailable());
        }
        let detail = request.decode(raw)?;
        if detail_uses_names(&detail) {
            members::names::capture_detail(state, &read).await?;
            if native::snapshot(&read.expected).await? != dates || !read.current(&state.peek()) {
                return Err(unavailable());
            }
        }
        Ok(detail)
    }
    .await;
    // Automatic entry has a retained native witness. Manual row clicks retain
    // their established detail path; neither can publish a retired read.
    let guard = match &automatic {
        Some(native) => native.lock().await,
        None => None,
    };
    if automatic.is_some() && (guard.is_none() || !read.current(&state.peek())) {
        #[allow(deprecated)]
        read.release(&mut state.write_silent());
        return;
    }
    let loaded = result.is_ok();
    {
        let mut s = state.write();
        if !read.finish(&mut s) {
            return;
        }
        let a = &mut document_mut(&mut s, visit).unwrap().activity;
        if a.detail_revision != revision {
            return;
        }
        a.detail_read = None;
        match result {
            Ok(detail) => {
                a.detail = Some(detail);
                a.notice = None;
            }
            Err(_) => {
                a.notice =
                    Some("Run details could not be loaded. Close the details and try again.".into())
            }
        }
    }
    if let Some(guard) = guard {
        let _ = guard.finish(loaded).await;
    }
}

pub(super) fn detail_uses_names(detail: &ActivityDetail) -> bool {
    detail
        .trigger_message()
        .into_iter()
        .chain(detail.steps().iter().map(|step| &step["outcome_message"]))
        .any(|message| {
            presentation::parts(message)
                .iter()
                .any(|part| matches!(part, presentation::Part::User(_)))
        })
}
