//! Stateful isolated native proof. No live Source or application-store fallback.
use super::adapter::Source as _;
use super::{Draft, adapter, domain};
use crate::state::{Overlay, ShellState};
use dioxus::prelude::*;
use serde_json::{Value, json};
use std::{
    cell::Cell,
    future::Future,
    path::PathBuf,
    pin::Pin,
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicU8, AtomicU64, Ordering},
    },
};
use super_platinum_core::{
    config::WorkspaceSession,
    slack::{Error, PreparedRequest, SlackClient, client::RequestBody},
};
use tokio::sync::oneshot;

pub(crate) mod check;
mod harness;

pub(crate) const ACCOUNT: &str = "00000000-0000-4000-8000-0000000000c1";
pub(crate) const TITLE: &str = "Native canvas \u{65e5}\u{672c}\u{8a9e} \u{1f680}";
pub(crate) const BODY: &str =
    "## Exact body\n\n  \u{4e2d}\u{1f642} e\u{301}  \n- [ ] Keep whitespace\n";
pub(crate) const AHEAD: &str = "Native ahead\n\u{65e5}\u{672c}\u{8a9e} \u{1f680} e\u{301}\n";
pub(crate) const COMPOSED: &str = "Composed \u{65e5}\u{672c}\u{8a9e} \u{1f680} e\u{301}\n";
pub(crate) const OURS: &str = "Our retained local version \u{4e2d}\u{1f642} e\u{301}\n";
pub(crate) const FOREIGN: &str = "Other isolated writer; retain this until explicit review.";
pub(crate) const FILE_ID: &str = "FCANVASFIXTURE1";
const FLAG: &str = "SUPER_PLATINUM_CANVAS_CREATION_CHECK";
static BINDING: OnceLock<Arc<Binding>> = OnceLock::new();

pub(crate) fn requested() -> bool {
    matches!(
        std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
        Ok("canvas-creation-durable" | "canvas-creation-durable-narrow")
    )
}
pub(crate) fn enabled() -> bool {
    crate::fixture::is_fixture() && requested() && std::env::var(FLAG).as_deref() == Ok("1")
}
pub(crate) fn apply(shell: &mut ShellState) {
    assert!(shell.core.session.is_none() && shell.core.transport.is_none());
    shell.main_view = crate::state::MainView::Files;
    shell.core.active_account = Some(ACCOUNT.into());
    shell.core.active_team = Some("T1".into());
    shell.self_account.user_id = "U00000001".into();
    shell.core.workspaces.get_mut("T1").unwrap().self_user_id = "U00000001".into();
    // A useful static view without an explicit storage/check capability.
    shell.canvas_create = Some(Draft::fixture(shell));
    shell.overlay = Some(Overlay::CanvasCreate);
}

pub(super) struct Binding {
    owner: domain::Owner,
    path: PathBuf,
    directory: PathBuf,
    nonce: String,
    fixture: String,
    scenario: String,
    phase: String,
    epoch: u64,
    mode: AtomicU8,
    sources: AtomicU64,
    requests: Mutex<Vec<Value>>,
    pending: Mutex<Option<oneshot::Sender<Result<Value, Error>>>>,
}
impl Binding {
    pub(super) fn path(&self) -> &std::path::Path {
        &self.path
    }
    pub(super) fn owner(&self) -> &domain::Owner {
        &self.owner
    }
    fn current(&self, shell: &ShellState) -> bool {
        enabled()
            && harness::same_configuration(self)
            && shell.core.session.is_none()
            && shell.core.transport.is_none()
            && shell.core.account_epoch == self.epoch
            && shell.core.active_account.as_deref() == Some(ACCOUNT)
            && shell.core.active_team.as_deref() == Some("T1")
            && shell.self_account.user_id == self.owner.user()
            && shell
                .core
                .workspaces
                .get("T1")
                .is_some_and(|w| w.self_user_id == self.owner.user())
    }
    fn storage(&self) -> adapter::Storage {
        adapter::Storage::fixture(self)
    }
    fn ledger(&self) -> Vec<Value> {
        self.requests.lock().unwrap().clone()
    }
    fn set_mode(&self, mode: u8) -> Result<(), String> {
        if self.pending.lock().unwrap().is_some() {
            return Err("A response is already held".into());
        }
        self.mode.store(mode, Ordering::Release);
        Ok(())
    }
    fn release(&self, value: Result<Value, Error>) -> Result<(), String> {
        self.pending
            .lock()
            .unwrap()
            .take()
            .ok_or("No owned held response")?
            .send(value)
            .map_err(|_| "Held response receiver vanished".into())
    }
    async fn inventory(&self) -> Result<Value, String> {
        let owner = self.owner.clone();
        self.storage().run(move |s| {
            let r=s.retirement_review(&owner)?;
            Ok(json!({"active":s.active(&owner)?,"drafts":r.drafts(),"attempts":r.attempts(),
                "conflicts":r.conflicts(),"lateResults":r.late_results(),"decisions":r.decisions(),"retryAt":r.retry_at()}))
        }).await.map_err(|_|"Cannot read isolated inventory".into())
    }
}
pub(super) fn owner(shell: &ShellState) -> Option<domain::Owner> {
    BINDING
        .get()
        .filter(|b| b.current(shell))
        .map(|b| b.owner.clone())
}
pub(super) fn storage(shell: &ShellState, owner: &domain::Owner) -> Option<adapter::Storage> {
    BINDING
        .get()
        .filter(|b| b.current(shell) && &b.owner == owner)
        .map(|b| b.storage())
}

