//! Exact named synthetic HTTP boundary. Same builders, positive decoder and reducers.
use super::*;
use std::sync::{Mutex, OnceLock};
use super_platinum_core::{
    config::WorkspaceSession,
    slack::{
        client::{PreparedRequest, RequestBody},
        transport::ListColumnRenameFailure,
    },
};
#[derive(Default)]
struct Ledger {
    requests: Vec<Value>,
    responses: Vec<Value>,
    pending: Option<tokio::sync::oneshot::Sender<()>>,
    held_read: Option<tokio::sync::oneshot::Sender<()>>,
    hold_next_read: bool,
    finished: Vec<Value>,
    server: Option<Value>,
}
fn ledger() -> &'static Mutex<Ledger> {
    static LEDGER: OnceLock<Mutex<Ledger>> = OnceLock::new();
    LEDGER.get_or_init(Default::default)
}
pub(crate) fn check_enabled() -> bool {
    std::env::var("SUPER_PLATINUM_LIST_RENAME_CHECK").as_deref() == Ok("1")
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("slack-list-column-rename" | "slack-list-column-rename-narrow")
        )
}
pub(crate) fn view_check_enabled() -> bool {
    std::env::var("SUPER_PLATINUM_LIST_VIEW_RENAME_CHECK").as_deref() == Ok("1")
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("slack-list-view-rename" | "slack-list-view-rename-narrow")
        )
}
pub(crate) fn create_check_enabled() -> bool {
    std::env::var("SUPER_PLATINUM_LIST_VIEW_CREATE_CHECK").as_deref() == Ok("1")
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("slack-list-view-create" | "slack-list-view-create-narrow")
        )
}
pub(super) fn enabled(shell: &ShellState) -> bool {
    (check_enabled() || view_check_enabled() || create_check_enabled())
        && super::super::fixture::enabled(shell)
        && shell.core.session.is_none()
        && shell.core.transport.is_none()
}
pub(crate) fn install(shell: &mut ShellState, name: &str) {
    let server = if name.starts_with("slack-list-view-create") {
        super::super::fixture::view_create::install(shell, name)
    } else if name.starts_with("slack-list-view-rename") {
        super::super::fixture::view_rename::install(shell, name)
    } else {
        super::super::fixture::column_rename::install(shell, name)
    };
    // Only this exact disconnected seed initializes the private synthetic server.
    *ledger().lock().unwrap() = Ledger {
        server: Some(server),
        ..Default::default()
    };
}
pub(super) fn session() -> WorkspaceSession {
    WorkspaceSession {
        team_id: "T1".into(),
        user_id: "U1".into(),
        enterprise_id: None,
        name: "Fixture".into(),
        url: "https://example.test".into(),
        token: "fixture-not-a-token".into(),
    }
}
fn record(request: PreparedRequest) -> Result<String, Error> {
    let url = url::Url::parse(&request.url).map_err(|_| Error::Api("fixture_url".into()))?;
    if request.method != "POST" || url.host_str() != Some("example.test") {
        return Err(Error::Api("fixture_request_origin".into()));
    }
    let RequestBody::Form(fields) = request.body else {
        return Err(Error::Api("fixture_form".into()));
    };
    if fields.first() != Some(&("token".into(), "fixture-not-a-token".into())) {
        return Err(Error::Api("fixture_token".into()));
    }
    let path = url.path().to_owned();
    ledger()
        .lock()
        .unwrap()
        .requests
        .push(json!({"method":"POST","path":path,"fields":fields}));
    Ok(path)
}
pub(super) async fn read(
    state: Signal<ShellState>,
    guard: &Guard,
    stage: Stage,
    request: PreparedRequest,
) -> Result<Value, Error> {
    if !enabled(&state.peek()) || !guard.current(&state.peek(), stage) {
        return Err(Error::Api("fixture_retired".into()));
    }
    let path = record(request)?;
    let (raw, held) = {
        let mut l = ledger().lock().unwrap();
        let raw = match path.as_str() {
            "/api/files.info" => json!({"ok":true,"file":l.server.clone().unwrap()}),
            "/api/team.prefs.get" => json!({"ok":true,"prefs":{"allow_lists":"full"}}),
            "/api/users.prefs.get" => {
                json!({"ok":true,"prefs":{"task_lists":"[]","assigned_to_list":""}})
            }
            _ => return Err(Error::Api("fixture_unknown_read".into())),
        };
        l.responses.push(raw.clone());
        let held = if stage == Stage::Checking && path == "/api/files.info" && l.hold_next_read {
            l.hold_next_read = false;
            let (tx, rx) = tokio::sync::oneshot::channel();
            l.held_read = Some(tx);
            Some(rx)
        } else {
            None
        };
        (raw, held)
    };
    if let Some(rx) = held {
        tokio::time::timeout(std::time::Duration::from_secs(30), rx)
            .await
            .map_err(|_| Error::Transport("fixture_timeout".into()))?
            .map_err(|_| Error::Transport("fixture_closed".into()))?;
    }
    // Exercise the same strict public JSON decoder before the positive file/policy parser.
    super_platinum_core::slash_commands::codec::decode(
        &serde_json::to_vec(&raw).unwrap(),
        1024 * 1024,
    )
    .map_err(|_| Error::Api("fixture_decode".into()))
}
pub(super) async fn write(
    state: Signal<ShellState>,
    guard: &Guard,
    permit: Permit,
) -> Result<Value, ListColumnRenameFailure> {
    if !enabled(&state.peek())
        || !guard.current(&state.peek(), Stage::Saving)
        || registry::blocked(&guard.key.owner)
    {
        return Err(ListColumnRenameFailure::NotRequested(
            Error::TransportNotConfigured,
        ));
    }
    let request = permit.request(&state.peek().core.client, &session());
    let desired = with_registry(|r| {
        r.entries[&guard.key]
            .intent
            .as_ref()
            .unwrap()
            .desired_name()
            .to_owned()
    });
    let target = with_registry(|r| r.entries[&guard.key].target.clone());
    record(request).map_err(ListColumnRenameFailure::NotRequested)?;
    let (tx, rx) = tokio::sync::oneshot::channel();
    {
        let mut l = ledger().lock().unwrap();
        let positive = target.creation().is_some()
            && std::env::var("SUPER_PLATINUM_FIXTURE").as_deref() == Ok("slack-list-view-create");
        if positive {
            let mut server = l.server.clone().unwrap();
            super::super::fixture::view_create::add(
                &mut server,
                &desired,
                target.creation().unwrap(),
                target.grouping(),
            );
            // Deliberately a mutation response.list, never files.info.file.
            l.responses.push(json!({"ok":true,"list":{"id":server["id"],"list_metadata":server["list_metadata"]},"opaque_reply":{"retain":true}}));
        } else {
            l.responses
                .push(json!({"syntheticFailure":"response_lost"}));
        }
        if l.pending.is_some() {
            return Err(ListColumnRenameFailure::Unconfirmed(Error::Transport(
                "fixture_duplicate_write".into(),
            )));
        }
        l.pending = Some(tx);
    }
    tokio::time::timeout(std::time::Duration::from_secs(30), rx)
        .await
        .map_err(|_| {
            ListColumnRenameFailure::Unconfirmed(Error::Transport("fixture_timeout".into()))
        })?
        .map_err(|_| {
            ListColumnRenameFailure::Unconfirmed(Error::Transport("fixture_closed".into()))
        })?;
    {
        let mut l = ledger().lock().unwrap();
        if let Some(layout) = target.creation() {
            super::super::fixture::view_create::add(
                l.server.as_mut().unwrap(),
                &desired,
                layout,
                target.grouping(),
            );
            if std::env::var("SUPER_PLATINUM_FIXTURE").as_deref() == Ok("slack-list-view-create") {
                return Ok(l.responses.last().unwrap().clone());
            }
        } else {
            let schema = l.server.as_mut().unwrap()["list_metadata"]
                [if target.is_view() { "views" } else { "schema" }]
            .as_array_mut()
            .unwrap();
            schema
                .iter_mut()
                .find(|c| c["id"].as_str() == target.id())
                .unwrap()["name"] = json!(desired);
        }
    }
    Err(ListColumnRenameFailure::Unconfirmed(Error::Transport(
        "fixture_response_lost".into(),
    )))
}
pub(super) fn finished(guard: &Guard, kind: &str, applied: bool) {
    if check_enabled() || view_check_enabled() || create_check_enabled() {
        ledger()
            .lock()
            .unwrap()
            .finished
            .push(json!({"serial":guard.serial,"kind":kind,"applied":applied}));
    }
}
pub(crate) fn release(shell: &ShellState, read: bool) -> Result<(), String> {
    if !enabled(shell) {
        return Err("Disconnected rename fixture required".into());
    }
    let tx = {
        let mut l = ledger().lock().unwrap();
        if read {
            l.held_read.take()
        } else {
            l.pending.take()
        }
    }
    .ok_or("Missing held response")?;
    tx.send(()).map_err(|_| "Held response closed".into())
}
pub(crate) fn hold_read(shell: &ShellState) -> Result<(), String> {
    if !enabled(shell) {
        return Err("Disconnected rename fixture required".into());
    }
    ledger().lock().unwrap().hold_next_read = true;
    Ok(())
}
pub(crate) fn snapshot(shell: &ShellState) -> Result<Value, String> {
    if !enabled(shell) {
        return Err("Disconnected rename fixture required".into());
    }
    let l = ledger().lock().unwrap();
    Ok(
        json!({"requests":l.requests,"responses":l.responses,"finished":l.finished,"server":l.server,"writePending":l.pending.is_some(),"readPending":l.held_read.is_some()}),
    )
}
pub(crate) fn reopen(shell: &mut ShellState) -> Result<u64, String> {
    if !enabled(shell) {
        return Err("Disconnected rename fixture required".into());
    }
    let server = ledger().lock().unwrap().server.clone().unwrap();
    // Opening is a new actual List visit. Canonical rows still come from the
    // untouched fixture population, never from the held metadata response.
    super::super::open_shell(
        shell,
        serde_json::from_value(server).map_err(|_| "fixture_file")?,
    );
    super::super::fixture::refresh(shell);
    generation(shell).ok_or("No reopened List".into())
}

