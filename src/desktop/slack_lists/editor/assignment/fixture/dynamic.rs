//! Request-bound disconnected I/O for the unchanged assignee worker algorithm.
//! No real session or Transport is constructed; no response directly admits a cell.
use super::*;
mod model;
mod wire;
use std::{collections::BTreeSet, sync::Mutex};
use super_platinum_core::slack::{
    client::{PreparedRequest, RequestBody},
    transport::ListAssigneeFailure,
};
use tokio::sync::oneshot;
type Reply = Result<Value, ListAssigneeFailure>;
struct Pending {
    serial: u64,
    draft: u64,
    job: u64,
    method: String,
    fields: Vec<(String, String)>,
    reply: oneshot::Sender<Reply>,
}
struct State {
    scope: Scope,
    list: String,
    requests: Vec<Value>,
    completed: Vec<Value>,
    cells: Vec<Value>,
    cell_completions: Vec<Value>,
    failure: Option<String>,
    visible: BTreeSet<String>,
    hold: BTreeSet<String>,
    pending: Option<Pending>,
}
fn slot<T>(f: impl FnOnce(&mut Option<State>) -> T) -> T {
    static SLOT: std::sync::OnceLock<Mutex<Option<State>>> = std::sync::OnceLock::new();
    f(&mut SLOT
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner()))
}
pub(crate) fn requested() -> bool {
    std::env::var("SUPER_PLATINUM_LIST_ASSIGNMENT_CHECK").as_deref() == Ok("1")
}
pub(crate) fn enabled(shell: &ShellState) -> bool {
    requested()
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("slack-list-task-assignment" | "slack-list-task-assignment-narrow")
        )
        && crate::slack_lists::fixture::enabled(shell)
        && shell.core.session.is_none()
        && shell.core.transport.is_none()
}
pub(crate) fn install(shell: &mut ShellState) -> Result<(), String> {
    if !enabled(shell) {
        return Err("Closed disconnected assignment fixture required".into());
    }
    let workspace = shell
        .core
        .active_team
        .as_ref()
        .and_then(|team| shell.core.workspaces.get_mut(team))
        .ok_or("Missing fixture workspace")?;
    workspace.users.clear();
    for (id, label) in [
        ("U00000001", "Alice Example"),
        ("U00000002", "Bob Guest"),
        ("W00000003", "Whitney External"),
    ] {
        workspace.users.insert(id.into(),serde_json::from_value(json!({"id":id,"name":label,"real_name":label,"profile":{"display_name":label},"is_stranger":id=="W00000003","is_restricted":id=="U00000002"})).map_err(|_|"Fixture user")?);
    }
    let scope = shell
        .slack_lists
        .scope
        .clone()
        .ok_or("Missing fixture scope")?;
    let list = shell
        .slack_lists
        .view
        .as_ref()
        .ok_or("Missing fixture List")?
        .id
        .clone();
    slot(|s| {
        if s.is_some() {
            return Err("Assignment I/O already installed".into());
        }
        *s = Some(State {
            scope,
            list,
            requests: vec![],
            completed: vec![],
            cells: vec![],
            cell_completions: vec![],
            failure: None,
            visible: BTreeSet::from(["U00000001".into()]),
            hold: BTreeSet::from(["files.share".into()]),
            pending: None,
        });
        Ok(())
    })
}
fn current(s: &State, shell: &ShellState) -> bool {
    enabled(shell) && same_owner(s, shell)
}
fn same_owner(s: &State, shell: &ShellState) -> bool {
    s.scope.current(shell)
        && shell
            .slack_lists
            .scope
            .as_ref()
            .is_some_and(|scope| Arc::ptr_eq(&scope.identity, &s.scope.identity))
        && shell
            .slack_lists
            .view
            .as_ref()
            .is_some_and(|v| v.id == s.list)
}
pub(in crate::slack_lists::editor::assignment) fn owns(shell: &ShellState, guard: &Guard) -> bool {
    slot(|s| {
        s.as_ref().is_some_and(|s| {
            current(s, shell)
                && Arc::ptr_eq(&s.scope.identity, &guard.scope.identity)
                && guard.current(shell)
        })
    })
}
fn refused() -> ListAssigneeFailure {
    ListAssigneeFailure::NotRequested(Error::TransportNotConfigured)
}
fn decoded(
    request: PreparedRequest,
) -> Result<(String, Vec<(String, String)>), ListAssigneeFailure> {
    let method = request
        .url
        .strip_prefix("https://example.test/api/")
        .and_then(|p| p.split('?').next())
        .ok_or_else(refused)?
        .to_owned();
    if !wire::url(&request.url, &method) {
        return Err(refused());
    }
    if request.method != "POST" || method.is_empty() || method.contains('/') {
        return Err(refused());
    }
    let RequestBody::Form(mut fields) = request.body else {
        return Err(refused());
    };
    if fields.first() != Some(&("token".into(), "fixture-not-a-token".into())) {
        return Err(refused());
    }
    fields.remove(0);
    let mut keys = BTreeSet::new();
    if fields
        .iter()
        .any(|(k, _)| k == "token" || !keys.insert(k.clone()))
    {
        return Err(refused());
    }
    Ok((method, fields))
}
fn field<'a>(fields: &'a [(String, String)], name: &str) -> Option<&'a str> {
    fields
        .iter()
        .find(|(k, _)| k == name)
        .map(|(_, v)| v.as_str())
}
fn dm(user: &str) -> Option<Value> {
    let id = match user {
        "U00000002" => "D00000002",
        "W00000003" => "D00000003",
        _ => return None,
    };
    Some(
        json!({"ok":true,"channel":{"id":id,"user":user,"is_im":true,"is_ext_shared":user=="W00000003"}}),
    )
}
fn response(s: &State, shell: &ShellState, method: &str, fields: &[(String, String)]) -> Reply {
    if !wire::valid(method, fields, &s.list, &s.scope.team) {
        return Err(refused());
    }
    let loaded = crate::slack_lists::fixture::deletion_loaded(shell).ok_or_else(refused)?;
    match method {
        "files.info" if field(fields, "file") == Some(s.list.as_str()) => {
            Ok(json!({"ok":true,"file":loaded.document.raw}))
        }
        "lists.records.list" if field(fields, "list_id") == Some(s.list.as_str()) => {
            Ok(loaded.records.raw)
        }
        "team.prefs.get" => Ok(json!({"ok":true,"prefs":{"allow_lists":"full"}})),
        "users.prefs.get" => {
            Ok(json!({"ok":true,"prefs":{"task_lists":"[]","assigned_to_list":""}}))
        }
        "experiments.getByUser" => Ok(
            json!({"ok":true,"assignments":{"lists_pilot":{"group":"on"},"lists_pilot_teams":{"group":"on"}}}),
        ),
        "files.canSee" if field(fields, "file_id") == Some(s.list.as_str()) => {
            let id = field(fields, "user_id").ok_or_else(refused)?;
            if !matches!(id, "U00000001" | "U00000002" | "W00000003") {
                return Err(refused());
            }
            Ok(json!({"ok":true,"can_see":s.visible.contains(id)}))
        }
        "conversations.open" => dm(field(fields, "users").ok_or_else(refused)?).ok_or_else(refused),
        "conversations.info" => dm(match field(fields, "channel") {
            Some("D00000002") => "U00000002",
            Some("D00000003") => "W00000003",
            _ => return Err(refused()),
        })
        .ok_or_else(refused),
        "files.share" if field(fields, "files") == Some(s.list.as_str()) => Ok(json!({"ok":true})),
        _ => Err(refused()),
    }
}
pub(in crate::slack_lists::editor::assignment) async fn execute(
    mut state: Signal<ShellState>,
    guard: &Guard,
    request: PreparedRequest,
) -> Reply {
    if !owns(&state.peek(), guard) {
        return Err(refused());
    }
    let url = request.url.clone();
    let (method, fields) = decoded(request)?;
    let pending = slot(
        |slot| -> Result<Result<Reply, oneshot::Receiver<Reply>>, ListAssigneeFailure> {
            let s = slot
                .as_mut()
                .filter(|s| current(s, &state.peek()))
                .ok_or_else(refused)?;
            if s.pending.is_some() || s.requests.len() >= 512 {
                return Err(refused());
            }
            let proof = json!({"method":method,"url":url,"fields":fields,"serial":guard.serial,"draft":guard.draft,"job":guard.job});
            s.requests.push(proof);
            // Validate the closed request route before a held request is admitted.
            let value = response(s, &state.peek(), &method, &fields)?;
            if s.hold.remove(&method) {
                let (tx, rx) = oneshot::channel();
                s.pending = Some(Pending {
                    serial: guard.serial,
                    draft: guard.draft,
                    job: guard.job,
                    method,
                    fields,
                    reply: tx,
                });
                Ok(Err(rx))
            } else {
                s.completed.push(json!({"serial":guard.serial,"draft":guard.draft,"job":guard.job,"method":method,"fields":fields,"outcome":"reply","applied":false}));
                Ok(Ok(Ok(value)))
            }
        },
    )?;
    let _ = state.write();
    match pending {
        Ok(value) => value,
        Err(rx) => rx.await.unwrap_or_else(|_| {
            Err(ListAssigneeFailure::Unconfirmed(Error::Transport(
                "Disconnected fixture response dropped".into(),
            )))
        }),
    }
}
pub(crate) fn hold_next(shell: &ShellState, method: &str) -> Result<(), String> {
    if !matches!(method, "files.info" | "files.share") {
        return Err("Unsupported held method".into());
    }
    slot(|slot| {
        let s = slot
            .as_mut()
            .filter(|s| current(s, shell))
            .ok_or("Stale fixture")?;
        if s.pending.is_some() {
            return Err("A request is already held".into());
        }
        s.hold.insert(method.into());
        Ok(())
    })
}
/// Only explicit checker actions release a captured request. It settles the
/// original worker even if the originating editor has since retired.
pub(crate) fn release(
    shell: &ShellState,
    serial: u64,
    job: u64,
    lost_reply: bool,
    apply: bool,
) -> Result<(), String> {
    let (tx, value) = slot(|slot| -> Result<_, String> {
        let s = slot
            .as_mut()
            .filter(|s| current(s, shell))
            .ok_or("Stale fixture")?;
        if s.pending
            .as_ref()
            .is_none_or(|p| p.serial != serial || p.job != job)
        {
            return Err("No exact held assignment request".into());
        }
        if apply
            && s.pending
                .as_ref()
                .is_some_and(|p| p.method != "files.share")
        {
            return Err("Only an exact share can change access".into());
        }
        let p = s.pending.take().unwrap();
        if apply && p.method == "files.share" {
            let id = match field(&p.fields, "channel") {
                Some("D00000002") => "U00000002",
                Some("D00000003") => "W00000003",
                _ => return Err("Unknown share recipient".into()),
            };
            s.visible.insert(id.into());
        }
        let result = if lost_reply {
            Err(ListAssigneeFailure::Unconfirmed(Error::Transport(
                "fixture lost reply".into(),
            )))
        } else {
            response(s, shell, &p.method, &p.fields)
        };
        s.completed.push(json!({"serial":serial,"draft":p.draft,"job":p.job,"method":p.method,"fields":p.fields,"outcome":if lost_reply {"unconfirmed"} else {"reply"},"applied":apply}));
        Ok((p.reply, result))
    })?;
    tx.send(value)
        .map_err(|_| "Original worker is no longer awaiting its response".into())
}
pub(crate) use model::diagnostic;
/// Observe the existing cell worker's exact prepared request. No dispatch or
/// canonical mutation occurs here. Unarmed/static fixtures remain untouched.
pub(crate) fn cell_request(shell: &ShellState, attempt: &EditAttempt, request: &PreparedRequest) {
    if !enabled(shell) {
        return;
    }
    slot(|slot| {
        if let Some(s) = slot.as_mut().filter(|s| current(s, shell)) {
            if s.cells.len() >= 32 {
                s.failure = Some("Assignment fixture cell request budget exceeded".into());
                return;
            }
            let form = match &request.body {
                RequestBody::Form(f) => json!(f),
                _ => Value::Null,
            };
            s.cells.push(json!({"serial":attempt.serial,"visit":attempt.generation,"revision":attempt.revision,"selection":attempt.selection_generation,"method":request.method,"url":request.url,"fields":form,"retrySafe":request.retry_safe(),"cells":attempt.edit.cells()}));
        }
    });
}
pub(crate) fn complete_cell(shell: &mut ShellState, lost: bool) -> Result<(), String> {
    let before = slot(|slot| {
        let s = slot
            .as_ref()
            .filter(|s| current(s, shell))
            .ok_or("Stale fixture")?;
        if s.cells.len() != s.cell_completions.len() + 1 {
            return Err("No exact outstanding cell request");
        }
        Ok(s.cells.last().unwrap().clone())
    })?;
    let view = shell.slack_lists.view.as_ref().ok_or("Missing view")?;
    let mutation = shell
        .slack_lists
        .operations
        .get(&view.id)
        .ok_or("No actual cell mutation")?;
    if mutation.attempt.serial != before["serial"].as_u64().ok_or("Missing cell serial")?
        || mutation.phase != Phase::Saving
    {
        return Err("Cell request no longer saving".into());
    }
    if lost {
        crate::slack_lists::fixture::set_outcome_uncertain(shell);
    }
    crate::slack_lists::fixture::complete_pending(shell);
    slot(|slot| {
        let s = slot
            .as_mut()
            .filter(|s| current(s, shell))
            .ok_or("Stale fixture")?;
        s.cell_completions.push(json!({"serial":before["serial"],"lostReply":lost,"backend":crate::slack_lists::fixture::deletion_loaded(shell).ok_or("No backend")?.records.raw}));
        Ok(())
    }).map_err(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn request_owner_requires_same_account_epoch_list_and_scope_identity() {
        let mut shell = ShellState::fixture_variant(
            crate::media::MediaRegistry::default(),
            "slack-list-task-assignment",
        );
        let scope = shell.slack_lists.scope.clone().unwrap();
        let s = State {
            scope: scope.clone(),
            list: "F_LIST_FIXTURE".into(),
            requests: vec![],
            completed: vec![],
            cells: vec![],
            cell_completions: vec![],
            failure: None,
            visible: BTreeSet::new(),
            hold: BTreeSet::new(),
            pending: None,
        };
        assert!(same_owner(&s, &shell));
        let epoch = shell.core.account_epoch;
        shell.core.account_epoch += 1;
        assert!(!same_owner(&s, &shell));
        shell.core.account_epoch = epoch;
        shell.slack_lists.scope = Scope::capture(&shell);
        assert!(!same_owner(&s, &shell));
        shell.slack_lists.scope = Some(scope);
        shell.slack_lists.view.as_mut().unwrap().id = "F_REPLACEMENT".into();
        assert!(!same_owner(&s, &shell));
    }
}
