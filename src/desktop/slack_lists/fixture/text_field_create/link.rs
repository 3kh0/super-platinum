//! Exact-gated observations of the existing cell fixture; no alternative worker.
use super::super::*;
use crate::slack_lists::rename::{self, fixture as create};
use super_platinum_core::slack::client::PreparedRequest;
pub(crate) const COLUMN: &str = "C_LINK_OBSERVED";
pub(crate) const KEY: &str = "link_observed";
pub(crate) const ROW: &str = "R_LIST_003";

#[derive(Clone)]
pub(crate) struct State {
    scope: Scope,
    visit: u64,
    selection: u64,
    document: Value,
    pages: [Value; 4],
    sync: Value,
    preflights: Vec<Value>,
    requests: Vec<Value>,
    completed: Vec<Value>,
    reads: Vec<Value>,
    #[cfg(test)]
    test_only: bool,
    error: Option<String>,
}
fn enabled(shell: &ShellState) -> bool {
    #[cfg(test)]
    if super::super::enabled(shell)
        && shell
            .slack_lists
            .fixture
            .as_ref()
            .and_then(|f| f.link_create.as_ref())
            .is_some_and(|s| s.test_only)
    {
        return true;
    }
    super::super::enabled(shell)
        && create::field_create_mode() == Some(create::FieldCreateMode::Link)
}
fn state(shell: &ShellState) -> Result<&State, String> {
    let s = shell
        .slack_lists
        .fixture
        .as_ref()
        .and_then(|f| f.link_create.as_ref())
        .ok_or("Link diagnostic not armed")?;
    if !enabled(shell)
        || !s.scope.current(shell)
        || shell
            .slack_lists
            .scope
            .as_ref()
            .is_none_or(|scope| !std::sync::Arc::ptr_eq(&scope.identity, &s.scope.identity))
        || generation(shell) != Some(s.visit)
        || rename::selection(shell) != Some(s.selection)
        || rename::presentation(shell).is_some()
        || shell.slack_lists.fixture.as_ref().unwrap().document != s.document
    {
        return Err("Link diagnostic owner/document retired".into());
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
        .link_create
        .as_mut()
        .unwrap()
}
pub(crate) fn armed(shell: &ShellState) -> bool {
    shell
        .slack_lists
        .fixture
        .as_ref()
        .is_some_and(|f| f.link_create.is_some())
}
pub(crate) fn fail(shell: &mut ShellState, error: String) {
    if armed(shell) {
        state_mut(shell).error.get_or_insert(error);
    }
}
/// Independent literal expected raw pages, preserving every unrelated record byte.
pub(crate) fn pages(original: &Value) -> Result<[Value; 4], String> {
    let records = original["records"]
        .as_array()
        .ok_or("Missing fixture records")?;
    if records.len() != 73
        || records.iter().filter(|r| r["id"] == ROW).count() != 1
        || records.iter().any(|r| {
            r["fields"].as_array().is_none_or(|fs| {
                fs.iter()
                    .any(|f| f["key"] == KEY || f["column_id"] == COLUMN)
            })
        })
    {
        return Err("Expected complete raw73 page with absent created field".into());
    }
    let changed = |value: Value| {
        let mut page = original.clone();
        let row = page["records"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|r| r["id"] == ROW)
            .unwrap();
        row["fields"]
            .as_array_mut()
            .unwrap()
            .push(json!({"key":KEY,"link":value}));
        page
    };
    Ok([
        original.clone(),
        changed(
            json!([{"originalUrl":"http://example.com/new","displayName":"R\u{e9}f\u{e9}rence \u{6771}\u{4eac}","displayAsUrl":false,"attachment":{}}]),
        ),
        changed(
            json!([{"originalUrl":"http://example.com/new","displayName":"R\u{e9}f\u{e9}rence \u{6771}\u{4eac}","displayAsUrl":true,"attachment":{}}]),
        ),
        changed(json!([])),
    ])
}
pub(crate) fn arm(
    shell: &mut ShellState,
    before: &Value,
    after: &Value,
    original: &Value,
    serial: u64,
) -> Result<Value, String> {
    if !enabled(shell)
        || armed(shell)
        || serial == 0
        || rename::presentation(shell).is_some()
        || create::text_create_registry(shell)?
            != json!({"present":false,"serial":null,"job":null,"intent":false})
        || !shell.slack_lists.operations.is_empty()
    {
        return Err("Link sync requires completed creation and actual Done".into());
    }
    let ledger = create::snapshot(shell)?;
    let f = shell
        .slack_lists
        .fixture
        .as_ref()
        .ok_or("Missing fixture")?;
    let v = shell.slack_lists.view.as_ref().ok_or("Missing visit")?;
    let loaded = v.loaded.as_ref().ok_or("Missing observed page")?;
    if ledger["server"] != *after
        || ledger["requests"].as_array().map(Vec::len) != Some(11)
        || ledger["finished"]
            != json!([
            {"serial":serial,"kind":"write","applied":false},
            {"serial":serial,"kind":"check","applied":true},
            {"serial":serial,"kind":"check","applied":false},
            {"serial":serial,"kind":"check","applied":true}])
        || ledger["writePending"] != false
        || ledger["readPending"] != false
        || loaded.document.raw != *after
        || loaded.records.raw != *original
        || f.document != *before
        || f.records != *original
        || f.attempts != 0
        || f.pending.is_some()
        || f.error
        || loaded.document.column(COLUMN).is_none_or(|c| {
            c.key != KEY
                || c.name != "Reference Équipe 東京"
                || c.raw.get("options").is_some()
                || c.raw["type"] != "link"
        })
    {
        return Err("Observed identity/full raw sync precondition differs".into());
    }
    let scope = shell.slack_lists.scope.clone().ok_or("Missing scope")?;
    let visit = generation(shell).ok_or("Missing visit")?;
    let selection = rename::selection(shell).ok_or("Missing selection")?;
    let pages = pages(original)?;
    let sync = json!({"creationSerial":serial,"before":before,"after":after,"population":original,"visit":visit,"selection":selection,"owner":scope.stable_owner(),"epoch":shell.core.account_epoch});
    let f = shell.slack_lists.fixture.as_mut().unwrap();
    f.document = after.clone();
    f.link_create = Some(State {
        scope,
        visit,
        selection,
        document: after.clone(),
        pages,
        sync: sync.clone(),
        preflights: vec![],
        requests: vec![],
        completed: vec![],
        reads: vec![],
        #[cfg(test)]
        test_only: false,
        error: None,
    });
    state(shell)?;
    Ok(sync)
}
fn attempt_json(attempt: &EditAttempt) -> Value {
    json!({"serial":attempt.serial,"visit":attempt.generation,"selection":attempt.selection_generation,"revision":attempt.revision,"owner":attempt.scope.stable_owner(),"list":attempt.edit.list_id(),"row":attempt.edit.record_id(),"column":attempt.edit.column_id(),"key":attempt.edit.column_key(),"cells":attempt.edit.cells()})
}
pub(crate) fn operation(shell: &ShellState) -> Result<Value, String> {
    let s = state(shell)?;
    if shell.slack_lists.operations.len() > 1 {
        return Err("Extra cell operation".into());
    }
    let Some(m) = shell.slack_lists.operations.get("F_LIST_FIXTURE") else {
        return Ok(Value::Null);
    };
    if !m.attempt.scope.stable_current(shell)
        || m.attempt.scope.stable_owner() != s.scope.stable_owner()
        || m.attempt.generation != s.visit
        || m.attempt.selection_generation != s.selection
    {
        return Err("Cell receipt ownership changed".into());
    }
    Ok(
        json!({"attempt":attempt_json(&m.attempt),"phase":match m.phase {Phase::Preparing=>"Preparing",Phase::Saving=>"Saving",Phase::Uncertain=>"Uncertain"}}),
    )
}
pub(crate) fn snapshot(shell: &ShellState) -> Result<Value, String> {
    let s = state(shell)?;
    let f = shell.slack_lists.fixture.as_ref().unwrap();
    Ok(
        json!({"sync":s.sync,"preflights":s.preflights,"requests":s.requests,"completed":s.completed,"reads":s.reads,"server":f.records,"pending":f.pending.as_ref().map(attempt_json),"attemptCount":f.attempts,"preparedValid":f.prepared_valid,"operation":operation(shell)?}),
    )
}
pub(crate) fn preflight(
    shell: &mut ShellState,
    attempt: &EditAttempt,
    canonical: &Loaded,
) -> Result<(), String> {
    if !armed(shell) {
        return Ok(());
    }
    let s = state(shell)?;
    let index = s.preflights.len();
    if index > 2
        || s.requests.len() != index
        || s.completed.len() != index
        || s.reads.len() != index
        || canonical.document.raw != s.document
        || canonical.records.raw != s.pages[index]
        || !attempt.scope.current(shell)
        || attempt.generation != s.visit
        || attempt.selection_generation != s.selection
    {
        return Err("Unexpected Link preflight sequence/raw page".into());
    }
    let expected = CellEdit::capture(
        &canonical.document,
        &canonical.records,
        ROW,
        COLUMN,
        canonical.allow_lists.as_deref(),
        canonical.personal_task_lists.as_ref(),
        desired(index),
    )
    .map_err(|e| e.to_string())?;
    if attempt.edit != expected
        || !matches!(
            shell
                .slack_lists
                .operations
                .get("F_LIST_FIXTURE")
                .map(|m| m.phase),
            Some(Phase::Saving)
        )
    {
        return Err("Actual typed Link custody or admitted phase differs".into());
    }
    let receipt = json!({"attempt":attempt_json(attempt),"document":canonical.document.raw,"population":canonical.records.raw,"allow":canonical.allow_lists,"phase":"Saving"});
    state_mut(shell).preflights.push(receipt);
    Ok(())
}
pub(crate) fn request(
    shell: &mut ShellState,
    attempt: &EditAttempt,
    request: &PreparedRequest,
) -> Result<(), String> {
    if !armed(shell) {
        return Ok(());
    }
    let s = state(shell)?;
    if s.requests.len() >= 3
        || s.preflights.len() != s.requests.len() + 1
        || s.preflights.last().unwrap()["attempt"] != attempt_json(attempt)
    {
        return Err("Extra/unadmitted Link request".into());
    }
    let RequestBody::Form(fields) = &request.body else {
        return Err("Cell request not form".into());
    };
    let url = url::Url::parse(&request.url).map_err(|_| "Invalid cell URL")?;
    if url.scheme() != "https" || url.host_str() != Some("example.test") {
        return Err("Nonfixture cell origin".into());
    }
    let value = json!({"method":request.method,"path":url.path(),"fields":fields,"retrySafe":request.retry_safe(),"attempt":attempt_json(attempt)});
    state_mut(shell).requests.push(value);
    Ok(())
}
pub(crate) fn completion(
    shell: &mut ShellState,
    attempt: &EditAttempt,
    uncertain: bool,
) -> Result<(), String> {
    if !armed(shell) {
        return Ok(());
    }
    let s = state(shell)?;
    let index = s.completed.len();
    if index >= 3
        || s.requests.len() != index + 1
        || s.requests[index]["attempt"] != attempt_json(attempt)
        || uncertain
        || shell.slack_lists.fixture.as_ref().unwrap().records != s.pages[index + 1]
    {
        return Err("Unexpected actual Link completion".into());
    }
    let op = operation(shell)?;
    if op["phase"] != "Uncertain" || op["attempt"] != attempt_json(attempt) {
        return Err("Completion failed to retain actual receipt".into());
    }
    state_mut(shell).completed.push(json!({"attempt":attempt_json(attempt),"result":if uncertain {"response_lost"} else {"positive_ack"},"operation":op}));
    Ok(())
}
pub(crate) fn read_input(
    shell: &mut ShellState,
    attempt: &ReadAttempt,
    result: &mut Result<Loaded, Error>,
) -> Result<(), String> {
    if !armed(shell) {
        return Ok(());
    }
    let s = state(shell)?;
    let index = s.reads.len();
    if index >= 3
        || s.completed.len() != index + 1
        || attempt.generation != s.visit
        || !attempt.scope.current(shell)
    {
        return Err("Unexpected local Link read".into());
    }
    let data = result
        .as_mut()
        .map_err(|_| "Expected successful local fixture read")?;
    if data.document.raw != s.document || data.records.raw != s.pages[index + 1] {
        return Err("Wrong canonical synthetic server read".into());
    }
    let op = operation(shell)?;
    if op["phase"] != "Uncertain" {
        return Err("Local read requires unresolved receipt".into());
    }
    let mutation = shell
        .slack_lists
        .operations
        .get("F_LIST_FIXTURE")
        .ok_or("Missing actual receipt")?;
    let readback = match mutation
        .attempt
        .edit
        .readback_value(&data.document, &data.records)
    {
        None => Value::Null,
        Some(crate::slack_lists::CellEditValue::Link(value)) => {
            json!({"known":true,"value":typed(&value)})
        }
        _ => return Err("Non-Link typed readback".into()),
    };
    let event = json!({"readback":readback,"read":attempt.read,"revision":attempt.revision,"visit":attempt.generation,"before":op,"document":data.document.raw,"population":data.records.raw,"after":null});
    let s = state_mut(shell);
    s.reads.push(event);
    Ok(())
}
pub(crate) fn read_completed(shell: &mut ShellState, attempt: &ReadAttempt) -> Result<(), String> {
    if !armed(shell) {
        return Ok(());
    }
    let s = state(shell)?;
    let event = s.reads.last().ok_or("Read completion lacks input")?;
    let v = shell
        .slack_lists
        .view
        .as_ref()
        .ok_or("Read visit missing")?;
    let loaded = v.loaded.as_ref().ok_or("Read failed to apply")?;
    if event["read"] != attempt.read
        || !event["after"].is_null()
        || v.loading
        || loaded.document.raw != event["document"]
        || loaded.records.raw != event["population"]
    {
        return Err("Local read failed actual application".into());
    }
    let op = operation(shell)?;
    if !op.is_null() || v.read != 0 {
        return Err("Link read resolved wrong receipt".into());
    }
    let after = json!({"read":v.read,"revision":v.revision,"operation":op});
    state_mut(shell).reads.last_mut().unwrap()["after"] = after;
    Ok(())
}

#[cfg(test)]
mod tests;

pub(crate) fn desired(index: usize) -> CellEditValue {
    CellEditValue::Link(
        (index < 2).then(|| super_platinum_core::slack_lists::LinkValue {
            original_url: "http://example.com/new".into(),
            display_name: Some("R\u{e9}f\u{e9}rence \u{6771}\u{4eac}".into()),
            display_as_url: index == 1,
        }),
    )
}
pub(crate) fn typed(value: &Option<super_platinum_core::slack_lists::LinkValue>) -> Value {
    value.as_ref().map(|v|json!({"original_url":v.original_url,"display_name":v.display_name,"display_as_url":v.display_as_url})).unwrap_or(Value::Null)
}