/// Passive fixture inspection of the production retained receipt, not a synthetic acknowledgement.
pub(crate) fn retained_reply(shell: &ShellState) -> Option<Value> {
    if !enabled(shell) {
        return None;
    }
    entry(shell)?.raw_reply
}

/// Exact disconnected source mutation for a schema-race preflight. No live reads/writes.
pub(crate) fn grouping_schema(shell: &ShellState, key: &str, changed: bool) -> Result<(), String> {
    if !create_check_enabled() || !enabled(shell) {
        return Err("Disconnected new-view fixture required".into());
    }
    let original = shell
        .slack_lists
        .view
        .as_ref()
        .and_then(|v| v.loaded.as_ref())
        .and_then(|l| l.document.columns.iter().find(|c| c.key == key))
        .ok_or("Missing fixture grouping column")?
        .raw
        .clone();
    let mut l = ledger().lock().unwrap();
    if l.pending.is_some() || l.held_read.is_some() {
        return Err("Pending fixture response".into());
    }
    let column = l.server.as_mut().ok_or("Missing fixture server")?["list_metadata"]["schema"]
        .as_array_mut()
        .ok_or("Missing fixture schema")?
        .iter_mut()
        .find(|c| c["key"] == key)
        .ok_or("Missing fixture server grouping column")?;
    *column = original;
    if changed {
        column["type"] = json!("text");
    }
    Ok(())
}
