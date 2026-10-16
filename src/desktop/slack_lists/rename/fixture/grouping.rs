//! Exact owned Group by adapter: real permits/forms and existing retained reducers.
use super::*;
use super_platinum_core::slack_lists::ViewGroupingChoice;

pub(crate) fn check_enabled() -> bool {
    gate(
        std::env::var("SUPER_PLATINUM_LIST_VIEW_GROUPING_CHECK")
            .ok()
            .as_deref(),
        std::env::var("SUPER_PLATINUM_FIXTURE").ok().as_deref(),
    )
}
fn gate(flag: Option<&str>, alias: Option<&str>) -> bool {
    flag == Some("1")
        && matches!(
            alias,
            Some(
                "slack-list-view-grouping"
                    | "slack-list-view-grouping-narrow"
                    | "slack-list-board-grouping"
                    | "slack-list-board-grouping-narrow"
            )
        )
}
fn require(shell: &ShellState) -> Result<(), String> {
    if check_enabled() && enabled(shell) {
        Ok(())
    } else {
        Err("Exact disconnected Group by checker required".into())
    }
}
pub(crate) fn seed(shell: &ShellState, server: Value) {
    require(shell).unwrap();
    *ledger().lock().unwrap() = Ledger {
        server: Some(server),
        ..Default::default()
    };
}
pub(crate) fn registry_empty(shell: &ShellState) -> Result<bool, String> {
    require(shell)?;
    Ok(entry(shell).is_none() && super::super::presentation(shell).is_none())
}
pub(crate) fn custody(shell: &ShellState) -> Result<Value, String> {
    require(shell)?;
    let e = entry(shell).ok_or("Missing Group by custody")?;
    Ok(
        json!({"account":shell.core.active_account,"epoch":shell.core.account_epoch,"serial":e.serial,"choice":e.grouping_choice,"original":e.original.raw,"intent":e.intent.as_ref().map(|i|match i {Intent::Grouping(i)=>json!({"original":i.original().raw,"grouping":i.grouping()}),_=>Value::Null})}),
    )
}
pub(crate) fn refuse_old_choice(
    shell: &ShellState,
    serial: u64,
    visit: u64,
    old: u64,
) -> Result<Value, String> {
    require(shell)?;
    let before = entry(shell).ok_or("Missing choice")?;
    let rendered = match &before.input {
        Input::Grouping(c) => c.clone(),
        _ => return Err("Wrong choice kind".into()),
    };
    let change = super::super::grouping::change(
        shell,
        serial,
        visit,
        old,
        &rendered,
        ViewGroupingChoice::None,
    );
    let confirm = super::super::grouping::confirm_current(shell, serial, visit, old);
    let after = entry(shell).ok_or("Lost choice")?;
    if change
        || confirm
        || before.input != after.input
        || before.grouping_choice != after.grouping_choice
    {
        return Err("Old grouping callback changed authority".into());
    }
    Ok(
        json!({"oldChoice":old,"currentChoice":after.grouping_choice,"change":change,"confirm":confirm}),
    )
}
pub(crate) fn conflict_file(shell: &ShellState) -> Result<(), String> {
    require(shell)?;
    let e = entry(shell)
        .filter(|e| {
            e.stage == Stage::Uncertain
                && e.target == Target::Grouping("V_GROUP".into())
                && e.intent.is_some()
        })
        .ok_or("No uncertain grouping")?;
    let mut raw = e.original.raw;
    set_grouping(&mut raw, json!({"group_by":"other"})).map_err(|_| "Invalid conflict")?;
    ledger().lock().unwrap().queue_precision_file(raw)
}
pub(crate) fn seed_reopen(
    shell: &mut ShellState,
    visit: u64,
    loaded: crate::slack_lists::Loaded,
    find: crate::slack_lists::find::State,
) -> Result<(), String> {
    require(shell)?;
    if ledger().lock().unwrap().held_read.is_none() {
        return Err("No held Group by read".into());
    }
    seed_reopened(shell, visit, loaded, find)
}
fn seed_reopened(
    shell: &mut ShellState,
    visit: u64,
    loaded: crate::slack_lists::Loaded,
    find: crate::slack_lists::find::State,
) -> Result<(), String> {
    let e = entry(shell)
        .filter(|e| {
            e.stage == Stage::Checking
                && e.target == Target::Grouping("V_GROUP".into())
                && e.intent.is_some()
                && e.job.is_some()
        })
        .ok_or("Missing retained grouping check")?;
    let mut conflict = e.original.raw.clone();
    set_grouping(&mut conflict, json!({"group_by":"other"})).map_err(|_| "Invalid conflict")?;
    if generation(shell) != Some(visit)
        || loaded.document.raw != conflict
        || e.view_selection == selection(shell)
        || find.input != "Launch item 0"
        || find.applied != find.input
        || find.revision == 0
    {
        return Err("Wrong accepted conflict snapshot/Find/replacement".into());
    }
    let v = shell
        .slack_lists
        .view
        .as_mut()
        .ok_or("Missing replacement")?;
    if v.id != loaded.document.id
        || v.loaded.is_some()
        || v.loading
        || v.unavailable
        || v.read != 0
        || v.revision != 0
        || !v.layout.find.input.is_empty()
    {
        return Err("Replacement already populated".into());
    }
    v.title = loaded.document.title.clone();
    v.loaded = Some(loaded);
    v.stale = false;
    v.revision = next_id();
    v.layout.find = find;
    crate::slack_lists::rebuild_rows(shell);
    Ok(())
}
fn vector(index: usize) -> Option<Value> {
    match index {
        0 => Some(json!({"group_by":"status"})),
        1 => Some(json!({"group_by":"done"})),
        2 => Some(json!({"group_by":""})),
        _ => None,
    }
}
fn set_grouping(server: &mut Value, grouping: Value) -> Result<(), Error> {
    let view = server["list_metadata"]["views"]
        .as_array_mut()
        .and_then(|v| v.iter_mut().find(|v| v["id"] == "V_GROUP"))
        .ok_or_else(|| Error::Api("fixture_grouping_view".into()))?;
    view["grouping"] = grouping;
    Ok(())
}
fn reply(server: &Value) -> Value {
    json!({"ok":true,"list":{"id":server["id"],"list_metadata":server["list_metadata"]},"opaque_reply":{"grouping":true}})
}
pub(super) async fn write(
    state: Signal<ShellState>,
    guard: &Guard,
    permit: Permit,
) -> Result<Value, ListColumnRenameFailure> {
    if !check_enabled()
        || !enabled(&state.peek())
        || !guard.current(&state.peek(), Stage::Saving)
        || registry::blocked(&guard.key.owner)
        || !permit.is_grouping()
    {
        return Err(ListColumnRenameFailure::NotRequested(
            Error::TransportNotConfigured,
        ));
    }
    let desired = with_registry(|r| {
        r.entries.get(&guard.key).and_then(|e| match &e.intent {
            Some(Intent::Grouping(i)) if e.target == Target::Grouping("V_GROUP".into()) => {
                Some(i.grouping().clone())
            }
            _ => None,
        })
    })
    .ok_or(ListColumnRenameFailure::NotRequested(
        Error::TransportNotConfigured,
    ))?;
    let index = {
        let l = ledger().lock().unwrap();
        if l.pending.is_some() || l.held_read.is_some() {
            return Err(ListColumnRenameFailure::NotRequested(Error::Api(
                "fixture_grouping_pending".into(),
            )));
        }
        l.requests
            .iter()
            .filter(|r| r["path"] == "/api/lists.views.update")
            .count()
    };
    if vector(index).as_ref() != Some(&desired) {
        return Err(ListColumnRenameFailure::NotRequested(Error::Api(
            "fixture_grouping_order".into(),
        )));
    }
    let request = permit.request(&state.peek().core.client, &session());
    let expected = vec![
        ("token".into(), "fixture-not-a-token".into()),
        ("list_id".into(), "F_LIST_FIXTURE".into()),
        ("id".into(), "V_GROUP".into()),
        ("grouping".into(), desired.to_string()),
        ("_x_reason".into(), "update-view".into()),
    ];
    if !matches!(&request.body,RequestBody::Form(fields) if fields==&expected) {
        return Err(ListColumnRenameFailure::NotRequested(Error::Api(
            "fixture_grouping_form".into(),
        )));
    }
    if record(request).map_err(ListColumnRenameFailure::NotRequested)? != "/api/lists.views.update"
    {
        return Err(ListColumnRenameFailure::NotRequested(Error::Api(
            "fixture_grouping_path".into(),
        )));
    }
    let (tx, rx) = tokio::sync::oneshot::channel();
    {
        let mut l = ledger().lock().unwrap();
        let response = if index == 0 {
            json!({"syntheticFailure":"response_lost"})
        } else {
            let mut server = l.server.clone().unwrap();
            set_grouping(&mut server, desired.clone())
                .map_err(ListColumnRenameFailure::NotRequested)?;
            reply(&server)
        };
        l.responses.push(response);
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
    let mut l = ledger().lock().unwrap();
    set_grouping(l.server.as_mut().unwrap(), desired)
        .map_err(ListColumnRenameFailure::Unconfirmed)?;
    if index == 0 {
        Err(ListColumnRenameFailure::Unconfirmed(Error::Transport(
            "fixture_response_lost".into(),
        )))
    } else {
        Ok(l.responses.last().unwrap().clone())
    }
}

/// Test teardown uses the real Cancel boundary; never drops an admitted operation.
#[cfg(test)]
pub(crate) fn close_static_for_test(shell: &ShellState) -> bool {
    let Some(e) = entry(shell).filter(|e| {
        e.target == Target::Grouping("V_GROUP".into())
            && e.stage == Stage::Editing
            && e.job.is_none()
            && e.intent.is_none()
    }) else {
        return false;
    };
    super::super::close_entry(shell, e.serial)
}

#[cfg(test)]
mod tests;
