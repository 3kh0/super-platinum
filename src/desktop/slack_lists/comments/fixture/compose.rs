//! Closed compose executor. Every request uses the product factory; no HTTP client exists.
use super::*;
use std::path::PathBuf;
use super_platinum_core::{
    CoreAppState,
    config::Session,
    slack::{Transport, api, transport::ListCommentReplyFailure},
    slack_lists::comments::{ReplyAccepted, ReplyIntent, ReplyPolicy},
    state::{
        Workspace,
        posting_metadata::authority::{Authority, BootRead},
    },
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Mode {
    #[default]
    Success,
    NotSent,
    Rejected,
    Ambiguous,
    HeldPreflight,
    HeldSuccess,
    HeldAmbiguous,
    Denied,
}
#[derive(Default)]
pub(super) struct Ledger {
    mode: Mode,
    prepared: Vec<Value>,
    posts: Vec<Value>,
    preflights: Vec<Value>,
    policy: Vec<Value>,
    messages: Vec<Value>,
    publish: bool,
    held: Option<oneshot::Sender<Value>>,
    held_post: Option<oneshot::Sender<()>>,
}
pub(crate) fn check_enabled() -> bool {
    std::env::var("SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_CHECK").as_deref() == Ok("1")
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("slack-list" | "slack-list-narrow")
        )
}
pub(crate) fn enabled(shell: &ShellState) -> bool {
    check_enabled()
        && super::enabled(shell)
        && shell.core.active_account.as_deref() == Some("8624eb74-23f4-4bee-9872-e37f392485c0")
}
pub(crate) fn path(shell: &ShellState) -> Option<PathBuf> {
    if !enabled(shell) {
        return None;
    }
    let directory = PathBuf::from(std::env::var_os(
        "SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_READY_DIR",
    )?);
    let path = PathBuf::from(std::env::var_os("SUPER_PLATINUM_LIST_COMMENT_JOURNAL")?);
    let nonce = std::env::var("SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_NONCE").ok()?;
    if nonce.len() != 32
        || !nonce
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        || !directory.is_absolute()
        || directory.file_name()?.to_str()? != format!("slack-list-comments-compose-{nonce}")
        || path != directory.join("comments.sqlite")
        || !directory.is_dir()
    {
        return None;
    }
    for parent in directory.ancestors() {
        let meta = std::fs::symlink_metadata(parent).ok()?;
        if meta.file_type().is_symlink() {
            return None;
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if meta.file_attributes() & 0x400 != 0 {
                return None;
            }
        }
    }
    Some(path)
}
pub(crate) fn session(shell: &ShellState) -> Result<WorkspaceSession, Error> {
    if !enabled(shell) {
        return Err(unavailable());
    }
    let team = shell.core.active_team.as_ref().ok_or_else(unavailable)?;
    let workspace = shell.core.workspaces.get(team).ok_or_else(unavailable)?;
    Ok(WorkspaceSession {
        team_id: team.clone(),
        user_id: workspace.self_user_id.clone(),
        ..super::session()
    })
}
pub(crate) fn mode(model: &Model, mode: Mode) -> Result<(), String> {
    let mut ledger = model.0.lock().map_err(|_| "Compose ledger poisoned")?;
    if ledger.compose.held.is_some() || ledger.compose.held_post.is_some() {
        return Err("Release original work first".into());
    }
    ledger.compose.mode = mode;
    Ok(())
}
fn wire(request: &PreparedRequest) -> Result<Value, Error> {
    let url = url::Url::parse(&request.url).map_err(|_| unavailable())?;
    if request.method != "POST"
        || url.scheme() != "https"
        || url.host_str() != Some("example.test")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(unavailable());
    }
    let RequestBody::Form(fields) = &request.body else {
        return Err(unavailable());
    };
    let mut args = BTreeMap::new();
    let mut token = false;
    for (k, v) in fields {
        if k == "token" {
            if token || v != "fixture-not-a-token" {
                return Err(unavailable());
            }
            token = true;
        } else if args.insert(k.clone(), v.clone()).is_some() {
            return Err(unavailable());
        }
    }
    if !token {
        return Err(unavailable());
    }
    Ok(json!({"method":url.path().strip_prefix("/api/").ok_or_else(unavailable)?,"args":args}))
}
fn same_request(request: &PreparedRequest, expected: &PreparedRequest) -> Result<bool, Error> {
    // Factory calls generate new trace IDs. The closed wire parser validates
    // the endpoint, credentials and exact form independently of those IDs.
    Ok(request.headers == expected.headers && wire(request)? == wire(expected)?)
}
fn channel() -> Value {
    json!({"id":"CCOMMENT01","context_team_id":"T1","is_general":true,
        "is_non_threadable":false,"is_archived":false,"is_frozen":false,"is_locked":false})
}
pub(crate) async fn preflight_request(
    state: Signal<ShellState>,
    request: &PreparedRequest,
) -> Result<Value, Error> {
    let model = {
        let s = state.peek();
        if !enabled(&s) {
            return Err(unavailable());
        }
        s.slack_lists.comments.fixture.clone().unwrap()
    };
    let form = wire(request)?;
    if form["method"] == "reactions.get" {
        return super::reaction_actions::get(state, request).await;
    }
    if form["method"] != "conversations.genericInfo" {
        return super::request(state, request).await;
    }
    if form["args"] != json!({"updated_channels":"{\"CCOMMENT01\":0}"}) {
        return Err(unavailable());
    }
    let mut raw = channel();
    raw["context_team_id"] = json!(session(&state.peek())?.team_id);
    let (response, wait) = {
        let mut l = model.0.lock().map_err(|_| unavailable())?;
        l.compose.preflights.push(form);
        let mode = l.compose.mode;
        if mode == Mode::Denied {
            raw["is_non_threadable"] = json!(true);
            l.compose.mode = Mode::Success;
        }
        let response = json!({"ok":true,"channels":[raw]});
        if mode == Mode::HeldPreflight {
            l.compose.mode = Mode::Success;
            let (tx, rx) = oneshot::channel();
            l.compose.held = Some(tx);
            (response, Some(rx))
        } else {
            (response, None)
        }
    };
    if let Some(rx) = wait {
        rx.await.map_err(|_| unavailable())
    } else {
        Ok(response)
    }
}
pub(crate) fn release(model: &Model, team: &str) -> Result<Value, String> {
    let tx = model
        .0
        .lock()
        .map_err(|_| "Compose ledger poisoned")?
        .compose
        .held
        .take()
        .ok_or("No held preflight")?;
    let cancelled = tx.is_closed();
    let mut raw = channel();
    raw["context_team_id"] = json!(team);
    let delivered = tx.send(json!({"ok":true,"channels":[raw]})).is_ok();
    Ok(json!({"delivered":delivered,"cancelledBeforeReply":cancelled}))
}
pub(crate) fn policy(state: Signal<ShellState>, raw: Value) -> Result<ReplyPolicy, Error> {
    let shell = state.peek();
    let credential = session(&shell)?;
    let model = shell
        .slack_lists
        .comments
        .fixture
        .clone()
        .ok_or_else(unavailable)?;
    // This separate core is only an identity carrier; disconnected_fixture has no HTTP client.
    let mut core = CoreAppState::new(Default::default());
    core.active_account = shell.core.active_account.clone();
    core.account_epoch = shell.core.account_epoch;
    let transport = Arc::new(Transport::disconnected_fixture("fixture-cookie"));
    core.transport = Some(transport.clone());
    core.workspaces.insert(
        credential.team_id.clone(),
        Workspace::from_session(&credential),
    );
    core.session = Some(Session {
        d_cookie: "fixture-cookie".into(),
        unified_user_client_enabled: BTreeMap::from([(credential.team_id.clone(), true)]),
        workspaces: BTreeMap::from([(credential.team_id.clone(), credential.clone())]),
    });
    let read = BootRead::begin(&mut core, &transport, &credential).map_err(|_| unavailable())?;
    let request = read.request(&core).map_err(|_| unavailable())?;
    let form = wire(&request)?;
    if form != json!({"method":"client.userBoot","args":{"include_min_version_bump_check":"1"}}) {
        return Err(unavailable());
    }
    let boot = json!({"ok":true,"team":{"id":credential.team_id,"plan":"plus"},
        "self":{"id":credential.user_id,"team_id":credential.team_id},
        "account_types":{"is_admin":[],"is_owner":[],"is_primary_owner":[]},
        "workspaces":[{"id":credential.team_id}],"read_only_channels":[],"thread_only_channels":[]});
    if !read.apply(&mut core, boot.clone(), false)? {
        return Err(unavailable());
    }
    let authority = Authority::capture(&core, &credential.team_id).map_err(|_| unavailable())?;
    let read = authority
        .begin_channel_read(&mut core, "CCOMMENT01")
        .map_err(|_| unavailable())?;
    let witness = read.apply(&mut core, raw).map_err(|_| unavailable())?;
    let read_only = authority
        .metadata(&core)
        .map_err(|_| unavailable())?
        .thread_read_only(authority.configuration(), witness.value())
        .map_err(|_| unavailable())?;
    model.0.lock().map_err(|_|unavailable())?.compose.policy.push(json!({"request":form,"boot":boot,"channel":witness.value(),"threadReadOnly":read_only}));
    Ok(ReplyPolicy::observed(
        witness.value().clone(),
        false,
        !read_only,
    ))
}
pub(crate) async fn post(
    state: Signal<ShellState>,
    request: &PreparedRequest,
    intent: ReplyIntent,
) -> Result<ReplyAccepted, ListCommentReplyFailure> {
    use ListCommentReplyFailure as Failure;
    let (model, form) = {
        let shell = state.peek();
        let credential = session(&shell).map_err(Failure::NotRequested)?;
        let exact = api::list_record_comment_reply(&shell.core.client, &credential, &intent)
            .map_err(Failure::NotRequested)?;
        let form = wire(request).map_err(Failure::NotRequested)?;
        if !same_request(request, &exact).map_err(Failure::NotRequested)? {
            return Err(Failure::NotRequested(unavailable()));
        }
        (shell.slack_lists.comments.fixture.clone().unwrap(), form)
    };
    let (mode, raw, wait) = {
        let mut l = model
            .0
            .lock()
            .map_err(|_| Failure::NotRequested(unavailable()))?;
        let l = &mut l.compose;
        l.prepared.push(form.clone());
        let mode = std::mem::take(&mut l.mode);
        if mode == Mode::NotSent {
            return Err(Failure::NotRequested(Error::Offline(
                "Synthetic before dispatch".into(),
            )));
        }
        l.posts.push(form);
        if mode == Mode::Rejected {
            return Err(Failure::Rejected(
                json!({"ok":false,"error":"restricted_action"}),
            ));
        }
        let record = intent.record();
        let ts = format!("1781525{:03}.000001", l.posts.len());
        let mut msg = json!({"type":"message","channel":record.target().channel_id(),"thread_ts":record.target().root_ts(),
            "ts":ts,"user":record.user(),"client_msg_id":record.client_msg_id(),"text":record.text(),"blocks":record.blocks()});
        // Capture this original response before holding it. Later socket reaction
        // removal must survive this deliberately older, immutable ACK.
        if matches!(
            std::env::var("SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_CASE").as_deref(),
            Ok("ack-socket-remove" | "edit-ack")
        ) {
            msg["reactions"] = json!([{"name":"wave","count":1,"users":["U_RT_REACTOR"]}]);
        }
        l.messages.push(msg.clone());
        let wait = if matches!(mode, Mode::HeldSuccess | Mode::HeldAmbiguous) {
            let (tx, rx) = oneshot::channel();
            l.held_post = Some(tx);
            Some(rx)
        } else {
            None
        };
        (
            mode,
            json!({"ok":true,"channel":record.target().channel_id(),"ts":ts,"message":msg}),
            wait,
        )
    };
    if let Some(rx) = wait {
        rx.await.map_err(|_| Failure::Unconfirmed {
            error: unavailable(),
            receipt: None,
        })?;
    }
    if matches!(mode, Mode::Ambiguous | Mode::HeldAmbiguous) {
        return Err(Failure::Unconfirmed {
            error: Error::Transport("Synthetic response lost after dispatch".into()),
            receipt: None,
        });
    }
    intent
        .accept(raw.clone())
        .map_err(|_| Failure::Unconfirmed {
            error: unavailable(),
            receipt: Some(raw),
        })
}
pub(crate) fn release_post(model: &Model) -> Result<Value, String> {
    let tx = model
        .0
        .lock()
        .map_err(|_| "Compose ledger poisoned")?
        .compose
        .held_post
        .take()
        .ok_or("No held post")?;
    let cancelled = tx.is_closed();
    let delivered = tx.send(()).is_ok();
    Ok(json!({"delivered":delivered,"cancelledBeforeReply":cancelled}))
}
pub(crate) fn publish(model: &Model) -> Result<(), String> {
    model
        .0
        .lock()
        .map_err(|_| "Compose ledger poisoned")?
        .compose
        .publish = true;
    Ok(())
}
pub(crate) fn unpublish(model: &Model) -> Result<(), String> {
    model
        .0
        .lock()
        .map_err(|_| "Compose ledger poisoned")?
        .compose
        .publish = false;
    Ok(())
}
pub(super) fn append_readback(ledger: &Ledger, response: &mut Value) {
    if !ledger.publish {
        return;
    }
    let root = response["messages"][0]["ts"].clone();
    if let Some(rows) = response["messages"].as_array_mut() {
        rows.extend(
            ledger
                .messages
                .iter()
                .filter(|m| m["thread_ts"] == root)
                .cloned(),
        );
    }
}
pub(crate) fn snapshot(shell: &ShellState, model: &Model) -> Value {
    let mut value = super::snapshot(shell, model);
    let ledger = model.0.lock().unwrap();
    let c = &ledger.compose;
    value["composeLedger"] = json!({"prepared":c.prepared,"posts":c.posts,"preflights":c.preflights,"policies":c.policy,
        "messages":c.messages,"published":c.publish,"held":c.held.is_some(),"heldPost":c.held_post.is_some()});
    value["draftStore"] = super::super::draft::fixture_snapshot(shell);
    value
}

