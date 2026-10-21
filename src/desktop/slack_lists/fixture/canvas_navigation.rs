//! Closed raw reads behind the actual List opener. No target View is installed here.
use super::*;
use std::sync::{Mutex, OnceLock};
use tokio::sync::oneshot;

const ACCOUNT: &str = "147b5480-9f1a-4443-89da-836472cbb209";
const TARGET: &str = "FCANON003";
const ROW: &str = "Native canonical List destination row";

pub(crate) fn requested() -> bool {
    cfg!(any(test, feature = "protected-native-fixture"))
        && std::env::var("SUPER_PLATINUM_CANVAS_CANONICAL_FILE_CHECK").as_deref() == Ok("1")
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("canvas-canonical-native-list" | "canvas-canonical-native-list-narrow")
        )
}
fn owner_matches(shell: &ShellState) -> bool {
    shell.core.session.is_none()
        && shell.core.transport.is_none()
        && shell.core.active_account.as_deref() == Some(ACCOUNT)
        && shell.core.active_team.as_deref() == Some("T1")
        && shell.self_account.user_id == "U0"
        && shell
            .core
            .workspaces
            .get("T1")
            .is_some_and(|w| w.team_id == "T1" && w.self_user_id == "U0")
}
pub(crate) fn enabled(shell: &ShellState) -> bool {
    requested() && owner_matches(shell)
}
fn refused() -> Error {
    Error::Offline("Closed Canvas destination List reader unavailable".into())
}

pub(crate) fn file() -> Value {
    json!({"id":TARGET,"mode":"list","filetype":"list",
        "name":"Canonical List destination","title":"Fresh canonical List",
        "permalink":"https://fixture.slack.com/files/FCANON003",
        "access":"read","is_locked":true,
        "list_metadata":{"schema":[
            {"id":"C_CANON_NAME","key":"NAME","name":"Item","type":"text","is_primary_column":true}
        ],"views":[]}})
}
fn responses() -> Vec<Value> {
    vec![
        json!({"ok":true,"file":file()}),
        json!({"ok":true,"list_id":TARGET,"records":[
            {"id":"R_CANON_DESTINATION","list_id":TARGET,"position":0,
                "fields":[{"key":"NAME","text":ROW}],
                "canonical_navigation_opaque":{"keep":[0,false,null]}}
        ]}),
        json!({"ok":true,"prefs":{"allow_lists":"full"}}),
        json!({"ok":true,"prefs":{"task_lists":"[]","assigned_to_list":""}}),
        json!({"ok":true,"assignments":{}}),
    ]
}
fn parse(raw: &[Value]) -> Result<Loaded, Error> {
    if raw.len() != 5 || raw[0]["ok"] != true {
        return Err(refused());
    }
    let document = Document::parse(raw[0]["file"].clone()).map_err(|_| refused())?;
    if document.id != TARGET {
        return Err(refused());
    }
    Ok(Loaded {
        document,
        records: api::parse_list_records(raw[1].clone(), TARGET)?,
        allow_lists: api::parse_list_allow_lists(&raw[2])?,
        personal_task_lists: Some(api::parse_list_personal_task_lists(&raw[3])?),
        list_experiments: Some(api::parse_list_experiments(&raw[4])?),
    })
}
fn requests(shell: &ShellState) -> Result<Vec<Value>, Error> {
    // Only request construction uses this synthetic session. Never installed.
    let session = WorkspaceSession {
        team_id: "T1".into(),
        user_id: "U0".into(),
        enterprise_id: None,
        name: "Closed Canvas List destination".into(),
        url: "https://fixture.slack.com".into(),
        token: "fixture-only".into(),
    };
    let c = &shell.core.client;
    [
        api::file_info(c, &session, TARGET),
        api::lists_records_list(c, &session, TARGET),
        api::list_allow_lists(c, &session),
        api::list_personal_task_preferences(c, &session),
        api::list_experiments(c, &session),
    ]
    .into_iter()
    .map(|request| {
        let RequestBody::Form(fields) = request.body else {
            return Err(refused());
        };
        // Retain original generated URL and ordered safe form, not rebuilt wire.
        Ok(json!({"method":request.method,"url":request.url,
            "fields":fields.into_iter().filter(|(key,_)|key!="token").collect::<Vec<_>>()}))
    })
    .collect()
}

