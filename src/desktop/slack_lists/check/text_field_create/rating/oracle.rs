//! Literal cell request and readback oracle independent of the fixture hooks.
use serde_json::{Value, json};
pub(crate) const STAGES: [&str; 20] = [
    "rating-edit",
    "rating-saving",
    "rating-acknowledged",
    "rating-observed",
    "rating-board",
    "rating-detail",
    "same-clear-edit",
    "same-clear-saving",
    "same-clear-acknowledged",
    "same-cleared",
    "empty-board",
    "empty-detail",
    "reset-edit",
    "reset-saving",
    "reset-acknowledged",
    "reset-observed",
    "clear-edit",
    "clear-saving",
    "clear-acknowledged",
    "cleared",
];
pub(super) const COUNTS: [(usize, usize, usize); 20] = [
    (0, 0, 0),
    (1, 0, 0),
    (1, 1, 0),
    (1, 1, 1),
    (1, 1, 1),
    (1, 1, 1),
    (1, 1, 1),
    (2, 1, 1),
    (2, 2, 1),
    (2, 2, 2),
    (2, 2, 2),
    (2, 2, 2),
    (2, 2, 2),
    (3, 2, 2),
    (3, 3, 2),
    (3, 3, 3),
    (3, 3, 3),
    (4, 3, 3),
    (4, 4, 3),
    (4, 4, 4),
];
pub(super) const LOADED: [usize; 20] = [0, 0, 0, 1, 1, 1, 1, 1, 1, 2, 2, 2, 2, 2, 2, 3, 3, 3, 3, 4];
pub(super) const SERVER: [usize; 20] = [0, 0, 1, 1, 1, 1, 1, 1, 2, 2, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4];
pub(super) const NAVIGATIONS: [usize; 20] =
    [0, 0, 0, 0, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 3, 3, 4, 4, 4, 4];