/// One private sum type keeps the production admission/execute path unchanged.
pub(super) enum DeliverySource {
    Live(adapter::LiveSource),
    Fixture(FixtureSource),
}
impl DeliverySource {
    pub(super) fn capture(state: Signal<ShellState>) -> Option<Self> {
        if crate::fixture::is_fixture() {
            let binding = BINDING.get()?.clone();
            let shell = state.peek();
            if !binding.current(&shell) || shell.overlay != Some(Overlay::CanvasCreate) {
                return None;
            }
            let visit = shell.canvas_create.as_ref()?.visit.clone();
            binding.sources.fetch_add(1, Ordering::AcqRel);
            let mode = binding.mode.load(Ordering::Acquire);
            Some(Self::Fixture(FixtureSource {
                binding,
                state,
                visit,
                mode,
                checks: Cell::new(0),
                prepared: Cell::new(None),
                client: SlackClient::default(),
                workspace: WorkspaceSession {
                    team_id: "T1".into(),
                    user_id: "U00000001".into(),
                    enterprise_id: None,
                    name: "Isolated Canvas fixture".into(),
                    url: "https://fixture.slack.com".into(),
                    token: "canvas-fixture-not-a-token".into(),
                },
            }))
        } else {
            adapter::LiveSource::capture(state).map(Self::Live)
        }
    }
}
pub(super) struct FixtureSource {
    binding: Arc<Binding>,
    state: Signal<ShellState>,
    visit: String,
    mode: u8,
    checks: Cell<u64>,
    prepared: Cell<Option<usize>>,
    client: SlackClient,
    workspace: WorkspaceSession,
}
impl adapter::Source for FixtureSource {
    fn owner(&self) -> &domain::Owner {
        &self.binding.owner
    }
    fn current(&self) -> bool {
        let n = self.checks.get() + 1;
        self.checks.set(n);
        let s = self.state.peek();
        self.binding.current(&s)
            && s.overlay == Some(Overlay::CanvasCreate)
            && s.canvas_create
                .as_ref()
                .is_some_and(|d| d.visit == self.visit)
            && !(self.mode == 1 && n >= 2) // Exact pre-poll refusal; no request enters execute.
    }
    fn client(&self) -> &SlackClient {
        &self.client
    }
    fn workspace(&self) -> &WorkspaceSession {
        &self.workspace
    }
    fn prepared(&self, request: &PreparedRequest) {
        // This callback observes the actual builder result before admission.
        // A repeated callback is invalid rather than another recorded request.
        if self.prepared.get().is_some() {
            self.prepared.set(Some(usize::MAX));
            return;
        }
        let mut records = self.binding.requests.lock().unwrap();
        self.prepared.set(Some(records.len()));
        records.push(request_record(request, false));
    }
    fn execute(
        &self,
        request: PreparedRequest,
    ) -> Pin<Box<dyn Future<Output = Result<Value, Error>> + '_>> {
        Box::pin(async move {
            let RequestBody::Form(_) = &request.body else {
                return Err(Error::Api("fixture_form_required".into()));
            };
            if request.method != "POST"
                || request.url.split('?').next()
                    != Some("https://fixture.slack.com/api/files.createCanvas")
                || request.retry_safe()
                || self.mode != 2
            {
                return Err(Error::Api("fixture_request_refused".into()));
            }
            let (send, receive) = oneshot::channel();
            {
                let mut pending = self.binding.pending.lock().unwrap();
                if pending.is_some() {
                    return Err(Error::Api("fixture_second_request_refused".into()));
                }
                *pending = Some(send);
            }
            {
                let mut records = self.binding.requests.lock().unwrap();
                let Some(record) = self.prepared.get().and_then(|i| records.get_mut(i)) else {
                    return Err(Error::Api("fixture_missing_prepared_request".into()));
                };
                if *record != request_record(&request, false) {
                    return Err(Error::Api("fixture_prepared_request_changed".into()));
                }
                *record = request_record(&request, true);
            }
            receive
                .await
                .unwrap_or_else(|_| Err(Error::Offline("isolated response dropped".into())))
        })
    }
}
impl adapter::Source for DeliverySource {
    fn owner(&self) -> &domain::Owner {
        match self {
            Self::Live(s) => s.owner(),
            Self::Fixture(s) => s.owner(),
        }
    }
    fn current(&self) -> bool {
        match self {
            Self::Live(s) => s.current(),
            Self::Fixture(s) => s.current(),
        }
    }
    fn client(&self) -> &SlackClient {
        match self {
            Self::Live(s) => s.client(),
            Self::Fixture(s) => s.client(),
        }
    }
    fn workspace(&self) -> &WorkspaceSession {
        match self {
            Self::Live(s) => s.workspace(),
            Self::Fixture(s) => s.workspace(),
        }
    }
    fn prepared(&self, r: &PreparedRequest) {
        match self {
            Self::Live(s) => s.prepared(r),
            Self::Fixture(s) => s.prepared(r),
        }
    }
    fn execute(
        &self,
        r: PreparedRequest,
    ) -> Pin<Box<dyn Future<Output = Result<Value, Error>> + '_>> {
        match self {
            Self::Live(s) => s.execute(r),
            Self::Fixture(s) => s.execute(r),
        }
    }
}

