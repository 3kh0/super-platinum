//! Bounded observations of the existing cell worker. No transport or second executor.
use super::super::*;
use crate::slack_lists::rename;
use super_platinum_core::slack::client::PreparedRequest;
pub(crate) const ROW: &str = "R_LIST_000";
#[derive(Clone)]
pub(crate) struct State {
    #[cfg(test)]
    test_only: bool,
    scope: Scope,
    visit: u64,
    selection: u64,
    checkbox: bool,
    document: Value,
    pages: [Value; 3],
    preflights: Vec<Value>,
    requests: Vec<Value>,
    completed: Vec<Value>,
    reads: Vec<Value>,
    baseline_ticket: Option<ReadAttempt>,
    duplicate: Option<Value>,
    queued: bool,
    error: Option<String>,
}
pub(crate) fn check_enabled() -> bool {
    gate(
        std::env::var("SUPER_PLATINUM_LIST_BOARD_MOVE_CHECK")
            .ok()
            .as_deref(),
        std::env::var("SUPER_PLATINUM_FIXTURE").ok().as_deref(),
    )
}
fn gate(flag: Option<&str>, fixture: Option<&str>) -> bool {
    flag == Some("1")
        && matches!(
            fixture,
            Some(
                "slack-list-board-move"
                    | "slack-list-board-move-narrow"
                    | "slack-list-board-move-checkbox"
                    | "slack-list-board-move-checkbox-narrow"
            )
        )
}