pub(super) const OPENS: [usize; 20] = [0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 2, 2, 2, 2, 2, 2, 2, 2, 2];
pub(super) fn pages(original: &Value, value: u8) -> Result<[Value; 5], String> {
    let raw = original["records"]
        .as_array()
        .ok_or("Missing raw records")?;
    if raw.len() != 73
        || raw.iter().filter(|r| r["id"] == "R_LIST_003").count() != 1
        || raw.iter().any(|r| {
            r["fields"].as_array().is_none_or(|f| {
                f.iter()
                    .any(|v| v["key"] == "rating_observed" || v["column_id"] == "C_RATING_OBSERVED")
            })
        })
    {
        return Err("Original Rating data not wholly absent".into());
    }
    let mut yes = original.clone();
    let row = yes["records"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|r| r["id"] == "R_LIST_003")
        .unwrap();
    row["fields"]
        .as_array_mut()
        .unwrap()
        .push(json!({"key":"rating_observed","rating":[value]}));
    let mut no = yes.clone();
    let row = no["records"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|r| r["id"] == "R_LIST_003")
        .unwrap();
    row["fields"].as_array_mut().unwrap().last_mut().unwrap()["rating"] = json!([]);
    Ok([original.clone(), yes.clone(), no.clone(), yes, no])
}
pub(super) fn exact(
    l: &Value,
    stage: usize,
    document: &Value,
    pages: &[Value; 5],
    sync: &Value,
    value: u8,
) -> Result<(), String> {
    let &(writes, done, reads) = COUNTS.get(stage).ok_or("Unknown cell stage")?;
    let pre = l["preflights"]
        .as_array()
        .ok_or("Missing local preflights")?;
    let requests = l["requests"]
        .as_array()
        .ok_or("Missing actual cell requests")?;
    let finished = l["completed"]
        .as_array()
        .ok_or("Missing cell completions")?;
    let local = l["reads"].as_array().ok_or("Missing local reads")?;
    if pre.len() != writes
        || requests.len() != writes
        || finished.len() != done
        || local.len() != reads
        || l["sync"] != *sync
        || l["attemptCount"] != writes
        || l["preparedValid"] != true
        || l["server"] != pages[SERVER[stage]]
    {
        return Err("Cell ledger count/server/sync drift".into());
    }
    let nav = l["navigations"]
        .as_array()
        .ok_or("Missing navigation observations")?;
    let opens = l["recordOpens"]
        .as_array()
        .ok_or("Missing actual record-open forms")?;
    if nav.len() != NAVIGATIONS[stage] || opens.len() != OPENS[stage] {
        return Err("Unexpected view/detail lifecycle count".into());
    }
    let destinations = ["V_RATING_BOARD", "V_FILTER", "V_RATING_BOARD", "V_FILTER"];
    let read_counts = [1, 1, 2, 3];
    let mut selection = sync["selection"].clone();
    for (i, n) in nav.iter().enumerate() {
        if n["fromSelection"] != selection
            || n["selection"] == selection
            || !n["selection"].as_u64().is_some_and(|v| v > 0)
            || n["view"] != destinations[i]
            || n["reads"] != read_counts[i]
            || n["find"]["input"] != "Launch item 0"
            || n["find"]["applied"] != "Launch item 0"
            || !n["find"]["revision"].as_u64().is_some_and(|v| v > 0)
        {
            return Err("Navigation/Find custody drift".into());
        }
        selection = n["selection"].clone();
    }
    if l["selection"] != selection {
        return Err("Current diagnostic selection differs".into());
    }
    for (i, o) in opens.iter().enumerate() {
        if !o["generation"].as_u64().is_some_and(|v| v > 0)
            || (i > 0 && o["generation"] == opens[0]["generation"])
            || o["method"] != "POST"
            || o["path"] != "/api/lists.records.open"
            || o["fields"]
                != json!([
                    ["token", "fixture-not-a-token"],
                    ["list_id", "F_LIST_FIXTURE"],
                    ["id", "R_LIST_003"],
                    ["_x_reason", "open-record"]
                ])
            || o["retrySafe"] != false
        {
            return Err("Actual detail mount request differs".into());
        }
    }
    for (i, p) in pre.iter().enumerate() {
        let a = &p["attempt"];
        if !a["serial"].as_u64().is_some_and(|n| n > 0)
            || !a["revision"].as_u64().is_some_and(|n| n > 0)
            || a["visit"] != sync["visit"]
            || a["selection"]
                != if i == 0 {
                    sync["selection"].clone()
                } else {
                    nav[[0, 1, 2, 3][i]]["selection"].clone()
                }
            || a["list"] != "F_LIST_FIXTURE"
            || a["row"] != "R_LIST_003"
            || a["column"] != "C_RATING_OBSERVED"
            || a["key"] != "rating_observed"
            || a["cells"]
                != json!([{"row_id":"R_LIST_003","column_id":"C_RATING_OBSERVED","rating":if i%2==0 {json!([value])} else {json!([])}}])
            || a["owner"] != sync["owner"]
            || a["owner"].as_array().map(Vec::len) != Some(3)
            || pre[..i]
                .iter()
                .any(|old| old["attempt"]["serial"] == a["serial"])
            || *p
                != json!({"attempt":a,"document":document,"population":pages[i],"allow":"full","phase":"Saving"})
        {
            return Err("Actual typed cell custody differs".into());
        }
        let cells =
            json!([{"row_id":"R_LIST_003","column_id":"C_RATING_OBSERVED","rating":if i%2==0 {json!([value])} else {json!([])}}])
                .to_string();
        let expected = json!({"method":"POST","path":"/api/lists.cells.update","fields":[["token","fixture-not-a-token"],["list_id","F_LIST_FIXTURE"],["cells",cells],["_x_reason","bulk-update-cells"]],"retrySafe":false,"attempt":a});
        if requests[i] != expected {
            return Err("Actual cell form/retry policy differs".into());
        }
        if i < done
            && finished[i]
                != json!({"attempt":a,"result":"positive_ack","operation":{"attempt":a,"phase":"Uncertain"}})
        {
            return Err("Cell completion mismatch".into());
        }
    }
    let unresolved = writes > reads;
    let phase = if writes > done { "Saving" } else { "Uncertain" };
    let a = pre.last().map(|p| &p["attempt"]).unwrap_or(&Value::Null);
    let expected_op = if unresolved {
        json!({"attempt":a,"phase":phase})
    } else {
        Value::Null
    };
    let pending = if writes > done {
        a.clone()
    } else {
        Value::Null
    };
    if l["operation"] != expected_op || l["pending"] != pending {
        return Err("Actual pending/retained operation differs".into());
    }
    for (i, r) in local.iter().enumerate() {
        let a = &pre[i]["attempt"];
        let before = json!({"attempt":a,"phase":"Uncertain"});
        let after = Value::Null;
        if r["readback"] != json!({"known":true,"value":if i%2==0 {Some(value)}else{None}})
            || r["visit"] != sync["visit"]
            || r["before"] != before
            || r["document"] != *document
            || r["population"] != pages[i + 1]
            || !r["read"].as_u64().is_some_and(|n| n > 0)
            || !r["revision"].as_u64().is_some_and(|n| n > 0)
            || r["after"]["read"] != 0
            || r["after"]["operation"] != after
            || !r["after"]["revision"]
                .as_u64()
                .zip(r["revision"].as_u64())
                .is_some_and(|(after, before)| after > before)
            || (i > 0
                && !r["read"]
                    .as_u64()
                    .zip(local[i - 1]["read"].as_u64())
                    .is_some_and(|(n, p)| n > p))
        {
            return Err("Local guarded read custody/application differs".into());
        }
    }
    Ok(())
}
