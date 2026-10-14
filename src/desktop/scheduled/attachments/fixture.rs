//! Exact-gated, memory-only attachment driver. No Transport, session or file I/O.
use super::Owner as AttachmentOwner;
use super::*;
use crate::state::{MainView, ShellState};
use dioxus::prelude::*;
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    path::PathBuf,
    sync::{
        Mutex, OnceLock, Weak,
        atomic::{AtomicU64, Ordering},
    },
};
use super_platinum_core::{
    config::WorkspaceSession,
    slack::{Error, PreparedRequest, SlackClient, client::RequestBody},
};
pub(crate) mod check;
mod harness;
pub(crate) use harness::{capture, wait_runner};

pub(crate) const ACCOUNT: &str = "00000000-0000-4000-8000-000000000091";
pub(crate) const DRAFT: &str = "DrAttachmentFixture";
pub(crate) const EMPTY: &str = "DrAttachmentEmpty";
pub(crate) const FILES: [&str; 3] = ["F00000001", "F00000002", "F00000003"];
pub(crate) const SCENARIOS: &[&str] = &[
    "ordered-rich",
    "empty-body-cancel",
    "uncertain-owner",
    "known-unsent",
    "rejected",
];
pub(crate) fn enabled() -> bool {
    std::env::var("SUPER_PLATINUM_SCHEDULED_ATTACHMENTS_CHECK").as_deref() == Ok("1")
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("scheduled-attachments" | "scheduled-attachments-narrow")
        )
}
pub(crate) fn raw(id: &str) -> Value {
    json!({"id":id,"client_msg_id":if id==EMPTY{"00000000-0000-4000-8000-000000000093"}else{"00000000-0000-4000-8000-000000000092"},
        "date_scheduled":if id==EMPTY{0}else{2100000000i64},"last_updated_ts":"10.000001","is_deleted":false,"is_sent":false,
        "file_ids":if id==EMPTY{json!([FILES[0]])}else{json!(FILES)},
        "blocks":if id==EMPTY{json!([])}else{json!([{"type":"rich_text","block_id":"retained-body","elements":[{"type":"rich_text_section","elements":[
            {"type":"text","text":"Attachment review \u{65e5}\u{672c}\u{8a9e} \u{1f680}","style":{"bold":true}},
            {"type":"text","text":"\nKeep this exact rich body and thread destination."}]}]}])},
        "destinations":[{"channel_id":"C2","thread_ts":"1719800400.000000","broadcast":true}]})
}
pub(crate) fn apply(shell: &mut ShellState) {
    assert!(shell.core.session.is_none() && shell.core.transport.is_none());
    shell.core.active_account = Some(ACCOUNT.into());
    shell.self_account.user_id = "U00000001".into();
    shell.core.workspaces.get_mut("T1").unwrap().self_user_id = "U00000001".into();
    shell.main_view = MainView::Scheduled;
    shell
        .core
        .scheduled
        .ensure_scope(shell.core.account_epoch, "T1");
    let (generation, limit) = shell.core.scheduled.begin(false).unwrap();
    shell.core.scheduled.apply(
        generation,
        super_platinum_core::scheduled::ScheduledPage {
            drafts: [DRAFT, EMPTY]
                .map(|id| serde_json::from_value(raw(id)).unwrap())
                .to_vec(),
            has_more: false,
        },
        limit,
    );
    shell.core.scheduled.filter = super_platinum_core::scheduled::DraftFilter::Scheduled;
}
#[derive(Clone, Copy)]
pub(crate) enum Outcome {
    Confirmed,
    Uncertain,
    NotSent,
    Rejected,
}
#[derive(Default)]
struct Hold {
    armed: bool,
    reached: bool,
    released: bool,
}
pub(crate) struct State {
    pub nonce: String,
    pub scenario: String,
    pub directory: PathBuf,
    generation: AtomicU64,
    pub ledger: Mutex<Vec<Value>>,
    server: Mutex<BTreeMap<String, Value>>,
    outcomes: Mutex<VecDeque<Outcome>>,
    hold: Mutex<Hold>,
}
fn installed() -> &'static Mutex<Weak<State>> {
    static STATE: OnceLock<Mutex<Weak<State>>> = OnceLock::new();
    STATE.get_or_init(Mutex::default)
}
impl State {
    fn configured(&self) -> bool {
        enabled()
            && std::env::var("SUPER_PLATINUM_SCHEDULED_ATTACHMENTS_NONCE").as_deref()
                == Ok(self.nonce.as_str())
            && std::env::var("SUPER_PLATINUM_SCHEDULED_ATTACHMENTS_SCENARIO").as_deref()
                == Ok(self.scenario.as_str())
            && std::env::var_os("SUPER_PLATINUM_SCHEDULED_ATTACHMENTS_READY_DIR")
                .is_some_and(|p| PathBuf::from(p) == self.directory)
    }
    fn admits(&self, shell: &ShellState) -> bool {
        self.configured()
            && shell.core.session.is_none()
            && shell.core.transport.is_none()
            && shell.core.active_account.as_deref() == Some(ACCOUNT)
            && shell.core.active_team.as_deref() == Some("T1")
            && shell.self_account.user_id == "U00000001"
            && shell
                .core
                .workspaces
                .get("T1")
                .is_some_and(|w| w.self_user_id == "U00000001")
    }
    pub fn count(&self) -> usize {
        self.ledger.lock().unwrap().len()
    }
    pub fn arm_read(&self) -> Result<(), String> {
        let mut h = self.hold.lock().unwrap();
        if h.armed {
            return Err("Read already held".into());
        }
        *h = Hold {
            armed: true,
            ..Default::default()
        };
        Ok(())
    }
    pub async fn wait_read(&self) -> Result<(), String> {
        for _ in 0..250 {
            if self.hold.lock().unwrap().reached {
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        Err("Preflight not reached".into())
    }
    pub fn release_read(&self) {
        self.hold.lock().unwrap().released = true;
    }
    async fn hold_read(&self) -> Result<(), Error> {
        {
            let mut h = self.hold.lock().unwrap();
            if !h.armed {
                return Ok(());
            }
            h.reached = true;
        }
        for _ in 0..500 {
            {
                let mut h = self.hold.lock().unwrap();
                if h.released {
                    *h = Hold::default();
                    return Ok(());
                }
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        Err(Error::Offline("Synthetic preflight hold expired".into()))
    }
    pub fn rotate(&self, shell: Signal<ShellState>) -> Result<(), String> {
        if !self.admits(&shell.peek()) {
            return Err("Cannot rotate foreign fixture".into());
        }
        // Rotate request credentials independently of account navigation. An
        // epoch change remounts ScheduledPanel and correctly cancels an unsent
        // editor; the owner-switch case exercises that separate boundary itself.
        self.generation.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    pub fn server(&self) -> Value {
        json!(*self.server.lock().unwrap())
    }
}
pub(crate) async fn install(state: Signal<ShellState>) -> Result<Arc<State>, String> {
    let (directory, owner) = harness::admit().await?;
    let scenario = std::env::var("SUPER_PLATINUM_SCHEDULED_ATTACHMENTS_SCENARIO")
        .map_err(|_| "Missing scenario")?;
    if !SCENARIOS.contains(&scenario.as_str()) {
        return Err("Unknown attachment case".into());
    }
    let first = match scenario.as_str() {
        "uncertain-owner" => Outcome::Uncertain,
        "known-unsent" => Outcome::NotSent,
        "rejected" => Outcome::Rejected,
        _ => Outcome::Confirmed,
    };
    let value = Arc::new(State {
        nonce: owner["nonce"].as_str().unwrap().into(),
        scenario,
        directory,
        generation: AtomicU64::new(1),
        ledger: Mutex::default(),
        server: Mutex::new(
            [DRAFT, EMPTY]
                .map(|id| (id.into(), raw(id)))
                .into_iter()
                .collect(),
        ),
        outcomes: Mutex::new(VecDeque::from([first, Outcome::Confirmed])),
        hold: Mutex::default(),
    });
    if !value.admits(&state.peek()) {
        return Err("Disconnected synthetic owner required".into());
    }
    *installed().lock().unwrap() = Arc::downgrade(&value);
    Ok(value)
}
pub(crate) fn owner(shell: &ShellState, draft: &str) -> Option<AttachmentOwner> {
    let fixture = installed().lock().ok()?.upgrade()?;
    (fixture.admits(shell) && [DRAFT, EMPTY].contains(&draft)).then(|| AttachmentOwner {
        account: ACCOUNT.into(),
        team: "T1".into(),
        user: "U00000001".into(),
        draft: draft.into(),
    })
}
#[derive(Clone)]
pub(crate) struct Binding {
    fixture: Arc<State>,
    epoch: u64,
    generation: u64,
    client: SlackClient,
    session: WorkspaceSession,
    draft: String,
}
impl Binding {
    pub fn capture(shell: &ShellState, lease: &Lease) -> Option<Self> {
        let fixture = installed().lock().ok()?.upgrade()?;
        if owner(shell, lease.draft_id()).as_ref() != Some(&lease.owner) {
            return None;
        }
        let generation = fixture.generation.load(Ordering::SeqCst);
        Some(Self {
            fixture,
            epoch: shell.core.account_epoch,
            generation,
            client: SlackClient::default(),
            draft: lease.draft_id().into(),
            session: WorkspaceSession {
                team_id: "T1".into(),
                enterprise_id: None,
                user_id: "U00000001".into(),
                name: "Isolated attachment fixture".into(),
                url: "https://fixture.slack.com".into(),
                token: format!("fixture-only-token-{generation}"),
            },
        })
    }
    pub fn current(&self, shell: &ShellState, lease: &Lease) -> bool {
        self.fixture.admits(shell)
            && shell.core.account_epoch == self.epoch
            && self.fixture.generation.load(Ordering::SeqCst) == self.generation
            && lease.draft_id() == self.draft
            && owner(shell, lease.draft_id()).as_ref() == Some(&lease.owner)
    }
    pub fn client(&self) -> &SlackClient {
        &self.client
    }
    pub fn session(&self) -> &WorkspaceSession {
        &self.session
    }
    pub fn epoch(&self) -> u64 {
        self.epoch
    }
    fn record(
        &self,
        request: &PreparedRequest,
        method: &str,
    ) -> Result<(usize, BTreeMap<String, String>), Error> {
        let RequestBody::Form(form) = &request.body else {
            return Err(Error::Api("fixture_form_required".into()));
        };
        let fields: BTreeMap<_, _> = form.iter().cloned().collect();
        if !self.fixture.configured()
            || request.method != "POST"
            || request.url.split('?').next()
                != Some(format!("https://fixture.slack.com/api/{method}").as_str())
            || fields.len() != form.len()
            || fields.get("token") != Some(&self.session.token)
            || fields.get("draft_id") != Some(&self.draft)
        {
            return Err(Error::Api("fixture_request_refused".into()));
        }
        let mut ledger = self.fixture.ledger.lock().unwrap();
        let index = ledger.len();
        ledger.push(json!({"method":request.method,"url":request.url,"form":form,"retrySafe":request.retry_safe(),"headers":request.headers,"fixtureGeneration":self.generation,"result":null}));
        Ok((index, fields))
    }
    fn complete(&self, index: usize, result: Value) {
        self.fixture.ledger.lock().unwrap()[index]["result"] = result;
    }
    pub async fn execute_read(&self, request: PreparedRequest) -> Result<Value, Error> {
        let (index, fields) = self.record(&request, "drafts.info")?;
        if fields.len() != 2 {
            return Err(Error::Api("fixture_read_fields".into()));
        }
        self.fixture.hold_read().await?;
        let draft = self
            .fixture
            .server
            .lock()
            .unwrap()
            .get(&self.draft)
            .cloned()
            .ok_or_else(|| Error::Api("fixture_missing_draft".into()))?;
        let result = json!({"ok":true,"draft":draft});
        self.complete(index, json!({"raw":result}));
        Ok(result)
    }
    pub async fn execute_mutation(&self, request: PreparedRequest) -> Result<Value, Error> {
        let (index, fields) = self.record(&request, "drafts.update")?;
        let outcome = self
            .fixture
            .outcomes
            .lock()
            .unwrap()
            .pop_front()
            .ok_or_else(|| Error::Api("fixture_unexpected_replay".into()))?;
        if matches!(outcome, Outcome::NotSent) {
            self.complete(index, json!({"error":"known-not-dispatched"}));
            return Err(Error::Api("draft_mutation_not_sent_offline".into()));
        }
        if matches!(outcome, Outcome::Rejected) {
            let raw = json!({"ok":false,"error":"token_expired"});
            self.complete(index, json!({"raw":raw}));
            return Ok(raw);
        }
        let mut server = self.fixture.server.lock().unwrap();
        let current = server
            .get_mut(&self.draft)
            .ok_or_else(|| Error::Api("fixture_draft_missing".into()))?;
        for name in ["blocks", "destinations", "file_ids"] {
            current[name] = serde_json::from_str(
                fields
                    .get(name)
                    .ok_or_else(|| Error::Api("fixture_missing_field".into()))?,
            )
            .map_err(|_| Error::Api("fixture_bad_json".into()))?;
        }
        current["date_scheduled"] = fields
            .get("date_scheduled")
            .and_then(|s| s.parse::<i64>().ok())
            .ok_or_else(|| Error::Api("fixture_bad_date".into()))?
            .into();
        current["last_updated_ts"] = format!("{}.000001", 20 + index).into();
        let response = json!({"ok":true,"draft":current});
        drop(server);
        if matches!(outcome, Outcome::Uncertain) {
            self.complete(
                index,
                json!({"error":"synthetic-response-lost","serverApplied":true}),
            );
            Err(Error::Transport(
                "Synthetic response lost after fixture application".into(),
            ))
        } else {
            self.complete(index, json!({"raw":response}));
            Ok(response)
        }
    }
}
pub(crate) fn metadata(
    state: Signal<ShellState>,
    mut operations: Signal<Operations>,
    lease: &Lease,
) {
    if Binding::capture(&state.peek(), lease).is_none() {
        return;
    }
    for (index, id) in FILES.iter().enumerate() {
        let title = match index {
            0 => "Roadmap \u{65e5}\u{672c}\u{8a9e}.txt",
            1 => "Launch \u{1f680}.pdf",
            _ => "Notes e\u{301}.txt",
        };
        let file: File = serde_json::from_value(
            json!({"id":id,"name":title,"title":title,"mimetype":"text/plain","size":1234}),
        )
        .unwrap();
        let _ = operations.write().metadata(lease, id, file);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fixture_order_undo_and_rich_payload_are_supported_by_actual_model() {
        let original: ScheduledMessage = serde_json::from_value(raw(DRAFT)).unwrap();
        let mut model = AttachmentEdit::new(original.clone()).unwrap();
        model.remove(FILES[1]).unwrap();
        model.restore(FILES[1]).unwrap();
        assert_eq!(model.file_ids(), original.file_ids);
        model.remove(FILES[0]).unwrap();
        let request = model.freeze().unwrap().request();
        assert_eq!(request.file_ids, vec![FILES[1], FILES[2]]);
        assert_eq!(request.blocks, original.blocks);
        assert_eq!(request.destinations, original.destinations);
        assert_eq!(request.client_msg_id, original.client_msg_id);
        assert_eq!(request.post_at, original.post_at);
    }
    #[test]
    fn fixture_empty_and_rich_last_file_cases_differ_without_invented_content() {
        for (id, allowed) in [(EMPTY, false), (DRAFT, true)] {
            let original: ScheduledMessage = serde_json::from_value(raw(id)).unwrap();
            let mut model = AttachmentEdit::new(original.clone()).unwrap();
            for file in &original.file_ids {
                model.remove(file).unwrap();
            }
            assert_eq!(model.freeze().is_ok(), allowed);
        }
    }
    #[test]
    fn injected_rejection_is_the_exact_supported_definite_envelope() {
        assert_eq!(
            super_platinum_core::scheduled::edit::definite_rejection(
                &json!({"ok":false,"error":"token_expired"})
            ),
            Some("token_expired")
        );
        assert_eq!(
            super_platinum_core::scheduled::edit::definite_rejection(
                &json!({"ok":false,"error":"permission_denied"})
            ),
            None
        );
    }
}
