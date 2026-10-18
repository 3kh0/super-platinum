//! Exact disconnected two-effect observer. Existing shared worker owns dispatch.
use super::*;
use super_platinum_core::slack_lists::BoardGroupDelete;
pub(crate) fn check_enabled() -> bool {
    gate(
        std::env::var("SUPER_PLATINUM_LIST_BOARD_GROUP_DELETE_CHECK")
            .ok()
            .as_deref(),
        std::env::var("SUPER_PLATINUM_FIXTURE").ok().as_deref(),
    )
}
fn gate(flag: Option<&str>, alias: Option<&str>) -> bool {
    flag == Some("1")
        && matches!(
            alias,
            Some("slack-list-board-group-delete" | "slack-list-board-group-delete-narrow")
        )
}
pub(crate) fn seed(shell: &ShellState) {
    assert!(check_enabled() && enabled(shell));
    let loaded = shell
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .loaded
        .as_ref()
        .unwrap();
    *ledger().lock().unwrap() = Ledger {
        server: Some(loaded.document.raw.clone()),
        board_records: Some(loaded.records.raw.clone()),
        ..Default::default()
    };
}
fn intent(shell: &ShellState) -> Option<BoardGroupDelete> {
    match entry(shell)?.intent? {
        Intent::GroupDelete(i) => Some(i),
        _ => None,
    }
}
fn expected(i: &BoardGroupDelete, schema: bool) -> Value {
    if schema {
        let mut raw = i.original().raw.clone();
        let col = raw["list_metadata"]["schema"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|c| c["id"] == i.column_id())
            .unwrap();
        col["options"] = i.options().clone();
        raw
    } else {
        let mut raw = i.original_records().raw.clone();
        for cell in i.cells() {
            let row = raw["records"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .find(|r| r["id"] == cell["row_id"])
                .unwrap();
            let field = row["fields"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .find(|f| f["key"] == i.column_key())
                .unwrap();
            field["select"] = json!([]);
        }
        raw
    }
}
fn form(i: &BoardGroupDelete, schema: bool) -> Vec<(String, String)> {
    let mut f = vec![
        ("token".into(), "fixture-not-a-token".into()),
        ("list_id".into(), "F_LIST_FIXTURE".into()),
    ];
    if schema {
        f.extend([
            ("key".into(), "status".into()),
            ("options".into(), i.options().to_string()),
            ("backfill_missing_cells_with_default".into(), "true".into()),
            ("_x_reason".into(), "update-column".into()),
        ]);
    } else {
        f.extend([
            ("cells".into(), Value::Array(i.cells().to_vec()).to_string()),
            ("_x_reason".into(), "bulk-update-cells".into()),
        ]);
    }
    f
}
pub(super) async fn write(
    state: Signal<ShellState>,
    guard: &Guard,
    permit: Permit,
) -> Result<Value, ListColumnRenameFailure> {
    use ListColumnRenameFailure::{NotRequested, Unconfirmed};
    let schema = match &permit {
        Permit::GroupDeleteSchema(_) => true,
        Permit::GroupDeleteCells(_) => false,
        _ => return Err(NotRequested(Error::TransportNotConfigured)),
    };
    if !check_enabled()
        || !enabled(&state.peek())
        || !guard.current(&state.peek(), Stage::Saving)
        || registry::blocked(&guard.key.owner)
    {
        return Err(NotRequested(Error::TransportNotConfigured));
    }
    let i = intent(&state.peek()).ok_or(NotRequested(Error::TransportNotConfigured))?;
    if i.list_id() != "F_LIST_FIXTURE"
        || i.view_id() != "V_LANE"
        || i.column_id() != "C_SELECT"
        || i.column_key() != "status"
        || i.value() != "OPT_PROGRESS"
        || i.cells() != (0..9).map(|n|json!({"row_id":format!("R_LIST_{n:03}"),"column_id":"C_SELECT","select":[]})).collect::<Vec<_>>()
        || i.options() != &json!({"choices":[{"value":"OPT_DONE","label":"Done","color":"green"}]})
    {
        return Err(NotRequested(Error::Api(
            "fixture_group_delete_target".into(),
        )));
    }
    {
        let l = ledger().lock().unwrap();
        let count = if schema { 5 } else { 4 };
        if l.requests.len() != count
            || l.responses.len() != count
            || l.held_read.is_some()
            || l.server.as_ref() != Some(&i.original().raw)
            || l.board_records.as_ref() != Some(&i.original_records().raw)
            || (schema && (l.pending.is_some() || l.group_delete_cells_pending.is_none()))
            || (!schema && (l.pending.is_some() || l.group_delete_cells_pending.is_some()))
        {
            return Err(NotRequested(Error::Api(
                "fixture_group_delete_order".into(),
            )));
        }
    }
    let request = permit.request(&state.peek().core.client, &session());
    let path = if schema {
        "/api/lists.columns.update"
    } else {
        "/api/lists.cells.update"
    };
    if !matches!(&request.body,RequestBody::Form(f) if f==&form(&i,schema))
        || url::Url::parse(&request.url)
            .ok()
            .is_none_or(|u| u.path() != path)
    {
        return Err(NotRequested(Error::Api("fixture_group_delete_form".into())));
    }
    record(request).map_err(NotRequested)?;
    let (tx, rx) = tokio::sync::oneshot::channel();
    {
        let mut l = ledger().lock().unwrap();
        if schema {
            l.pending = Some(tx);
        } else {
            l.group_delete_cells_pending = Some(tx);
        }
        l.responses.push(
            json!({"syntheticFailure":"response_lost","effect":if schema{"schema"}else{"cells"}}),
        );
    }
    tokio::time::timeout(std::time::Duration::from_secs(30), rx)
        .await
        .map_err(|_| Unconfirmed(Error::Transport("fixture_timeout".into())))?
        .map_err(|_| Unconfirmed(Error::Transport("fixture_closed".into())))?;
    // The already accepted synthetic request may complete after its UI owner retires.
    // This only changes the disconnected server; the real completion reducer guards UI.
    let expected = expected(&i, schema);
    let mut l = ledger().lock().unwrap();
    if schema {
        l.server = Some(expected);
    } else {
        l.board_records = Some(expected);
    }
    Err(Unconfirmed(Error::Transport(
        "fixture_response_lost".into(),
    )))
}
pub(crate) fn release_effect(shell: &ShellState, schema: bool) -> Result<(), String> {
    if !check_enabled() || !enabled(shell) {
        return Err("Exact disconnected group deletion required".into());
    }
    let tx = {
        let mut l = ledger().lock().unwrap();
        if l.requests.len() != 6 || l.responses.len() != 6 || l.held_read.is_some() {
            return Err("Both source requests must be held before release".into());
        }
        if schema {
            l.pending.take()
        } else {
            l.group_delete_cells_pending.take()
        }
    }
    .ok_or("Missing held effect")?;
    tx.send(()).map_err(|_| "Held effect closed".into())
}
/// One positive current response observes schema but leaves the cells unconfirmed.
pub(crate) fn partial(shell: &ShellState) -> Result<(), String> {
    if !check_enabled() || !enabled(shell) {
        return Err("Exact disconnected group deletion required".into());
    }
    let e = entry(shell)
        .filter(|e| e.stage == Stage::Uncertain)
        .ok_or("No uncertain group deletion")?;
    let Some(Intent::GroupDelete(i)) = e.intent else {
        return Err("No group intent".into());
    };
    let mut l = ledger().lock().unwrap();
    if l.pending.is_some()
        || l.group_delete_cells_pending.is_some()
        || l.held_read.is_some()
        || l.card_records_once.is_some()
        || l.server.as_ref() != Some(&expected(&i, true))
        || l.board_records.as_ref() != Some(&expected(&i, false))
    {
        return Err("Effects have not completed independently".into());
    }
    l.card_records_once = Some(i.original_records().raw.clone());
    Ok(())
}
pub(crate) fn custody(shell: &ShellState) -> Result<Value, String> {
    if !check_enabled() || !enabled(shell) {
        return Err("Exact disconnected group deletion required".into());
    }
    let e = entry(shell);
    let l = ledger().lock().unwrap();
    Ok(
        json!({"account":shell.core.active_account,"epoch":shell.core.account_epoch,
        "serial":e.as_ref().map(|e|e.serial),"target":e.as_ref().and_then(|e|e.target.id()),
        "original":e.as_ref().map(|e|&e.original.raw),"population":e.as_ref().and_then(|e|e.original_records.as_ref().map(|p|&p.raw)),
        "intent":e.as_ref().and_then(|e|e.intent.as_ref()).map(|i|match i {Intent::GroupDelete(i)=>json!({"view":i.view_id(),"value":i.value(),"options":i.options(),"cells":i.cells()}),_=>Value::Null}),
        "schemaPending":l.pending.is_some(),"cellsPending":l.group_delete_cells_pending.is_some(),
        "readPending":l.held_read.is_some(),"requests":l.requests.len(),"responses":l.responses.len(),"completions":l.finished.len(),
        "schema":e.as_ref().and_then(|e|e.group_delete_outcomes).map(|o|format!("{:?}",o.schema)),
        "cells":e.as_ref().and_then(|e|e.group_delete_outcomes).map(|o|format!("{:?}",o.cells)),"serverPopulation":l.board_records}),
    )
}
/// Controlled reopen projection; does not fabricate a production fetch completion.
pub(crate) fn seed_reopen(
    shell: &mut ShellState,
    visit: u64,
    loaded: Loaded,
    find: crate::slack_lists::find::State,
) -> Result<(), String> {
    if !check_enabled() || !enabled(shell) || generation(shell) != Some(visit) {
        return Err("Reopened fixture ownership changed".into());
    }
    let e = entry(shell)
        .filter(|e| e.stage == Stage::Checking && e.target.is_group_delete() && e.job.is_some())
        .ok_or("Missing retained deletion check")?;
    let Some(Intent::GroupDelete(i)) = e.intent else {
        return Err("Missing retained deletion intent".into());
    };
    if ledger().lock().unwrap().held_read.is_none()
        || loaded.document.raw != expected(&i, true)
        || loaded.records != *i.original_records()
        || e.view_selection == selection(shell)
        || find.input != "Launch item 0"
        || find.applied != find.input
        || find.revision == 0
    {
        return Err("Wrong partial snapshot or replacement Find custody".into());
    }
    let v = shell.slack_lists.view.as_mut().ok_or("No reopened List")?;
    if v.id != loaded.document.id
        || v.loaded.is_some()
        || v.loading
        || v.unavailable
        || v.read != 0
        || v.revision != 0
        || !v.layout.find.input.is_empty()
        || !v.layout.find.applied.is_empty()
    {
        return Err("Replacement List already populated".into());
    }
    v.title = loaded.document.title.clone();
    v.loaded = Some(loaded);
    v.loading = false;
    v.stale = false;
    v.unavailable = false;
    v.revision = next_id();
    v.layout.find = find;
    rebuild_rows(shell);
    Ok(())
}
pub(crate) fn registry_empty(shell: &ShellState) -> Result<bool, String> {
    if !check_enabled() || !enabled(shell) {
        return Err("Exact disconnected group deletion required".into());
    }
    Ok(entry(shell).is_none())
}
pub(crate) fn initial_readiness(shell: &ShellState) -> Result<Value, String> {
    if !registry_empty(shell)? {
        return Err("Group deletion receipt exists before first action".into());
    }
    let v = shell.slack_lists.view.as_ref().ok_or("No initial List")?;
    let l = v.loaded.as_ref().ok_or("No initial population")?;
    if v.loading
        || v.stale
        || v.unavailable
        || selected_view(shell) != Some("V_LANE")
        || l.records.records.len() != 73
        || l.records.raw["response_metadata"]["next_cursor"] != "precision-opaque+/="
    {
        return Err("Initial offline Board is not ready".into());
    }
    Ok(
        json!({"provenance":"controlled-offline-seed","loading":false,"stale":false,"unavailable":false,
        "selected":"V_LANE","document":l.document.raw,"population":l.records.raw}),
    )
}
pub(crate) fn refuse_old_confirmation(
    shell: &mut ShellState,
    old_serial: u64,
) -> Result<Value, String> {
    if !check_enabled() || !enabled(shell) {
        return Err("Exact disconnected group deletion required".into());
    }
    let e = entry(shell)
        .filter(|e| {
            e.stage == Stage::Editing && e.target.is_group_delete() && e.serial != old_serial
        })
        .ok_or("No replaced group confirmation")?;
    if !ledger().lock().unwrap().requests.is_empty() {
        return Err("Stale confirmation must precede all reads and writes".into());
    }
    let before = custody(shell)?;
    let prepared = super::super::prepare(shell, old_serial).is_some();
    let closed = super::super::close_entry(shell, old_serial);
    if prepared
        || closed
        || custody(shell)? != before
        || entry(shell).is_none_or(|n| n.serial != e.serial || n.intent.is_some())
    {
        return Err("Old group confirmation changed current custody".into());
    }
    Ok(
        json!({"oldSerial":old_serial,"currentSerial":e.serial,"prepared":false,"closed":false,"writes":0}),
    )
}
pub(crate) fn retired_read_receipt(shell: &ShellState, held: &Value) -> Result<Value, String> {
    if !check_enabled() || !enabled(shell) {
        return Err("Exact disconnected group deletion required".into());
    }
    let now = snapshot(shell)?;
    if held["readPending"] != true
        || now["readPending"] != false
        || now["requests"] != held["requests"]
        || now["responses"] != held["responses"]
        || held["requests"].as_array().map(Vec::len) != Some(11)
        || held["responses"].as_array().map(Vec::len) != Some(11)
        || held["finished"].as_array().map(Vec::len) != Some(2)
        || now["finished"].as_array().map(Vec::len) != Some(3)
        || held["requests"][10]["path"] != "/api/files.info"
        || held["writePending"] != false
        || now["writePending"] != false
        || now["finished"][0] != held["finished"][0]
        || now["finished"][1] != held["finished"][1]
        || now["finished"][2]["serial"] != held["finished"][0]["serial"]
        || now["server"] != held["server"]
        || now["finished"][2]["kind"] != "check"
        || now["finished"][2]["applied"] != false
    {
        return Err("Retired check issued later reads or applied stale data".into());
    }
    Ok(
        json!({"firstPath":"/api/files.info","requestsBefore":11,"requestsAfter":11,"laterReads":0,"applied":false}),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn aliases_and_flag_are_closed() {
        for a in [
            "slack-list-board-group-delete",
            "slack-list-board-group-delete-narrow",
        ] {
            assert!(gate(Some("1"), Some(a)));
            assert!(!gate(Some("true"), Some(a)));
            assert!(!gate(None, Some(a)));
        }
        assert!(!gate(Some("1"), Some("slack-list-board-choice")));
    }
}
