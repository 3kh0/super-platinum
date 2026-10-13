//! Board-only backend changes, restored after the disconnected native checks.
use super::*;

pub(crate) struct Snapshot {
    document: Value,
    records: Value,
    experiments: api::ListExperiments,
}

pub(crate) fn add_board_views(shell: &mut ShellState) -> Option<Snapshot> {
    if !enabled(shell) {
        return None;
    }
    let fixture = shell.slack_lists.fixture.as_mut()?;
    let snapshot = Snapshot {
        document: fixture.document.clone(),
        records: fixture.records.clone(),
        experiments: fixture.experiments,
    };
    let schema = fixture.document["list_metadata"]["schema"].as_array_mut()?;
    let teams = schema.iter_mut().find(|column| column["id"] == "C_MULTI")?;
    teams["options"]["choices"] = json!([
        {"value":"OPT_DESIGN","label":"Design"},
        {"value":"OPT_ENGINEERING","label":"Engineering"},
        {"value":"OPT_OPERATIONS","label":"Operations"}
    ]);
    schema.push(json!({"id":"C_BOARD_LANES","key":"board_lanes","name":"Extra lanes","type":"select","options":{"choices":[
        {"value":"LANE_1","label":"Lane one"},{"value":"LANE_2","label":"Lane two"},
        {"value":"LANE_3","label":"Lane three"},{"value":"LANE_4","label":"Lane four"},
        {"value":"LANE_5","label":"Lane five"}
    ]}}));
    schema.push(
        json!({"id":"C_BOARD_EMPTY","key":"board_empty","name":"Optional note","type":"text"}),
    );
    // The primary is deliberately not first. Board promotes it; grouping is
    // included in saved columns but omitted from actual card fields.
    let columns = json!([
        {"id":"C_NUMBER","key":"estimate","visible":true},
        {"id":"C_MULTI","key":"teams","visible":true},
        {"id":"C_NAME","key":"NAME","visible":true},
        {"id":"C_CHECK","key":"done","visible":true},
        {"id":"C_SELECT","key":"status","visible":false}
    ]);
    let grouping = json!({"group_by":"teams","group_by_column_id":"C_MULTI","order":[
        {"select":["OPT_ENGINEERING"]},{"select":["OPT_DESIGN"]},{"select":["OPT_OPERATIONS"]},{"select":[]}
    ]});
    let views = fixture.document["list_metadata"]["views"].as_array_mut()?;
    views.extend([
        json!({"id":"V_BOARD_MULTI","name":"Teams board","type":"board","position":"201","columns":columns,"filters":[],"sorts":[],"grouping":grouping}),
        json!({"id":"V_BOARD_UNGROUPED","name":"All cards","type":"board","position":"202","columns":columns,"filters":[],"sorts":[]}),
        json!({"id":"V_BOARD_CHECK","name":"Completion board","type":"board","position":"203","columns":columns,"filters":[],"sorts":[],"grouping":{"group_by":"done","group_by_column_id":"C_CHECK","order":[{"checkbox":true},{"checkbox":false}]}}),
        json!({"id":"V_BOARD_FILTERED","name":"High estimates board","type":"board","position":"204","columns":columns,"filters":[{"column_id":"C_NUMBER","key":"estimate","operator":"greater_than_or_equal","values":[68]}],"sorts":[],"grouping":grouping}),
        json!({"id":"V_BOARD_TEAM_FILTER","name":"Engineering board","type":"board","position":"205","columns":columns,"filters":[{"column_id":"C_MULTI","key":"teams","operator":"contains","values":["OPT_ENGINEERING"]}],"sorts":[],"grouping":grouping}),
        json!({"id":"V_BOARD_MANY","name":"Many lanes","type":"board","position":"206","columns":columns,"filters":[],"sorts":[],"grouping":{"group_by":"board_lanes","group_by_column_id":"C_BOARD_LANES"}}),
        json!({"id":"V_BOARD_EMPTY_CARD","name":"Empty card fields","type":"board","position":"208","columns":[{"id":"C_NAME","key":"NAME","visible":false},{"id":"C_BOARD_EMPTY","key":"board_empty","visible":true}],"filters":[{"key":"estimate","column_id":"C_NUMBER","operator":"greater_than_or_equal","values":[72]}],"sorts":[]}),
    ]);
    for record in fixture.records["records"].as_array_mut()? {
        let id = record["id"].as_str().unwrap().to_owned();
        let fields = record["fields"].as_array_mut().unwrap();
        for field in fields {
            if field["key"] == "teams" {
                field["select"] = match id.as_str() {
                    "R_LIST_002" => json!(["OPT_DESIGN", "OPT_ENGINEERING"]),
                    "R_LIST_004" => json!(["OPT_DESIGN"]),
                    "R_LIST_006" => json!(["OPT_ENGINEERING"]),
                    // Deleted selections join Ungrouped, not an invented lane.
                    "R_LIST_008" => json!(["OPT_DELETED"]),
                    _ => json!([]),
                };
            }
            if field["key"] == "done" {
                field["checkbox"] = json!(matches!(id.as_str(), "R_LIST_002" | "R_LIST_004"));
            }
        }
    }
    refresh(shell);
    Some(snapshot)
}

pub(crate) fn enable_board_tasks(shell: &mut ShellState) {
    if !enabled(shell) {
        return;
    }
    let fixture = shell.slack_lists.fixture.as_mut().unwrap();
    fixture.experiments.grid_todos = true;
    fixture.document["list_metadata"]["todo_mode"] = json!(true);
    fixture.document["list_metadata"]["schema"].as_array_mut().unwrap().push(
        json!({"id":"C_TODO","key":"todo_completed","name":"Completed","type":"todo_completed"})
    );
    fixture.document["list_metadata"]["views"].as_array_mut().unwrap().push(json!({
        "id":"V_BOARD_TASKS","name":"Task completion board","type":"board","position":"207",
        "show_completed_items":true,
        "columns":[{"id":"C_NUMBER","key":"estimate","visible":true},{"id":"C_NAME","key":"NAME","visible":true},{"id":"C_TODO","key":"todo_completed","visible":true}],
        "filters":[],"sorts":[],"grouping":{"group_by":"todo_completed","group_by_column_id":"C_TODO","order":[{"checkbox":false},{"checkbox":true}]}
    }));
    for record in fixture.records["records"].as_array_mut().unwrap() {
        record["fields"]
            .as_array_mut()
            .unwrap()
            .push(json!({"key":"todo_completed","column_id":"C_TODO","checkbox":false}));
    }
    refresh(shell);
}

pub(crate) fn restore_board_views(shell: &mut ShellState, snapshot: Snapshot) {
    if !enabled(shell) {
        return;
    }
    let fixture = shell.slack_lists.fixture.as_mut().unwrap();
    fixture.document = snapshot.document;
    fixture.records = snapshot.records;
    fixture.experiments = snapshot.experiments;
    refresh(shell);
}
