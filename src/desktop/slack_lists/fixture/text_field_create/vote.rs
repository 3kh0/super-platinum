//! Exact-gated observations of the existing cell fixture; no alternative worker.
use super::super::*;
use crate::slack_lists::rename::{self, fixture as create};
use super_platinum_core::slack::client::PreparedRequest;
pub(crate) mod media;

pub(crate) const COLUMN: &str = "C_VOTE_OBSERVED";
pub(crate) const KEY: &str = "vote_observed";
pub(crate) const ROW: &str = "R_LIST_003";

#[derive(Clone)]
pub(crate) struct State {
    mode: create::FieldCreateMode,
    different: Option<Value>,
    navigations: Vec<Value>,
    record_opens: Vec<Value>,
    scope: Scope,
    visit: u64,
    selection: u64,
    document: Value,
    pages: [Value; 5],
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
            .and_then(|f| f.vote.as_ref())
            .is_some_and(|s| s.test_only)
    {
        return true;
    }
    super::super::enabled(shell)
        && create::field_create_mode().is_some_and(create::FieldCreateMode::is_vote)
}
fn state(shell: &ShellState) -> Result<&State, String> {
    let s = shell
        .slack_lists
        .fixture
        .as_ref()
        .and_then(|f| f.vote.as_ref())
        .ok_or("Vote diagnostic not armed")?;
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
        return Err("Vote diagnostic owner/document retired".into());
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
        .vote
        .as_mut()
        .unwrap()
}
pub(crate) fn armed(shell: &ShellState) -> bool {
    shell
        .slack_lists
        .fixture
        .as_ref()
        .is_some_and(|f| f.vote.is_some())
}
pub(crate) fn fail(shell: &mut ShellState, error: String) {
    if armed(shell) {
        state_mut(shell).error.get_or_insert(error);
    }
}
/// Independent literal expected raw pages, preserving every unrelated record byte.
pub(crate) fn pages(original: &Value) -> Result<[Value; 5], String> {
    let records = original["records"].as_array().ok_or("Missing raw73")?;
    if records.len() != 73
        || records.iter().filter(|r| r["id"] == ROW).count() != 1
        || records.iter().any(|r| {
            r["fields"].as_array().is_none_or(|fs| {
                fs.iter()
                    .any(|f| f["key"] == KEY || f["column_id"] == COLUMN)
            })
        })
    {
        return Err("Vote field must be genuinely absent".into());
    }
    let changed = |users: Value| {
        let mut p = original.clone();
        let row = p["records"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|r| r["id"] == ROW)
            .unwrap();
        row["fields"]
            .as_array_mut()
            .unwrap()
            .push(json!({"key":KEY,"user":users}));
        p
    };
    Ok([
        original.clone(),
        changed(json!(["U0"])),
        changed(json!(["U_OTHER_B", "U_OTHER_A"])),
        changed(json!(["U_OTHER_B", "U_OTHER_A", "U0"])),
        changed(json!(["U_OTHER_B", "U_OTHER_A"])),
    ])
}
/// Explicit simulated concurrent server change; never replaces the loaded projection.
pub(crate) fn different(shell: &mut ShellState) -> Result<Value, String> {
    let s = state(shell)?;
    let f = shell.slack_lists.fixture.as_ref().unwrap();
    if s.different.is_some()
        || s.requests.len() != 1
        || s.completed.len() != 1
        || !s.reads.is_empty()
        || f.pending.is_some()
        || f.records != s.pages[1]
        || operation(shell)?["phase"] != "Uncertain"
    {
        return Err("Different-vote transition out of order".into());
    }
    let before = f.records.clone();
    let after = s.pages[2].clone();
    let event = json!({"before":before,"after":after,"operation":operation(shell)?,"owner":s.scope.stable_owner(),"visit":s.visit,"selection":s.selection,"synthetic":true});
    shell.slack_lists.fixture.as_mut().unwrap().records = after;
    state_mut(shell).different = Some(event.clone());
    Ok(event)
}
pub(crate) fn arm(
    shell: &mut ShellState,
    before: &Value,
    after: &Value,
    original: &Value,
    serial: u64,
    mode: create::FieldCreateMode,
) -> Result<Value, String> {
    if !enabled(shell)
        || !mode.is_vote()
        || shell.core.active_team.as_deref() != Some("T1")
        || shell.self_account.user_id != "U0"
        || armed(shell)
        || serial == 0
        || rename::presentation(shell).is_some()
        || create::text_create_registry(shell)?
            != json!({"present":false,"serial":null,"job":null,"intent":false})
        || !shell.slack_lists.operations.is_empty()
    {
        return Err("Vote sync requires completed creation and actual Done".into());
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
        || f.uncertain
        || loaded.document.column(COLUMN).is_none_or(|c| {
            c.key != KEY
                || c.name != "Support Équipe 東京"
                || c.raw.get("options") != Some(&mode.vote_options())
                || c.raw["type"] != "vote"
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
    f.uncertain = true; // One explicitly lost first cell reply, consumed by the existing worker.
    f.vote = Some(State {
        mode,
        different: None,
        navigations: vec![],
        record_opens: vec![],
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
    let authority = match attempt.authority {
        EditAuthority::Layout => json!({"kind":"table"}),
        EditAuthority::RecordDetail(g) => json!({"kind":"detail","generation":g}),
        _ => json!({"kind":"unsupported"}),
    };
    json!({"authority":authority,"serial":attempt.serial,"visit":attempt.generation,"selection":attempt.selection_generation,"revision":attempt.revision,"owner":attempt.scope.stable_owner(),"list":attempt.edit.list_id(),"row":attempt.edit.record_id(),"column":attempt.edit.column_id(),"key":attempt.edit.column_key(),"cells":attempt.edit.cells()})
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
        json!({"sync":s.sync,"case":s.mode.prefix(),"different":s.different,"selection":s.selection,"navigations":s.navigations,"recordOpens":s.record_opens,"preflights":s.preflights,"requests":s.requests,"completed":s.completed,"reads":s.reads,"server":f.records,"pending":f.pending.as_ref().map(attempt_json),"attemptCount":f.attempts,"preparedValid":f.prepared_valid,"operation":operation(shell)?}),
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
        || canonical.records.raw != s.pages[[0, 2, 3][index]]
        || !attempt.scope.current(shell)
        || attempt.generation != s.visit
        || attempt.selection_generation != s.selection
    {
        return Err("Unexpected Vote preflight sequence/raw page".into());
    }
    let expected = CellEdit::capture_vote(
        &canonical.document,
        &canonical.records,
        ROW,
        COLUMN,
        canonical.allow_lists.as_deref(),
        canonical.personal_task_lists.as_ref(),
        &s.scope.stable_owner().2,
        if index == 2 {
            super_platinum_core::slack_lists::VoteIntent::RemoveMyVote
        } else {
            super_platinum_core::slack_lists::VoteIntent::AddMyVote
        },
    )
    .map_err(|e| e.to_string())?;
    if attempt.edit != expected
        || (index < 2 && !matches!(attempt.authority, EditAuthority::Layout))
        || (index == 2 && !matches!(attempt.authority, EditAuthority::RecordDetail(_)))
        || !matches!(
            shell
                .slack_lists
                .operations
                .get("F_LIST_FIXTURE")
                .map(|m| m.phase),
            Some(Phase::Saving)
        )
    {
        return Err("Actual typed Vote custody or admitted phase differs".into());
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
        return Err("Extra/unadmitted Vote request".into());
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
        || uncertain != (index == 0)
        || shell.slack_lists.fixture.as_ref().unwrap().records != s.pages[[1, 3, 4][index]]
    {
        return Err("Unexpected actual Vote completion".into());
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
        return Err("Unexpected local Vote read".into());
    }
    let data = result
        .as_mut()
        .map_err(|_| "Expected successful local fixture read")?;
    if data.document.raw != s.document || data.records.raw != s.pages[[2, 3, 4][index]] {
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
        Some(crate::slack_lists::CellEditValue::Votes(value)) => {
            json!({"known":true,"value":value})
        }
        _ => return Err("Nonvote typed readback".into()),
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
        return Err("Vote read resolved wrong receipt".into());
    }
    let after = json!({"read":v.read,"revision":v.revision,"operation":op});
    state_mut(shell).reads.last_mut().unwrap()["after"] = after;
    Ok(())
}

/// A real saved-view gesture is observed only between completed cell attempts.
/// This changes diagnostic custody, not the application view or mutation state.
pub(crate) fn adopt_view(shell: &mut ShellState, expected: &str) -> Result<Value, String> {
    if !enabled(shell) {
        return Err("Vote navigation gate".into());
    }
    let s = shell
        .slack_lists
        .fixture
        .as_ref()
        .and_then(|f| f.vote.as_ref())
        .ok_or("Vote unarmed")?;
    let index = s.navigations.len();
    let destinations = ["V_VOTE_BOARD", "V_FILTER"];
    let reads = [2, 3];
    let v = shell
        .slack_lists
        .view
        .as_ref()
        .ok_or("Missing navigation view")?;
    let loaded = v.loaded.as_ref().ok_or("Missing navigation data")?;
    let selection = rename::selection(shell).ok_or("Missing navigation selection")?;
    if destinations.get(index) != Some(&expected)
        || s.reads.len() != reads[index]
        || s.reads.iter().any(|r| r["after"].is_null())
        || !s.scope.current(shell)
        || generation(shell) != Some(s.visit)
        || selection == s.selection
        || !shell.slack_lists.operations.is_empty()
        || v.editor.is_some()
        || v.detail.is_some()
        || v.loading
        || v.stale
        || v.layout.selected.as_deref() != Some(expected)
        || loaded.document.raw != s.document
        || loaded.records.raw != s.pages[[0, 2, 3, 4][s.reads.len()]]
        || v.layout.find.input != "Launch item 0"
        || v.layout.find.applied != v.layout.find.input
        || v.layout.find.revision == 0
    {
        return Err("Unowned/out-of-order Vote view navigation".into());
    }
    let receipt = json!({"fromSelection":s.selection,"selection":selection,"view":expected,"reads":s.reads.len(),"find":{"input":v.layout.find.input,"applied":v.layout.find.applied,"revision":v.layout.find.revision}});
    let s = state_mut(shell);
    s.selection = selection;
    s.navigations.push(receipt.clone());
    state(shell)?;
    Ok(receipt)
}
pub(crate) fn record_open(
    shell: &mut ShellState,
    generation: u64,
    request: &PreparedRequest,
) -> Result<(), String> {
    if !armed(shell) {
        return Ok(());
    }
    let s = state(shell)?;
    let index = s.record_opens.len();
    let v = shell.slack_lists.view.as_ref().ok_or("Missing detail")?;
    let detail =
        crate::slack_lists::record_detail::presentation(v).ok_or("Missing actual detail")?;
    if index >= 1
        || s.navigations.len() != 1
        || detail.open_generation != generation
        || detail.record_id != ROW
        || !shell.slack_lists.operations.is_empty()
    {
        return Err("Extra or unowned Vote record-open".into());
    }
    let RequestBody::Form(fields) = &request.body else {
        return Err("Record-open is not a form".into());
    };
    let url = url::Url::parse(&request.url).map_err(|_| "Bad record-open URL")?;
    if request.method != "POST"
        || url.scheme() != "https"
        || url.host_str() != Some("example.test")
        || url.path() != "/api/lists.records.open"
        || serde_json::to_value(fields).map_err(|_| "Record-open fields")?
            != json!([
                ["token", "fixture-not-a-token"],
                ["list_id", "F_LIST_FIXTURE"],
                ["id", ROW],
                ["_x_reason", "open-record"]
            ])
    {
        return Err("Wrong actual record-open request".into());
    }
    let receipt = json!({"generation":generation,"method":request.method,"path":url.path(),"fields":fields,"retrySafe":request.retry_safe()});
    state_mut(shell).record_opens.push(receipt);
    Ok(())
}
#[cfg(test)]
mod tests;