#[derive(Default)]
struct Ledger {
    hold_next: bool,
    held: Option<(usize, oneshot::Sender<()>)>,
    requests: Vec<Value>,
    responses: Vec<Value>,
    settlements: Vec<Value>,
    releases: Vec<Value>,
}
static LEDGER: OnceLock<Mutex<Ledger>> = OnceLock::new();
pub(crate) fn snapshot() -> Value {
    let l = LEDGER.get_or_init(Default::default).lock().unwrap();
    json!({"requests":l.requests,"responses":l.responses,
        "settlements":l.settlements,"releases":l.releases,
        "held":l.held.as_ref().map(|(serial,_)|serial),
        "networkCalls":0,"httpCapability":false})
}
pub(crate) fn hold_next(shell: &ShellState) -> Result<(), Error> {
    if !enabled(shell) {
        return Err(refused());
    }
    let mut l = LEDGER.get_or_init(Default::default).lock().unwrap();
    if l.held.is_some() || l.hold_next {
        return Err(refused());
    }
    l.hold_next = true;
    Ok(())
}
pub(crate) fn release(shell: &ShellState) -> Result<Value, Error> {
    if !enabled(shell) {
        return Err(refused());
    }
    let mut l = LEDGER.get_or_init(Default::default).lock().unwrap();
    let (serial, sender) = l.held.take().ok_or_else(refused)?;
    let witness = json!({"serial":serial,"delivered":sender.send(()).is_ok()});
    l.releases.push(witness.clone());
    Ok(witness)
}

/// True consumes only this named target refresh; other readers remain unchanged.
pub(in crate::slack_lists) fn refresh(mut state: Signal<ShellState>) -> bool {
    if !requested()
        || state
            .peek()
            .slack_lists
            .view
            .as_ref()
            .is_none_or(|v| v.id != TARGET)
    {
        return false;
    }
    let Some(attempt) = begin_read(&mut state.write()) else {
        return true;
    };
    let wire = {
        let shell = state.peek();
        if enabled(&shell) && attempt.current(&shell) {
            requests(&shell)
        } else {
            Err(refused())
        }
    };
    let Ok(wire) = wire else {
        finish_read(&mut state.write(), &attempt, Err(refused()));
        return true;
    };
    let raw = responses();
    let (serial, held) = {
        let mut l = LEDGER.get_or_init(Default::default).lock().unwrap();
        let serial = l.requests.len() + 1;
        l.requests.push(json!({"serial":serial,"id":attempt.id,
            "generation":attempt.generation,"read":attempt.read,"revision":attempt.revision,
            "requests":wire}));
        l.responses.push(json!({"serial":serial,"responses":raw}));
        let held = if std::mem::take(&mut l.hold_next) {
            let (sender, receiver) = oneshot::channel();
            l.held = Some((serial, sender));
            Some(receiver)
        } else {
            None
        };
        (serial, held)
    };
    dioxus::core::spawn_forever(async move {
        let delivered = match held {
            Some(receiver) => tokio::time::timeout(std::time::Duration::from_secs(45), receiver)
                .await
                .is_ok_and(|result| result.is_ok()),
            None => true,
        };
        let active = enabled(&state.peek()) && attempt.current(&state.peek());
        let result = if active && delivered {
            parse(&raw)
        } else {
            Err(refused())
        };
        let parsed = result.is_ok();
        let refresh_again = finish_read(&mut state.write(), &attempt, result);
        LEDGER
            .get_or_init(Default::default)
            .lock()
            .unwrap()
            .settlements
            .push(
                json!({"serial":serial,"delivered":delivered,"current":active,"parsed":parsed,
                "refreshAgain":refresh_again}),
            );
        if refresh_again {
            super::super::network::refresh(state);
        }
    });
    true
}

#[cfg(test)]
mod tests;
