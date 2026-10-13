//! Calendar-only writes; timestamp and personal-task semantics stay excluded.
use super::*;

pub(crate) struct DateEditSnapshot {
    document: Value,
    records: Value,
}
pub(crate) fn add_date_edits(shell: &mut ShellState) -> Option<DateEditSnapshot> {
    if !enabled(shell) {
        return None;
    }
    let fixture = shell.slack_lists.fixture.as_mut()?;
    let snapshot = DateEditSnapshot {
        document: fixture.document.clone(),
        records: fixture.records.clone(),
    };
    fixture.document["list_metadata"]["schema"].as_array_mut()?.extend([
        json!({"id":"C_EDIT_DATE","key":"edit_date","name":"Target date","type":"date"}),
        json!({"id":"C_EDIT_DUE","key":"edit_due","name":"Due date","type":"due_date"}),
        json!({"id":"C_EDIT_TODO","key":"todo_due_date","name":"Task date","type":"todo_due_date"}),
        json!({"id":"C_EDIT_LATER","key":"later_date","name":"Later timestamp","type":"todo_due_date"}),
    ]);
    let columns = json!([
        {"id":"C_NAME","key":"NAME","visible":true},
        {"id":"C_EDIT_DATE","key":"edit_date","visible":true},
        {"id":"C_EDIT_DUE","key":"edit_due","visible":true},
        {"id":"C_EDIT_TODO","key":"todo_due_date","visible":false},
        {"id":"C_EDIT_LATER","key":"later_date","visible":true}
    ]);
    fixture.document["list_metadata"]["views"].as_array_mut()?.extend([
        json!({"id":"V_EDIT_DATES","name":"Calendar editing","type":"table","position":"401","columns":columns,"filters":[{"column_id":"C_NUMBER","key":"estimate","operator":"less_than_or_equal","values":[6]}],"sorts":[]}),
        json!({"id":"V_EDIT_DATE_FILTER","name":"October 13 items","type":"table","position":"402","columns":columns,"filters":[{"column_id":"C_EDIT_DATE","key":"edit_date","operator":"equal","typed_values":[],"values":["2026-10-13"]}],"sorts":[]}),
    ]);
    for record in fixture.records["records"].as_array_mut()? {
        let id = record["id"].as_str()?.to_owned();
        let fields = record["fields"].as_array_mut()?;
        if let Some(i) = ["R_LIST_000", "R_LIST_001", "R_LIST_002"]
            .iter()
            .position(|candidate| *candidate == id)
        {
            for field in fields.iter_mut().filter(|field| field["key"] == "estimate") {
                field["number"] = json!([i]);
            }
        }
        if matches!(id.as_str(), "R_LIST_000" | "R_LIST_001") {
            fields.push(json!({"column_id":"C_EDIT_DATE","key":"edit_date","date":["2026-10-13"],"timestamp":[-1]}));
        }
        if id == "R_LIST_000" {
            fields.extend([
                json!({"column_id":"C_EDIT_DUE","key":"edit_due","date":["2026-10-13"]}),
                json!({"column_id":"C_EDIT_TODO","key":"todo_due_date","date":["2026-10-13"],"timestamp":[-1]}),
                json!({"column_id":"C_EDIT_LATER","key":"later_date","timestamp":[0]}),
            ]);
        }
    }
    refresh(shell);
    Some(snapshot)
}

pub(crate) fn change_date_edit_due(shell: &mut ShellState) {
    if !enabled(shell) {
        return;
    }
    let fixture = shell.slack_lists.fixture.as_mut().unwrap();
    let record = fixture.records["records"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|record| record["id"] == "R_LIST_000")
        .unwrap();
    let field = record["fields"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|field| field["key"] == "edit_due")
        .unwrap();
    field["date"] = json!(["2026-10-16"]);
    refresh(shell);
}

pub(crate) fn date_edit_value(shell: &ShellState, row: &str, key: &str) -> Option<Value> {
    if !enabled(shell) {
        return None;
    }
    shell.slack_lists.fixture.as_ref()?.records["records"]
        .as_array()?
        .iter()
        .find(|record| record["id"] == row)?["fields"]
        .as_array()?
        .iter()
        .find(|field| field["key"] == key)?
        .get("date")
        .cloned()
}

pub(crate) fn restore_date_edits(shell: &mut ShellState, snapshot: DateEditSnapshot) {
    if !enabled(shell) {
        return;
    }
    let fixture = shell.slack_lists.fixture.as_mut().unwrap();
    fixture.document = snapshot.document;
    fixture.records = snapshot.records;
    refresh(shell);
}