fn state(shell: &ShellState) -> Result<&State, String> {
    let s = shell
        .slack_lists
        .fixture
        .as_ref()
        .and_then(|f| f.board_move_check.as_ref())
        .ok_or("Board Move check not armed")?;
    let view = shell
        .slack_lists
        .view
        .as_ref()
        .ok_or("Missing Board visit")?;
    let allowed = check_enabled();
    #[cfg(test)]
    let allowed = allowed || s.test_only;
    if !allowed
        || !super::super::enabled(shell)
        || !s.scope.current(shell)
        || shell
            .slack_lists
            .scope
            .as_ref()
            .is_none_or(|p| !std::sync::Arc::ptr_eq(&p.identity, &s.scope.identity))
        || generation(shell) != Some(s.visit)
        || rename::selection(shell) != Some(s.selection)
        || view.layout.selected.as_deref() != Some("V_MOVE")
        || rename::presentation(shell).is_some()
        || shell.slack_lists.fixture.as_ref().unwrap().document != s.document
    {
        return Err("Board Move scope/document retired".into());
    }
    if let Some(error) = &s.error {
        return Err(error.clone());
    }
    Ok(s)
}
fn state_mut(shell: &mut ShellState) -> &mut State {
    shell
        .slack_lists
        .fixture
        .as_mut()
        .unwrap()
        .board_move_check
        .as_mut()
        .unwrap()
}
pub(crate) fn armed(shell: &ShellState) -> bool {
    shell
        .slack_lists
        .fixture
        .as_ref()
        .is_some_and(|f| f.board_move_check.is_some())
}
pub(crate) fn fail(shell: &mut ShellState, error: String) {
    if armed(shell) {
        state_mut(shell).error.get_or_insert(error);
    }
}
pub(crate) fn pages(original: &Value, checkbox: bool) -> Result<[Value; 3], String> {
    let records = original["records"]
        .as_array()
        .ok_or("Missing raw records")?;
    if records.len() != 73 || records.iter().filter(|r| r["id"] == ROW).count() != 1 {
        return Err("Wrong original population".into());
    }
    let fields = records.iter().find(|r| r["id"] == ROW).unwrap()["fields"]
        .as_array()
        .ok_or("Malformed raw fields")?;
    let key = if checkbox { "done" } else { "status" };
    let existing = fields
        .iter()
        .filter(|f| f["key"] == key)
        .collect::<Vec<_>>();
    if checkbox && !existing.is_empty()
        || !checkbox && (existing.len() != 1 || existing[0]["select"] != json!(["OPT_PROGRESS"]))
    {
        return Err("Wrong original grouping value".into());
    }
    let changed = |second: bool| {
        let mut page = original.clone();
        let row = page["records"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|r| r["id"] == ROW)
            .unwrap();
        let fields = row["fields"].as_array_mut().unwrap();
        if checkbox {
            fields.push(json!({"key":"done","checkbox":!second}));
        } else {
            fields.iter_mut().find(|f| f["key"] == "status").unwrap()["select"] = if second {
                json!(["OPT_DONE"])
            } else {
                json!([])
            };
        }
        page
    };
    Ok([original.clone(), changed(false), changed(true)])
}
pub(crate) fn arm(shell: &mut ShellState) -> Result<Value, String> {
    if !check_enabled() {
        return Err("Board Move opt-in required".into());
    }
    let checkbox = std::env::var("SUPER_PLATINUM_FIXTURE")
        .unwrap_or_default()
        .contains("checkbox");
    arm_owned(shell, checkbox)?;
    snapshot(shell)
}
fn arm_owned(shell: &mut ShellState, checkbox: bool) -> Result<(), String> {
    if !super::super::enabled(shell)
        || armed(shell)
        || !shell.slack_lists.operations.is_empty()
        || rename::presentation(shell).is_some()
    {
        return Err("Board Move arm refused".into());
    }
    let f = shell.slack_lists.fixture.as_ref().ok_or("No fixture")?;
    let v = shell.slack_lists.view.as_ref().ok_or("No visit")?;
    let l = v.loaded.as_ref().ok_or("No loaded data")?;
    if v.editor.is_some()
        || v.loading
        || v.layout.find.input != "Launch"
        || v.layout.find.applied != "Launch"
        || f.attempts != 0
        || f.pending.is_some()
        || f.error
        || f.document != l.document.raw
        || f.records != l.records.raw
    {
        return Err("Arm requires settled Board/Find and zero writes".into());
    }
    let s = State {
        #[cfg(test)]
        test_only: false,
        scope: shell.slack_lists.scope.clone().ok_or("No scope")?,
        visit: v.generation,
        selection: rename::selection(shell).ok_or("No selection")?,
        checkbox,
        document: f.document.clone(),
        pages: pages(&f.records, checkbox)?,
        preflights: vec![],
        requests: vec![],
        completed: vec![],
        reads: vec![],
        baseline_ticket: None,
        duplicate: None,
        queued: false,
        error: None,
    };
    shell.slack_lists.fixture.as_mut().unwrap().board_move_check = Some(s);
    Ok(())
}
fn attempt_json(a: &EditAttempt) -> Value {
    json!({"serial":a.serial,"visit":a.generation,"selection":a.selection_generation,"revision":a.revision,"owner":a.scope.stable_owner(),"list":a.edit.list_id(),"row":a.edit.record_id(),"column":a.edit.column_id(),"key":a.edit.column_key(),"cells":a.edit.cells()})
}
pub(crate) fn operation(shell: &ShellState) -> Result<Value, String> {
    let s = state(shell)?;
    if shell.slack_lists.operations.len() > 1 {
        return Err("Extra operation".into());
    }
    let Some(m) = shell.slack_lists.operations.get("F_LIST_FIXTURE") else {
        return Ok(Value::Null);
    };
    if !m.attempt.scope.stable_current(shell)
        || m.attempt.scope.stable_owner() != s.scope.stable_owner()
        || m.attempt.generation != s.visit
        || m.attempt.selection_generation != s.selection
    {
        return Err("Mutation owner changed".into());
    }
    Ok(
        json!({"attempt":attempt_json(&m.attempt),"phase":match m.phase{Phase::Preparing=>"Preparing",Phase::Saving=>"Saving",Phase::Uncertain=>"Uncertain"}}),
    )
}
pub(crate) fn snapshot(shell: &ShellState) -> Result<Value, String> {
    let s = state(shell)?;
    let f = shell.slack_lists.fixture.as_ref().unwrap();
    let v = shell.slack_lists.view.as_ref().unwrap();
    let l = v.loaded.as_ref().ok_or("No loaded page")?;
    Ok(
        json!({"checkbox":s.checkbox,"owner":s.scope.stable_owner(),"visit":s.visit,"selection":s.selection,"document":s.document,"pages":s.pages,"loaded":l.records.raw,"loadedDocument":l.document.raw,"server":f.records,"find":{"input":v.layout.find.input,"applied":v.layout.find.applied,"revision":v.layout.find.revision},"preflights":s.preflights,"requests":s.requests,"completed":s.completed,"reads":s.reads,"duplicate":s.duplicate,"missingQueued":s.queued,"pending":f.pending.as_ref().map(attempt_json),"attemptCount":f.attempts,"preparedValid":f.prepared_valid,"operation":operation(shell)?,"editor":crate::slack_lists::editor::presentation(shell).map(|e|e.generation),"read":v.read,"loading":v.loading,"revision":v.revision,"layout":v.layout.generation,"expanded":v.expanded,"collapsed":v.layout.collapsed}),
    )
}
pub(crate) fn preflight(
    shell: &mut ShellState,
    a: &EditAttempt,
    canonical: &Loaded,
) -> Result<(), String> {
    if !armed(shell) {
        return Ok(());
    }
    let s = state(shell)?;
    let i = s.preflights.len();
    if i > 1
        || s.requests.len() != i
        || s.completed.len() != i
        || canonical.document.raw != s.document
        || canonical.records.raw != s.pages[i]
        || !a.scope.current(shell)
        || a.generation != s.visit
        || a.selection_generation != s.selection
        || !matches!(a.authority, EditAuthority::BoardMove(_))
    {
        return Err("Unexpected Board Move preflight".into());
    }
    let desired = if s.checkbox {
        CellEditValue::Boolean(i == 0)
    } else {
        CellEditValue::Select(if i == 0 {
            vec![]
        } else {
            vec!["OPT_DONE".into()]
        })
    };
    let expected = crate::slack_lists::board_move::capture(
        canonical,
        ROW,
        if s.checkbox { "C_CHECK" } else { "C_SELECT" },
        desired,
    )?;
    if a.edit != expected || operation(shell)?["phase"] != "Saving" {
        return Err("Wrong actual typed Board Move admission".into());
    }
    let event = json!({"attempt":attempt_json(a),"document":canonical.document.raw,"population":canonical.records.raw,"allow":canonical.allow_lists,"phase":"Saving"});
    state_mut(shell).preflights.push(event);
    Ok(())
}
pub(crate) fn request(
    shell: &mut ShellState,
    a: &EditAttempt,
    r: &PreparedRequest,
) -> Result<(), String> {
    if !armed(shell) {
        return Ok(());
    }
    let s = state(shell)?;
    let i = s.requests.len();
    if i >= 2 || s.preflights.len() != i + 1 || s.preflights[i]["attempt"] != attempt_json(a) {
        return Err("Extra/unadmitted Board Move request".into());
    }
    let RequestBody::Form(fields) = &r.body else {
        return Err("Expected form".into());
    };
    let url = url::Url::parse(&r.url).map_err(|_| "Invalid URL")?;
    let cells = if s.checkbox {
        json!([{"row_id":ROW,"column_id":"C_CHECK","checkbox":i==0}])
    } else {
        json!([{"row_id":ROW,"column_id":"C_SELECT","select":if i==0{vec![]}else{vec!["OPT_DONE"]}}])
    };
    let expected = json!([
        ["token", "fixture-not-a-token"],
        ["list_id", "F_LIST_FIXTURE"],
        ["cells", serde_json::to_string(&cells).unwrap()],
        ["_x_reason", "bulk-update-cells"]
    ]);
    if url.scheme() != "https"
        || url.host_str() != Some("example.test")
        || url.path() != "/api/lists.cells.update"
        || r.retry_safe()
        || json!(fields) != expected
    {
        return Err("Actual four-field cell form differs".into());
    }
    let event = json!({"method":r.method,"path":url.path(),"fields":fields,"retrySafe":r.retry_safe(),"attempt":attempt_json(a)});
    state_mut(shell).requests.push(event);
    Ok(())
}
pub(crate) fn completion(
    shell: &mut ShellState,
    a: &EditAttempt,
    uncertain: bool,
) -> Result<(), String> {
    if !armed(shell) {
        return Ok(());
    }
    let s = state(shell)?;
    let i = s.completed.len();
    if i >= 2
        || s.requests.len() != i + 1
        || s.requests[i]["attempt"] != attempt_json(a)
        || uncertain != (i == 0)
        || shell.slack_lists.fixture.as_ref().unwrap().records != s.pages[i + 1]
    {
        return Err("Wrong Board Move completion".into());
    }
    let v = shell.slack_lists.view.as_ref().ok_or("No visit")?;
    let l = v.loaded.as_ref().ok_or("No loaded data")?;
    let op = operation(shell)?;
    if l.document.raw != s.document
        || l.records.raw != s.pages[i]
        || op["phase"] != "Uncertain"
        || op["attempt"] != attempt_json(a)
    {
        return Err("ACK changed loaded observation or lost receipt".into());
    }
    let event = json!({"attempt":attempt_json(a),"result":if uncertain{"response_lost"}else{"positive_ack"},"operation":op,"loaded":l.records.raw,"document":l.document.raw});
    state_mut(shell).completed.push(event);
    Ok(())
}
pub(crate) fn queue_missing(shell: &mut ShellState) -> Result<(), String> {
    let s = state(shell)?;
    if !s.checkbox
        || s.queued
        || s.reads.len() != 1
        || s.completed.len() != 1
        || s.duplicate.is_none()
        || operation(shell)?["phase"] != "Uncertain"
    {
        return Err("Missing read challenge out of order".into());
    }
    state_mut(shell).queued = true;
    Ok(())
}
pub(crate) fn read_input(
    shell: &mut ShellState,
    a: &ReadAttempt,
    result: &mut Result<Loaded, Error>,
) -> Result<(), String> {
    if !armed(shell) {
        return Ok(());
    }
    let s = state(shell)?;
    let i = s.reads.len();
    let missing = s.checkbox && i == 1;
    let want = if i == 0 {
        0
    } else if i == 1 || s.checkbox && i == 2 {
        1
    } else {
        2
    };
    if i >= if s.checkbox { 4 } else { 3 }
        || s.completed.len() != want
        || s.queued != missing
        || a.generation != s.visit
        || !a.current(shell)
        || !a.owns_read(shell)
    {
        return Err("Unexpected owned local Board Move read".into());
    }
    let data = result
        .as_mut()
        .map_err(|_| "Successful fixture read required")?;
    if data.document.raw != s.document || data.records.raw != s.pages[want] {
        return Err("Wrong synthetic canonical read".into());
    }
    if missing {
        data.records = RecordsPage::parse(s.pages[0].clone()).map_err(|e| e.to_string())?;
    }
    let op = operation(shell)?;
    if i == 0 && !op.is_null() || i > 0 && op["phase"] != "Uncertain" {
        return Err("Wrong read receipt phase".into());
    }
    let readback = if let Some(m) = shell.slack_lists.operations.get("F_LIST_FIXTURE") {
        match m.attempt.edit.readback_value(&data.document, &data.records) {
            None => Value::Null,
            Some(CellEditValue::Boolean(b)) => json!(b),
            Some(CellEditValue::Select(v)) => json!(v),
            _ => return Err("Wrong readback kind".into()),
        }
    } else {
        Value::Null
    };
    if missing && !readback.is_null() {
        return Err("Missing value synthesized".into());
    }
    let event = json!({"kind":"local_fixture_read","read":a.read,"revision":a.revision,"visit":a.generation,"readback":readback,"before":op,"document":data.document.raw,"population":data.records.raw,"after":null});
    let s = state_mut(shell);
    if i == 0 {
        s.baseline_ticket = Some(a.clone());
    }
    s.queued = false;
    s.reads.push(event);
    Ok(())
}
pub(crate) fn read_completed(shell: &mut ShellState, a: &ReadAttempt) -> Result<(), String> {
    if !armed(shell) {
        return Ok(());
    }
    let s = state(shell)?;
    let e = s.reads.last().ok_or("Missing read input")?;
    let v = shell.slack_lists.view.as_ref().ok_or("No visit")?;
    let l = v.loaded.as_ref().ok_or("No loaded data")?;
    let op = operation(shell)?;
    let missing = s.checkbox && s.reads.len() == 2;
    if e["read"] != a.read
        || !e["after"].is_null()
        || v.loading
        || v.read != 0
        || l.document.raw != e["document"]
        || l.records.raw != e["population"]
        || missing && op != e["before"]
        || !missing && !op.is_null()
    {
        return Err("Read did not apply or settle exact receipt".into());
    }
    let after = json!({"read":v.read,"revision":v.revision,"operation":op});
    state_mut(shell).reads.last_mut().unwrap()["after"] = after;
    Ok(())
}
pub(crate) fn duplicate_consumed_read(shell: &mut ShellState) -> Result<Value, String> {
    let s = state(shell)?;
    if s.reads.len() != 1 || s.completed.len() != 1 || s.duplicate.is_some() {
        return Err("Duplicate read challenge out of order".into());
    }
    let ticket = s.baseline_ticket.clone().ok_or("No real baseline ticket")?;
    if ticket.owns_read(shell) || ticket.current(shell) {
        return Err("Baseline read ticket not retired".into());
    }
    let mut data = loaded(shell.slack_lists.fixture.as_ref().unwrap());
    data.records = RecordsPage::parse(s.pages[2].clone()).map_err(|e| e.to_string())?;
    let before = snapshot(shell)?;
    finish_read(shell, &ticket, Ok(data));
    let after = snapshot(shell)?;
    if before != after {
        return Err("Consumed duplicate completion changed state".into());
    }
    let event = json!({"read":ticket.read,"revision":ticket.revision,"ownsRead":false,"current":false,"ignored":true,"unchanged":true,"kind":"consumed_local_completion"});
    state_mut(shell).duplicate = Some(event.clone());
    Ok(event)
}

#[cfg(test)]
mod tests;
