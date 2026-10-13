//! Deterministic date scenarios appended after the ordinary interaction checks.
use super::*;
use super_platinum_core::slack_lists::saved_views::DateContext;

pub(crate) fn set_date_clock(shell: &mut ShellState, now: &str, prior_read: &str) {
    let context = DateContext::from_local_with_offset(now, Some(prior_read), -4 * 3600)
        .expect("fixture local calendar");
    assert!(super::super::date_context::set_override(shell, context));
}

pub(crate) fn add_date_views(shell: &mut ShellState) {
    if !enabled(shell) {
        return;
    }
    let fixture = shell.slack_lists.fixture.as_mut().unwrap();
    fixture.document["list_metadata"]["schema"]
        .as_array_mut()
        .unwrap()
        .extend([
            json!({"id":"C_DATE","key":"date","name":"Target date","type":"date"}),
            json!({"id":"C_STAMP","key":"edited","name":"Changed at","type":"last_edited_time"}),
        ]);
    let columns = json!([
        {"id":"C_NAME","key":"NAME","visible":true},
        {"id":"C_DATE","key":"date","visible":true},
        {"id":"C_STAMP","key":"edited","visible":true}
    ]);
    let filter = |key: &str, column: &str, operator: &str, values: Value| json!({"key":key,"column_id":column,"operator":operator,"typed_values":[],"values":values});
    let mut view = |id: &str, name: &str, predicate: Value, sorts: Value| {
        let views = fixture.document["list_metadata"]["views"]
            .as_array_mut()
            .unwrap();
        views.push(
            json!({"id":id,"name":name,"type":"table","position":(100+views.len()).to_string(),
            "columns":columns,"filters":[predicate],"sorts":sorts}),
        );
    };
    view(
        "V_DATE_TODAY",
        "Due today",
        filter("date", "C_DATE", "equal", json!(["today"])),
        json!([]),
    );
    view(
        "V_DATE_WEEK",
        "This week, latest first",
        filter("date", "C_DATE", "equal", json!(["this_week"])),
        json!([{"key":"date","column_id":"C_DATE","ascending":false}]),
    );
    view(
        "V_DATE_LATER",
        "Later",
        filter("date", "C_DATE", "equal", json!(["later"])),
        json!([]),
    );
    view(
        "V_DATE_NEGATED",
        "Not today or tomorrow",
        filter("date", "C_DATE", "not_equal", json!(["today", "tomorrow"])),
        json!([{"key":"date","column_id":"C_DATE","ascending":true}]),
    );
    view(
        "V_DATE_SORT",
        "Dated items",
        filter("date", "C_DATE", "is_not_empty", json!([])),
        json!([{"key":"date","column_id":"C_DATE","ascending":true}]),
    );
    view(
        "V_DATE_SINCE",
        "Changed since last viewed",
        filter("edited", "C_STAMP", "equal", json!(["since_i_last_viewed"])),
        json!([{"key":"edited","column_id":"C_STAMP","ascending":true}]),
    );
    for record in fixture.records["records"].as_array_mut().unwrap() {
        let id = record["id"].as_str().unwrap().to_owned();
        let date = match id.as_str() {
            "R_LIST_000" => Some("2026-10-13"),
            "R_LIST_001" => Some("2026-10-14"),
            "R_LIST_002" => Some("2026-10-11"),
            "R_LIST_003" => Some("2026-10-12"),
            "R_LIST_004" => Some("2026-10-19"),
            _ => None,
        };
        let fields = record["fields"].as_array_mut().unwrap();
        fields.retain(|field| field["key"] != "date" && field["key"] != "edited");
        if let Some(date) = date {
            // Exact live wire family: -1 says the calendar date has no time.
            fields.push(json!({"key":"date","column_id":"C_DATE","date":[date],"timestamp":[-1],"value":date,"text":date}));
        } else if id == "R_LIST_005" {
            fields.push(json!({"key":"date","column_id":"C_DATE","timestamp":[0],"text":"Later"}));
        }
        let stamp = match id.as_str() {
            "R_LIST_000" => Some(1_791_820_800u64), // prior-read equality
            "R_LIST_001" => Some(1_791_820_801),    // one second later
            "R_LIST_002" => Some(1_791_864_000),    // next local midnight
            _ => None,
        };
        if let Some(stamp) = stamp {
            fields.push(json!({"key":"edited","column_id":"C_STAMP","timestamp":[stamp],"text":stamp.to_string()}));
        }
    }
    refresh(shell);
    set_date_clock(shell, "2026-10-13T12:00:00", "2026-10-12T12:00:00");
}

pub(crate) fn remove_date_views(shell: &mut ShellState) {
    if !enabled(shell) {
        return;
    }
    let fixture = shell.slack_lists.fixture.as_mut().unwrap();
    fixture.document["list_metadata"]["schema"]
        .as_array_mut()
        .unwrap()
        .retain(|column| column["id"] != "C_DATE" && column["id"] != "C_STAMP");
    fixture.document["list_metadata"]["views"]
        .as_array_mut()
        .unwrap()
        .retain(|view| !view["id"].as_str().unwrap_or("").starts_with("V_DATE_"));
    for record in fixture.records["records"].as_array_mut().unwrap() {
        record["fields"]
            .as_array_mut()
            .unwrap()
            .retain(|field| field["key"] != "date" && field["key"] != "edited");
    }
    refresh(shell);
}