/// Reopen the isolated database for evidence independent of the in-memory draft.
pub(crate) async fn journal_snapshot(state: Signal<ShellState>) -> Result<Value, String> {
    use super_platinum_core::slack_lists::comments::journal::{Journal, Scope};
    let (path, account, scope) = {
        let shell = state.peek();
        if !enabled(&shell)
            || ((shell.core.session.is_some() || shell.core.transport.is_some())
                && !super::completion::enabled(&shell))
        {
            return Err("Disconnected compose fixture required".into());
        }
        let credential = session(&shell).map_err(|_| "Fixture session missing")?;
        let target = &shell
            .slack_lists
            .comments
            .entry
            .as_ref()
            .ok_or("Acquired comment target missing")?
            .target;
        (
            path(&shell).ok_or("Isolated journal missing")?,
            shell
                .core
                .active_account
                .clone()
                .ok_or("Fixture account missing")?,
            Scope::capture(&credential.team_id, &credential.user_id, target)
                .map_err(|_| "Journal scope invalid")?,
        )
    };
    tokio::task::spawn_blocking(move || {
        let journal = Journal::open(path, &account).map_err(|e| e.to_string())?;
        let inventory = journal.inventory(&scope).map_err(|e| e.to_string())?;
        serde_json::to_value(inventory).map_err(|e| e.to_string())
    })
    .await
    .map_err(|_| "Journal snapshot task failed")?
}

