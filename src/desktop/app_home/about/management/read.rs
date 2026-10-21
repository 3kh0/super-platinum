use super::*;
use super_platinum_core::slack::PreparedRequest;

#[derive(Clone)]
pub(super) struct Lease {
    pub owner: Owner,
    pub visit: u64,
    pub id: u64,
    credentials: String,
    synthetic: bool,
}
impl Lease {
    pub fn begin(s: &mut ShellState, visit: u64) -> Option<Self> {
        permissions::managers::departure::revoke(s, visit);
        if !admitted(s, visit)
            || delete::confirming(s, visit)
            || unpublish::confirming(s, visit)
            || permissions::confirming(s, visit)
        {
            return None;
        }
        let credentials = serde_json::to_string(s.core.session.as_ref()?).ok()?;
        let h = s.app_home.about.management.surface.as_mut()?;
        if h.read.is_some() {
            return None;
        }
        let id = next();
        h.read = Some(id);
        h.notice = None;
        Some(Self {
            owner: h.owner.clone(),
            visit,
            id,
            credentials,
            synthetic: h.fixture,
        })
    }
    pub fn current(&self, s: &ShellState) -> bool {
        read_current(s, self.visit, self.id)
            && self.owner.current(s)
            && s.core
                .session
                .as_ref()
                .and_then(|s| serde_json::to_string(s).ok())
                .as_ref()
                == Some(&self.credentials)
    }
    pub async fn request(
        &self,
        mut state: Signal<ShellState>,
        request: PreparedRequest,
    ) -> Result<Value, Error> {
        if !self.current(&state.peek()) {
            return Err(unavailable());
        }
        let result = if self.synthetic {
            if let Some(result) = fixture::first_publish::read(state, &request).await {
                result
            } else if let Some(result) = fixture::managers::read(state, &request).await {
                result
            } else if let Some(result) = fixture::run_access::read(state, &request).await {
                result
            } else if let Some(result) = fixture::permissions::read(state, &request).await {
                result
            } else if let Some(result) = fixture::publish::read(state, &request).await {
                result
            } else if let Some(result) = fixture::unpublish::read(state, &request).await {
                result
            } else {
                match fixture::delete::read(state, &request).await {
                    Some(result) => result,
                    None => fixture::response(&mut state.write(), &request),
                }
            }
        } else if crate::fixture::is_fixture() {
            Err(unavailable())
        } else {
            self.owner.ctx.transport.execute(request).await
        };
        if let Err(Error::RateLimited { retry_after_secs }) = &result {
            navigation::retain_limit(&mut state.write(), &self.owner.ctx, *retry_after_secs);
        }
        if !self.current(&state.peek()) {
            return Err(unavailable());
        }
        result
    }
    pub async fn plan(
        &self,
        state: Signal<ShellState>,
        plan: &protocol::RequestPlan,
    ) -> Result<Value, Error> {
        self.request(
            state,
            plan.request(&self.owner.ctx.client, &self.owner.ctx.session),
        )
        .await
    }
    pub fn finish(&self, s: &mut ShellState, message: Option<String>) {
        if let Some(h) = s
            .app_home
            .about
            .management
            .surface
            .as_mut()
            .filter(|h| h.id == self.visit && h.read == Some(self.id))
        {
            h.read = None;
            h.notice = message;
        }
    }
}
pub(super) async fn load_editor(
    state: Signal<ShellState>,
    lease: &Lease,
    id: &str,
) -> Result<Editor, Error> {
    let mut load = EditorLoad::new(id)?;
    loop {
        let requests = load.requests();
        if requests.is_empty() {
            break;
        }
        for request in requests {
            let raw = lease.plan(state, &request.plan).await?;
            load.accept(request.kind, raw)?;
        }
    }
    load.editor()
}
pub(super) async fn acquire_policy(
    mut state: Signal<ShellState>,
    lease: &Lease,
    managed: bool,
) -> Result<(), Error> {
    let policy = policy::acquire(state, lease).await?;
    if !lease.current(&state.peek())
        || !(if managed {
            policy.can_manage()
        } else {
            policy.can_enter()
        })
    {
        return Err(unavailable());
    }
    state
        .write()
        .app_home
        .about
        .management
        .surface
        .as_mut()
        .unwrap()
        .policy = Some(policy);
    Ok(())
}
pub(super) fn list(mut state: Signal<ShellState>, visit: u64, more: bool) {
    let prepared = {
        let mut s = state.write();
        let Some(lease) = Lease::begin(&mut s, visit) else {
            return;
        };
        let h = s.app_home.about.management.surface.as_mut().unwrap();
        let query = if more {
            let Some(q) = h.list.next_query() else {
                h.read = None;
                return;
            };
            q
        } else {
            h.list = List::default();
            h.query.cursor = None;
            h.query.clone()
        };
        h.document = None;
        (lease, query)
    };
    dioxus::core::spawn_forever(async move {
        let (lease, query) = prepared;
        let result = async {
            acquire_policy(state, &lease, true).await?;
            let raw = lease.plan(state, &query.request()?).await?;
            let workflows = ListWorkflows::decode(query, raw)?;
            let raw = lease.plan(state, &workflows.triggers_request()).await?;
            ListPage::decode(workflows, raw)
        }
        .await;
        let mut s = state.write();
        if !lease.current(&s) {
            lease.finish(
                &mut s,
                Some("Workflow list changed or could not load. Retry when connected.".into()),
            );
            return;
        }
        let result = result.and_then(|page| {
            s.app_home
                .about
                .management
                .surface
                .as_mut()
                .unwrap()
                .list
                .append(page)
        });
        lease.finish(
            &mut s,
            result
                .err()
                .map(|_| "Workflow list or permissions could not load. Try again.".into()),
        );
    });
}
pub(super) fn resolve(mut state: Signal<ShellState>, visit: u64, app: String) {
    let Some(lease) = Lease::begin(&mut state.write(), visit) else {
        return;
    };
    dioxus::core::spawn_forever(async move {
        let result = async {
            acquire_policy(state, &lease, false).await?;
            let raw = lease.plan(state, &protocol::resolve_app(&app)?).await?;
            let resolved = protocol::AppWorkflow::decode(&app, raw)?;
            load_editor(state, &lease, resolved.workflow_id()).await
        }
        .await;
        finish_editor(&mut state.write(), &lease, result);
    });
}
pub(super) fn editor(mut state: Signal<ShellState>, visit: u64, id: String) {
    if permissions::owns(&state.peek(), visit) {
        permissions::check(state, visit);
        return;
    }
    if publish::owns(&state.peek(), visit) {
        publish::check(state, visit);
        return;
    }
    if unpublish::owns(&state.peek(), visit) {
        unpublish::check(state, visit);
        return;
    }
    let Some(lease) = Lease::begin(&mut state.write(), visit) else {
        return;
    };
    dioxus::core::spawn_forever(async move {
        let result = async {
            acquire_policy(state, &lease, false).await?;
            load_editor(state, &lease, &id).await
        }
        .await;
        finish_editor(&mut state.write(), &lease, result);
    });
}
fn finish_editor(s: &mut ShellState, lease: &Lease, result: Result<Editor, Error>) {
    if !lease.current(s) {
        lease.finish(
            s,
            Some("The workflow read was retired. Your draft is retained.".into()),
        );
        return;
    }
    let result = result.and_then(|editor| adopt(s, lease, editor));
    let accepted = result.is_ok();
    lease.finish(
        s,
        result.err().map(|_| {
            "The workflow or its permissions could not load. Your draft is retained.".into()
        }),
    );
    if accepted {
        entry::loaded(s, lease.visit);
    }
}
fn adopt(s: &mut ShellState, lease: &Lease, editor: Editor) -> Result<(), Error> {
    let account = s.core.active_account.clone();
    let book = &mut s.app_home.about.management;
    let existing = book.documents.iter().position(|d| {
        d.account == account
            && d.team == lease.owner.ctx.session.team_id
            && d.user == lease.owner.ctx.session.user_id
            && d.editor.workflow_id() == editor.workflow_id()
    });
    let id = if let Some(index) = existing {
        let d = &mut book.documents[index];
        if delete::retained(&book.deletions, d.pending)
            || unpublish::retained(&book.unpublishing, d.pending)
            || publish::retained(&book.publishing, d.pending)
            || permissions::retained(&book.permissions, d.pending)
        {
            // A read cannot resolve, replay, or erase a Delete receipt.
        } else if d.pending.is_some() || d.dirty() {
            d.observed = Some(editor);
            d.notice = Some(
                "Current workflow loaded for review. Your proposed changes are retained.".into(),
            );
        } else {
            d.adopt(editor);
            d.notice = None;
        }
        d.id
    } else {
        if book.documents.len() >= 32 {
            return Err(unavailable());
        }
        let id = next();
        book.documents.push(Document {
            activity: activity::State::default(),
            id,
            account,
            team: lease.owner.ctx.session.team_id.clone(),
            user: lease.owner.ctx.session.user_id.clone(),
            title: editor.raw_workflow()["title"].as_str().unwrap_or("").into(),
            description: editor.raw_workflow()["description"]
                .as_str()
                .unwrap_or("")
                .into(),
            editor,
            revision: next(),
            observed: None,
            pending: None,
            notice: None,
        });
        id
    };
    book.surface.as_mut().unwrap().document = Some(id);
    write::observe(s, lease.visit);
    Ok(())
}
