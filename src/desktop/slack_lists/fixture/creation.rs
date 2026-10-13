//! Disconnected create backend; a lost response never identifies a blank by title.
use super::*;
use crate::slack_lists::create;

#[derive(Clone, Default)]
pub(super) struct State {
    pending: Option<create::Attempt>,
    attempts: usize,
    uncertain: bool,
    prepared_valid: bool,
}
pub(crate) struct CreationSnapshot {
    document: Value,
    records: Value,
    fixture: State,
    controller: create::State,
}
pub(crate) fn add_creation_views(shell: &mut ShellState) -> Option<CreationSnapshot> {
    if !enabled(shell) {
        return None;
    }
    let fixture = shell.slack_lists.fixture.as_mut()?;
    let snapshot = CreationSnapshot {
        document: fixture.document.clone(),
        records: fixture.records.clone(),
        fixture: fixture.creation.clone(),
        controller: shell.slack_lists.create.clone(),
    };
    fixture.creation = State::default();
    fixture.document["list_limits"] = json!({"over_row_maximum":false,"row_count":fixture.records["records"].as_array()?.len(),"row_count_limit":1000});
    fixture.document["list_metadata"]["schema"].as_array_mut()?.push(json!({"id":"C_CREATE_DONE","key":"todo_completed","name":"Completed","type":"todo_completed"}));
    let columns = json!([{"id":"C_NAME","key":"NAME","visible":true},{"id":"C_NUMBER","key":"estimate","visible":true},{"id":"C_CREATE_DONE","key":"todo_completed","visible":true}]);
    fixture.document["list_metadata"]["views"].as_array_mut()?.extend([
        json!({"id":"V_CREATE_FILTER","name":"High estimates only","type":"table","position":"501","columns":columns,"filters":[{"column_id":"C_NUMBER","key":"estimate","operator":"greater_than_or_equal","values":[68]}],"sorts":[]}),
        json!({"id":"V_CREATE_COMPLETED","name":"Completed items","type":"table","position":"502","default_view_key":"completed_items","columns":columns,"filters":[],"sorts":[]}),
        json!({"id":"V_CREATE_BOARD","name":"New cards","type":"board","position":"503","columns":columns,"filters":[],"sorts":[]}),
    ]);
    refresh(shell);
    Some(snapshot)
}
pub(crate) fn creation_attempts(shell: &ShellState) -> usize {
    shell
        .slack_lists
        .fixture
        .as_ref()
        .map_or(0, |fixture| fixture.creation.attempts)
}
pub(crate) fn creation_prepared_valid(shell: &ShellState) -> bool {
    shell
        .slack_lists
        .fixture
        .as_ref()
        .is_some_and(|fixture| fixture.creation.prepared_valid)
}
pub(crate) fn set_creation_limit(shell: &mut ShellState, over: Option<bool>) {
    if !enabled(shell) {
        return;
    }
    let fixture = shell.slack_lists.fixture.as_mut().unwrap();
    match over {
        Some(value) => fixture.document["list_limits"]["over_row_maximum"] = json!(value),
        None => {
            fixture.document["list_limits"]
                .as_object_mut()
                .unwrap()
                .remove("over_row_maximum");
        }
    }
    refresh(shell);
}
pub(crate) fn set_creation_uncertain(shell: &mut ShellState) {
    if enabled(shell) {
        shell
            .slack_lists
            .fixture
            .as_mut()
            .unwrap()
            .creation
            .uncertain = true;
    }
}
pub(in crate::slack_lists) fn dispatch_creation(shell: &mut ShellState, attempt: create::Attempt) {
    if !enabled(shell) {
        return;
    }
    let current = loaded(shell.slack_lists.fixture.as_ref().unwrap());
    if let Err(error) = create::validate(shell, &attempt, &current) {
        create::rejected(shell, &attempt, error, None);
        return;
    }
    let session = WorkspaceSession {
        team_id: "T_LIST_FIXTURE".into(),
        enterprise_id: None,
        user_id: "U_LIST_FIXTURE".into(),
        name: "Fixture".into(),
        url: "https://example.test".into(),
        token: "fixture-not-a-token".into(),
    };
    let request = api::list_record_create(
        &shell.core.client,
        &session,
        &attempt.intent,
        &attempt.timestamp,
    );
    let no_retry = !request.retry_safe();
    let valid = match request.body {
        RequestBody::Form(fields) => {
            let field = |key: &str| {
                fields
                    .iter()
                    .find(|(name, _)| *name == key)
                    .map(|(_, value)| value.as_str())
            };
            field("list_id") == Some(attempt.intent.list_id())
                && field("position") == Some("")
                && field("updated_timestamp") == Some(attempt.timestamp.as_str())
                && field("_x_reason") == Some("create-record")
                && field("view_id").is_none()
                && field("fields").is_none()
                && field("parent_record_id").is_none()
                && field("duplicated_record_id").is_none()
                && field("initial_fields")
                    .and_then(|value| serde_json::from_str::<Value>(value).ok())
                    .as_ref()
                    == attempt.intent.initial_fields()
        }
        _ => false,
    };
    let fixture = shell.slack_lists.fixture.as_mut().unwrap();
    fixture.creation.attempts += 1;
    fixture.creation.prepared_valid =
        valid && no_retry && (fixture.creation.attempts == 1 || fixture.creation.prepared_valid);
    fixture.creation.pending = Some(attempt);
}
pub(crate) fn complete_creation(shell: &mut ShellState) -> Option<String> {
    if !enabled(shell) {
        return None;
    }
    let fixture = shell.slack_lists.fixture.as_mut()?;
    let attempt = fixture.creation.pending.take()?;
    let id = format!("R_CREATED_{:03}", fixture.creation.attempts);
    let mut fields = Vec::new();
    for cell in attempt
        .intent
        .initial_fields()
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let column = fixture.document["list_metadata"]["schema"]
            .as_array()?
            .iter()
            .find(|column| column["id"] == cell["column_id"])?;
        let mut field = cell.clone();
        field["key"] = column["key"].clone();
        fields.push(field);
    }
    let record = json!({"id":id,"list_id":attempt.intent.list_id(),"fields":fields,"date_created":1800000000+fixture.creation.attempts,"created_by":"U_LIST_FIXTURE","position":"","updated_timestamp":attempt.timestamp.as_str()});
    fixture.records["records"]
        .as_array_mut()?
        .push(record.clone());
    fixture.document["list_limits"]["row_count"] =
        json!(fixture.records["records"].as_array()?.len());
    let result = if std::mem::take(&mut fixture.creation.uncertain) {
        Err(Error::Transport("fixture lost create response".into()))
    } else {
        Ok(json!({"ok":true,"record":record}))
    };
    create::finish(shell, &attempt, result);
    Some(id)
}
pub(crate) fn created_record(shell: &ShellState, id: &str) -> Option<Value> {
    shell.slack_lists.fixture.as_ref()?.records["records"]
        .as_array()?
        .iter()
        .find(|record| record["id"] == id)
        .cloned()
}
pub(crate) fn restore_creation_views(shell: &mut ShellState, snapshot: CreationSnapshot) {
    if !enabled(shell) {
        return;
    }
    let fixture = shell.slack_lists.fixture.as_mut().unwrap();
    fixture.document = snapshot.document;
    fixture.records = snapshot.records;
    fixture.creation = snapshot.fixture;
    shell.slack_lists.create = snapshot.controller;
    refresh(shell);
}
