//! Details use the same canonical backend, including hidden fields and events.
use super::*;
use super_platinum_core::slack::events::RtEvent;

pub(crate) fn add_record_details(shell: &mut ShellState) -> Option<board_views::Snapshot> {
    let snapshot = add_board_views(shell)?;
    let fixture = shell.slack_lists.fixture.as_mut()?;
    fixture.document["list_metadata"]["schema"]
        .as_array_mut()?
        .extend((0..22).map(|i| json!({"id":format!("C_DETAIL_{i:02}"),"key":format!("detail_{i:02}"),"name":format!("Additional field {}",i+1),"type":"text"})));
    fixture.document["list_metadata"]["views"].as_array_mut()?.push(json!({
        "id":"V_DETAIL_TABLE","name":"Compact items","type":"table","position":"301",
        "columns":[{"id":"C_NAME","key":"NAME","visible":true},{"id":"C_NUMBER","key":"estimate","visible":true},{"id":"C_MULTI","key":"teams","visible":false}],
        "filters":[{"column_id":"C_NUMBER","key":"estimate","operator":"less_than_or_equal","values":[6]}],"sorts":[]
    }));
    refresh(shell);
    Some(snapshot)
}

/// Deliver a genuine invalidation shape without trusting its partial cell data.
/// A failed read holds the old projection long enough to test disabled editors;
/// the ordinary fixture refresh later supplies the changed canonical record.
pub(crate) fn change_detail_number(shell: &mut ShellState, row_id: &str, number: i64) -> bool {
    if !enabled(shell) {
        return false;
    }
    let fixture = shell.slack_lists.fixture.as_mut().unwrap();
    let Some(record) = fixture.records["records"]
        .as_array_mut()
        .and_then(|records| records.iter_mut().find(|record| record["id"] == row_id))
    else {
        return false;
    };
    let Some(field) = record["fields"]
        .as_array_mut()
        .and_then(|fields| fields.iter_mut().find(|field| field["key"] == "estimate"))
    else {
        return false;
    };
    field["number"] = json!([number]);
    fixture.error = true;
    let Some(team) = shell
        .slack_lists
        .scope
        .as_ref()
        .map(|scope| scope.team.clone())
    else {
        return false;
    };
    let Some(generation) = shell
        .core
        .workspaces
        .get(&team)
        .map(|workspace| workspace.rt_generation)
    else {
        return false;
    };
    let event = RtEvent::Unknown(
        serde_json::from_value(json!({
            "type":"list_cells_updated","list_id":"F_LIST_FIXTURE",
            "cells":[{"row_id":row_id,"column_id":"C_NUMBER","key":"estimate","number":[number]}]
        }))
        .expect("fixture event"),
    );
    events::prepare(shell, &team, generation, &event)
}
