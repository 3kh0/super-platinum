//! Closed ordinary own-comment delete receipts. Future deletion is published
//! explicitly; issued ACKs, prior Pages and held responses remain immutable.
use super::*;
use super_platinum_core::{
    slack::{api, transport::ListCommentDeleteFailure},
    slack_lists::comments::{OwnDeleteAccepted, OwnDeleteIntent},
};
mod guards;
mod policy;
pub(crate) mod restart;
#[cfg(test)]
mod tests;
pub(crate) use guards::permission;
pub(crate) use policy::policy;
pub(crate) const RECEIVED_TS: &str = own_edit::RECEIVED_TS;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Mode {
    #[default]
    Success,
    HeldSuccess,
    Rejected,
    Unconfirmed,
    Offline,
    RateLimited,
}
#[derive(Default)]
pub(super) struct State {
    seed: Value,
    mode: Mode,
    policy: Vec<Value>,
    prepared: Vec<Value>,
    deletes: Vec<Value>,
    records: Vec<Value>,
    responses: Vec<Value>,
    held: Option<oneshot::Sender<()>>,
    releases: Vec<Value>,
    future_deleted: Option<String>,
    published: Option<String>,
    permission_revoked: bool,
}
pub(crate) fn named() -> bool {
    cfg!(any(test, feature = "protected-native-fixture"))
        && compose::check_enabled()
        && matches!(
            std::env::var("SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_CASE").as_deref(),
            Ok("own-delete"
                | "own-delete-failure"
                | "own-delete-storage"
                | "own-delete-guards"
                | "own-delete-rate-limit"
                | "own-delete-restart-pending-exercise"
                | "own-delete-restart-pending-restore"
                | "own-delete-restart-unknown-exercise"
                | "own-delete-restart-unknown-restore")
        )
}
pub(crate) fn enabled(shell: &ShellState) -> bool {
    named() && super::enabled(shell) && completion::enabled(shell)
}
fn refused() -> Error {
    Error::Offline("Closed own-comment deletion unavailable".into())
}
fn model(shell: &ShellState) -> Result<Model, Error> {
    if !enabled(shell) {
        return Err(refused());
    }
    shell
        .slack_lists
        .comments
        .fixture
        .clone()
        .ok_or_else(refused)
}
fn session(shell: &ShellState) -> Result<WorkspaceSession, Error> {
    completion::handle(shell)?;
    shell
        .core
        .session
        .as_ref()
        .and_then(|s| s.workspaces.get("T1"))
        .cloned()
        .ok_or_else(refused)
}
pub(crate) fn seed(state: Signal<ShellState>) -> Result<Value, String> {
    model(&state.peek()).map_err(|e| e.to_string())?;
    // The accepted BootRead seed owns the original metadata and exact received
    // rich/context row. No credentials are changed after the List scope exists.
    let seed = own_edit::seed(state)?;
    let model = model(&state.peek()).map_err(|e| e.to_string())?;
    let mut l = model
        .0
        .lock()
        .map_err(|_| "Delete fixture ledger poisoned")?;
    if l.own_delete.is_some() {
        return Err("Delete fixture already seeded".into());
    }
    l.own_delete = Some(State {
        seed: seed.clone(),
        ..Default::default()
    });
    Ok(seed)
}
pub(crate) fn mode(shell: &ShellState, mode: Mode) -> Result<(), String> {
    let model = model(shell).map_err(|e| e.to_string())?;
    let mut l = model
        .0
        .lock()
        .map_err(|_| "Delete fixture ledger poisoned")?;
    let l = l.own_delete.as_mut().ok_or("Delete fixture not seeded")?;
    if l.held.is_some() {
        return Err("Release original held delete first".into());
    }
    l.mode = mode;
    Ok(())
}
pub(crate) fn release(shell: &ShellState) -> Result<Value, String> {
    let model = model(shell).map_err(|e| e.to_string())?;
    let mut l = model
        .0
        .lock()
        .map_err(|_| "Delete fixture ledger poisoned")?;
    let l = l.own_delete.as_mut().ok_or("Delete fixture not seeded")?;
    let tx = l.held.take().ok_or("No held original delete")?;
    let cancelled = tx.is_closed();
    let delivered = tx.send(()).is_ok();
    let raw = json!({"delivered":delivered,"cancelledBeforeReply":cancelled});
    l.releases.push(raw.clone());
    Ok(raw)
}
pub(crate) fn publish(shell: &ShellState) -> Result<(), String> {
    let model = model(shell).map_err(|e| e.to_string())?;
    let mut l = model
        .0
        .lock()
        .map_err(|_| "Delete fixture ledger poisoned")?;
    let l = l.own_delete.as_mut().ok_or("Delete fixture not seeded")?;
    l.published = Some(
        l.future_deleted
            .clone()
            .ok_or("No independently prepared deletion readback")?,
    );
    Ok(())
}
pub(super) fn decorate(state: &Option<State>, root: &str, raw: &mut Value) {
    let Some(ts) = state.as_ref().and_then(|s| s.published.as_ref()) else {
        return;
    };
    if root != ROOT || raw["ok"] != true {
        return;
    }
    if let Some(rows) = raw["messages"].as_array_mut() {
        rows.retain(|r| r["ts"].as_str() != Some(ts.as_str()));
    }
    let mut deleted = raw["deleted"].as_array().cloned().unwrap_or_default();
    if !deleted.iter().any(|v| v == ts) {
        deleted.push(json!(ts));
    }
    raw["deleted"] = json!(deleted);
}
pub(crate) fn snapshot(shell: &ShellState) -> Value {
    let Ok(model) = model(shell) else {
        return json!({"enabled":false});
    };
    let l = model.0.lock().unwrap();
    let Some(l) = &l.own_delete else {
        return json!({"enabled":false});
    };
    json!({"enabled":true,"seed":l.seed,"received":l.seed["received"],"policy":l.policy,"prepared":l.prepared,
        "deletes":l.deletes,"records":l.records,"responses":l.responses,"held":l.held.is_some(),"releases":l.releases,
        "futureDeleted":l.future_deleted,"published":l.published.is_some(),"deleted":l.published.as_ref().map(|v|vec![v]).unwrap_or_default(),
        "networkCalls":0,"httpCapability":false})
}
fn source_matches(l: &Ledger, intent: &OwnDeleteIntent) -> bool {
    let r = intent.record();
    r.workspace() == "T1"
        && r.user() == completion::SELF
        && r.ts() == RECEIVED_TS
        && r.target().channel_id() == "CCOMMENT01"
        && r.target().list_id() == LIST
        && r.target().record_id() == "R_LIST_000"
        && r.target().root_ts() == Some(ROOT)
        && r.target().document_raw() == &l.document
        && l.records
            .get("R_LIST_000")
            .is_some_and(|v| v == r.target().record_raw())
}
pub(crate) async fn delete(
    state: Signal<ShellState>,
    request: &PreparedRequest,
    intent: OwnDeleteIntent,
) -> Result<OwnDeleteAccepted, ListCommentDeleteFailure> {
    use ListCommentDeleteFailure as Failure;
    let model = {
        let shell = state.peek();
        let owner = session(&shell).map_err(Failure::NotRequested)?;
        let expected = api::list_record_comment_delete(&shell.core.client, &owner, &intent)
            .map_err(Failure::NotRequested)?;
        if !own_edit::same(request, &expected) {
            return Err(Failure::NotRequested(refused()));
        }
        model(&shell).map_err(Failure::NotRequested)?
    };
    let (mode, raw, wait) = {
        let mut l = model
            .0
            .lock()
            .map_err(|_| Failure::NotRequested(refused()))?;
        if !source_matches(&l, &intent) {
            return Err(Failure::NotRequested(refused()));
        }
        let l = l
            .own_delete
            .as_mut()
            .ok_or_else(|| Failure::NotRequested(refused()))?;
        let form = own_edit::wire(request).map_err(Failure::NotRequested)?;
        l.prepared.push(form.clone());
        l.records.push(
            serde_json::to_value(intent.record()).map_err(|_| Failure::NotRequested(refused()))?,
        );
        l.future_deleted = None;
        let mode = std::mem::take(&mut l.mode);
        if mode == Mode::Offline {
            l.responses
                .push(json!({"disposition":"not-requested","raw":null,"retryAfter":null}));
            return Err(Failure::NotRequested(Error::Offline(
                "Closed delete not dispatched".into(),
            )));
        }
        l.deletes.push(form);
        if mode == Mode::Rejected {
            let raw = json!({"ok":false,"error":"cant_delete_message"});
            l.responses
                .push(json!({"disposition":"rejected","raw":raw,"retryAfter":null}));
            return Err(Failure::Rejected(raw));
        }
        if mode == Mode::RateLimited {
            let retry_after = guards::retry_after(
                &std::env::var("SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_CASE").unwrap_or_default(),
            );
            l.responses
                .push(json!({"disposition":"unconfirmed-rate-limited","raw":null,"retryAfter":retry_after}));
            return Err(Failure::Unconfirmed {
                error: Error::RateLimited {
                    retry_after_secs: Some(retry_after),
                },
                receipt: None,
            });
        }
        let raw = json!({"ok":true,"channel":intent.record().target().channel_id(),"ts":intent.record().ts(),"delete_fixture_opaque":{"original":true}});
        l.future_deleted = Some(intent.record().ts().into());
        l.responses.push(json!({"disposition":if mode==Mode::Unconfirmed {"unconfirmed"}else{"accepted"},"raw":if mode==Mode::Unconfirmed {Value::Null}else{raw.clone()},"retryAfter":null}));
        let wait = if mode == Mode::HeldSuccess {
            let (tx, rx) = oneshot::channel();
            l.held = Some(tx);
            Some(rx)
        } else {
            None
        };
        (mode, raw, wait)
    };
    if let Some(wait) = wait {
        wait.await.map_err(|_| Failure::Unconfirmed {
            error: refused(),
            receipt: None,
        })?;
    }
    if mode == Mode::Unconfirmed {
        return Err(Failure::Unconfirmed {
            error: Error::Transport("Closed delete acknowledgement lost".into()),
            receipt: None,
        });
    }
    intent
        .accept(raw.clone())
        .map_err(|_| Failure::Unconfirmed {
            error: refused(),
            receipt: Some(raw),
        })
}