/// Seed existing opaque content through the same isolated durable store used by the editor.
pub(crate) async fn seed_draft(state: Signal<ShellState>) -> Result<Value, String> {
    use super_platinum_core::slack_lists::comments::journal::{Journal, Scope};
    let completion = super::completion::enabled(&state.peek());
    let (path, account, scope) = {
        let shell = state.peek();
        let credential = session(&shell).map_err(|_| "Fixture session missing")?;
        let target = &shell
            .slack_lists
            .comments
            .entry
            .as_ref()
            .ok_or("Acquired comment target missing")?
            .target;
        (
            path(&shell).ok_or("Isolated journal missing")?,
            shell
                .core
                .active_account
                .clone()
                .ok_or("Fixture account missing")?,
            Scope::capture(&credential.team_id, &credential.user_id, target)
                .map_err(|_| "Journal scope invalid")?,
        )
    };
    tokio::task::spawn_blocking(move||{
        let mut journal=Journal::open(&path,&account).map_err(|e|e.to_string())?;
        let inventory=journal.inventory(&scope).map_err(|e|e.to_string())?;
        let mut blocks=vec![json!({"type":"rich_text","elements":[{"type":"rich_text_section","elements":[
            {"type":"text","text":"Existing context "},{"type":"user","user_id":"U_FIXTURE_OPAQUE","opaque":{"retain":[1,2]}},
            {"type":"text","text":" "},{"type":"link","url":"https://example.test/reference","text":"Reference","opaque":{"source":"fixture"}},
            {"type":"text","text":"\n"}]}]})];
        if completion {
            let elements=blocks[0]["elements"][0]["elements"].as_array_mut().unwrap();
            elements.insert(4,json!({"type":"channel","channel_id":super::completion::LOCAL,"style":{"bold":true},"opaque":{"occurrence":1}}));
            elements.insert(5,json!({"type":"channel","channel_id":super::completion::LOCAL,"style":{"italic":true},"opaque":{"occurrence":2}}));
        }
        let saved=journal.save(&inventory,&uuid::Uuid::new_v4().to_string(),blocks).map_err(|e|e.to_string())?;
        drop(journal);
        let reopened=Journal::open(&path,&account).map_err(|e|e.to_string())?.inventory(&scope).map_err(|e|e.to_string())?;
        if serde_json::to_value(&saved).ok()!=serde_json::to_value(&reopened).ok(){return Err("Journal reopen changed seeded content".into());}
        Ok(json!({"scope":scope,"blocks":reopened.blocks(),"revision":reopened.revision(),"reopened":true}))
    }).await.map_err(|_|"Journal seed task failed")?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn readback_requires_publication_and_exact_original_thread() {
        let mut ledger = Ledger::default();
        let exact = json!({"ts":"1781525001.000001","thread_ts":ROOT,"client_msg_id":"original","blocks":[{"opaque":true}]});
        ledger.messages = vec![
            exact.clone(),
            json!({"ts":"1781525002.000001","thread_ts":SYNTHETIC_ROOT}),
        ];
        let initial = json!({"ok":true,"messages":[{"ts":ROOT}]});
        let mut raw = initial.clone();
        append_readback(&ledger, &mut raw);
        assert_eq!(raw, initial);
        ledger.publish = true;
        append_readback(&ledger, &mut raw);
        assert_eq!(raw["messages"], json!([{"ts":ROOT},exact]));
    }
    #[test]
    fn generic_info_admission_keeps_actual_factory_fields() {
        let request = api::conversations_generic_info(
            &Default::default(),
            &super::super::session(),
            &BTreeMap::from([("CCOMMENT01".into(), 0)]),
        );
        assert_eq!(
            wire(&request).unwrap(),
            json!({"method":"conversations.genericInfo","args":{"updated_channels":"{\"CCOMMENT01\":0}"}})
        );
        let mut duplicate = request.clone();
        if let RequestBody::Form(fields) = &mut duplicate.body {
            fields.push(("token".into(), "fixture-not-a-token".into()));
        }
        assert!(wire(&duplicate).is_err());
        let mut foreign = request;
        foreign.url = "https://slack.com/api/conversations.genericInfo".into();
        assert!(wire(&foreign).is_err());
    }
    #[test]
    fn request_admission_ignores_fresh_trace_ids_but_preserves_exact_payload() {
        let factory = || {
            api::conversations_generic_info(
                &Default::default(),
                &super::super::session(),
                &BTreeMap::from([("CCOMMENT01".into(), 0)]),
            )
        };
        let request = factory();
        let mut regenerated = factory();
        assert_ne!(request.url, regenerated.url);
        assert!(same_request(&request, &regenerated).unwrap());
        if let RequestBody::Form(fields) = &mut regenerated.body {
            fields.push(("unexpected".into(), "extra".into()));
        }
        assert!(!same_request(&request, &regenerated).unwrap());
        regenerated.url = "https://slack.com/api/conversations.genericInfo".into();
        assert!(same_request(&request, &regenerated).is_err());
    }
}