fn request_record(request: &PreparedRequest, executed: bool) -> Value {
    let form = match &request.body {
        RequestBody::Form(form) => json!(form),
        _ => Value::Null,
    };
    json!({"method":request.method,"url":request.url,"headers":request.headers,
        "form":form,"retrySafe":request.retry_safe(),"executed":executed})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn request_observation_preserves_real_builder_output_before_dispatch() {
        let workspace = WorkspaceSession {
            team_id: "T1".into(),
            user_id: "U00000001".into(),
            enterprise_id: None,
            name: "Fixture".into(),
            url: "https://fixture.slack.com".into(),
            token: "canvas-fixture-not-a-token".into(),
        };
        let request = super_platinum_core::slack::api::canvas_create(
            &SlackClient::default(),
            &workspace,
            TITLE,
            BODY,
        )
        .unwrap();
        let prepared = request_record(&request, false);
        assert_eq!(prepared["executed"], false);
        assert_eq!(
            prepared["form"],
            json!([
                ["token", "canvas-fixture-not-a-token"],
                ["title", TITLE],
                ["markdown", BODY]
            ])
        );
        let executed = request_record(&request, true);
        assert_eq!(executed["executed"], true);
        for key in ["url", "method", "headers", "form", "retrySafe"] {
            assert_eq!(prepared[key], executed[key]);
        }
        assert_eq!(prepared["retrySafe"], false);
        assert!(BODY.contains('\u{1f642}'));
        assert!(BODY.contains("e\u{301}"));
        assert!(TITLE.contains('\u{65e5}'));
        assert!(BODY.encode_utf16().count() > BODY.chars().count());
    }
}
