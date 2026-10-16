//! Literal cell request and readback oracle independent of the fixture hooks.
use serde_json::{Value, json};
pub(crate) const STAGES: [&str; 7] = [
    "select-edit",
    "select-saving",
    "select-acknowledged",
    "select-observed",
    "clear-saving",
    "clear-acknowledged",
    "cleared",
];
pub(super) const COUNTS: [(usize, usize, usize); 7] = [
    (0, 0, 0),
    (1, 0, 0),
    (1, 1, 0),
    (1, 1, 1),
    (2, 1, 1),
    (2, 2, 1),
    (2, 2, 2),
];
pub(super) const LOADED: [usize; 7] = [0, 0, 0, 1, 1, 1, 2];
pub(super) const SERVER: [usize; 7] = [0, 0, 1, 1, 1, 2, 2];
pub(super) fn pages(original: &Value, selected: &str) -> Result<Vec<Value>, String> {
    pages_for(original, &[vec![selected.into()], vec![]])
}
pub(super) fn values(options: &Value) -> Result<Vec<Vec<String>>, String> {
    if options["format"] != "multi_select" || options["choices"].as_array().map(Vec::len) != Some(8)
    {
        return Err("Wrong Multi options inventory".into());
    }
    let first = options["choices"][0]["value"]
        .as_str()
        .ok_or("Missing first option")?
        .to_owned();
    let second = options["choices"][1]["value"]
        .as_str()
        .ok_or("Missing second option")?
        .to_owned();
    if first == second {
        return Err("Duplicate choice".into());
    }
    Ok(vec![vec![second.clone(), first], vec![second], vec![]])
}
pub(super) fn pages_for(original: &Value, values: &[Vec<String>]) -> Result<Vec<Value>, String> {
    let raw = original["records"]
        .as_array()
        .ok_or("Missing raw records")?;
    if raw.len() != 73
        || raw.iter().filter(|r| r["id"] == "R_LIST_003").count() != 1
        || raw.iter().any(|r| {
            r["fields"].as_array().is_none_or(|f| {
                f.iter()
                    .any(|v| v["key"] == "select_observed" || v["column_id"] == "C_SELECT_OBSERVED")
            })
        })
    {
        return Err("Original Select data not wholly absent".into());
    }
    let mut pages = vec![original.clone()];
    for value in values {
        let mut page = original.clone();
        let row = page["records"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|r| r["id"] == "R_LIST_003")
            .unwrap();
        row["fields"]
            .as_array_mut()
            .unwrap()
            .push(json!({"key":"select_observed","select":value}));
        pages.push(page);
    }
    Ok(pages)
}
pub(crate) const MULTI_STAGES: [&str; 10] = [
    "multi-edit",
    "multi-saving",
    "multi-acknowledged",
    "multi-observed",
    "remove-saving",
    "remove-acknowledged",
    "single-observed",
    "clear-saving",
    "clear-acknowledged",
    "cleared",
];
pub(super) const MULTI_COUNTS: [(usize, usize, usize); 10] = [
    (0, 0, 0),
    (1, 0, 0),
    (1, 1, 0),
    (1, 1, 1),
    (2, 1, 1),
    (2, 2, 1),
    (2, 2, 2),
    (3, 2, 2),
    (3, 3, 2),
    (3, 3, 3),
];
pub(super) const MULTI_LOADED: [usize; 10] = [0, 0, 0, 1, 1, 1, 2, 2, 2, 3];
const MULTI_SERVER: [usize; 10] = [0, 0, 1, 1, 1, 2, 2, 2, 3, 3];
pub(super) fn exact_multi(
    l: &Value,
    stage: usize,
    document: &Value,
    pages: &[Value],
    sync: &Value,
    options: &Value,
) -> Result<(), String> {
    exact_for(
        l,
        stage,
        document,
        pages,
        sync,
        &values(options)?,
        &MULTI_COUNTS,
        &MULTI_SERVER,
    )
}

pub(super) fn exact(
    l: &Value,
    stage: usize,
    document: &Value,
    pages: &[Value],
    sync: &Value,
    selected: &str,
) -> Result<(), String> {
    exact_for(
        l,
        stage,
        document,
        pages,
        sync,
        &[vec![selected.into()], vec![]],
        &COUNTS,
        &SERVER,
    )
}
fn exact_for(
    l: &Value,
    stage: usize,
    document: &Value,
    pages: &[Value],
    sync: &Value,
    values: &[Vec<String>],
    counts: &[(usize, usize, usize)],
    server: &[usize],
) -> Result<(), String> {
    let &(writes, done, reads) = counts.get(stage).ok_or("Unknown cell stage")?;
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
        || l["server"] != pages[server[stage]]
    {
        return Err("Cell ledger count/server/sync drift".into());
    }
    for (i, p) in pre.iter().enumerate() {
        let a = &p["attempt"];
        if !a["serial"].as_u64().is_some_and(|n| n > 0)
            || !a["revision"].as_u64().is_some_and(|n| n > 0)
            || a["visit"] != sync["visit"]
            || a["selection"] != sync["selection"]
            || a["list"] != "F_LIST_FIXTURE"
            || a["row"] != "R_LIST_003"
            || a["column"] != "C_SELECT_OBSERVED"
            || a["key"] != "select_observed"
            || a["cells"]
                != json!([{"row_id":"R_LIST_003","column_id":"C_SELECT_OBSERVED","select":values[i]}])
            || a["owner"] != sync["owner"]
            || a["owner"].as_array().map(Vec::len) != Some(3)
            || (pre[..i]
                .iter()
                .any(|p| p["attempt"]["serial"] == a["serial"]))
            || *p
                != json!({"attempt":a,"document":document,"population":pages[i],"allow":"full","phase":"Saving"})
        {
            return Err("Actual typed cell custody differs".into());
        }
        let cells =
            json!([{"row_id":"R_LIST_003","column_id":"C_SELECT_OBSERVED","select":values[i]}])
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
    let unresolved = writes != reads;
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
        if r["readback"] != json!({"known":true,"value":values[i]})
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
