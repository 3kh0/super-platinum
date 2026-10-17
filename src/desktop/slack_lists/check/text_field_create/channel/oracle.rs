//! Literal cell request and readback oracle independent of the fixture hooks.
use serde_json::{Value, json};
pub(crate) const STAGES: [&str; 12] = [
    "channel-edit",
    "channel-saving",
    "channel-acknowledged",
    "channel-observed",
    "replace-edit",
    "replace-saving",
    "replace-acknowledged",
    "replace-observed",
    "clear-edit",
    "clear-saving",
    "clear-acknowledged",
    "cleared",
];
pub(super) const COUNTS: [(usize, usize, usize); 12] = [
    (0, 0, 0),
    (1, 0, 0),
    (1, 1, 0),
    (1, 1, 1),
    (1, 1, 1),
    (2, 1, 1),
    (2, 2, 1),
    (2, 2, 2),
    (2, 2, 2),
    (3, 2, 2),
    (3, 3, 2),
    (3, 3, 3),
];
pub(super) const LOADED: [usize; 12] = [0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3];
pub(super) const SERVER: [usize; 12] = [0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3];
pub(super) fn value(index: usize) -> Value {
    match index {
        0 => json!(["C00000001"]),
        1 => json!(["G00000002"]),
        _ => json!([]),
    }
}
fn wire(index: usize) -> Value {
    value(index)
}
pub(super) fn pages(original: &Value) -> Result<[Value; 4], String> {
    let raw = original["records"].as_array().ok_or("Missing records")?;
    if raw.len() != 73
        || raw.iter().filter(|r| r["id"] == "R_LIST_003").count() != 1
        || raw.iter().any(|r| {
            r["fields"].as_array().is_none_or(|fs| {
                fs.iter().any(|f| {
                    f["key"] == "channel_observed" || f["column_id"] == "C_CHANNEL_OBSERVED"
                })
            })
        })
    {
        return Err("Channel baseline not wholly absent".into());
    }
    let changed = |ids: Value| {
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
            .push(json!({"key":"channel_observed","channel":ids}));
        page
    };
    Ok([
        original.clone(),
        changed(json!(["C00000001"])),
        changed(json!(["G00000002"])),
        changed(json!([])),
    ])
}

pub(super) fn exact(
    l: &Value,
    stage: usize,
    document: &Value,
    pages: &[Value; 4],
    sync: &Value,
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
    for (i, p) in pre.iter().enumerate() {
        let a = &p["attempt"];
        if !a["serial"].as_u64().is_some_and(|n| n > 0)
            || !a["revision"].as_u64().is_some_and(|n| n > 0)
            || a["visit"] != sync["visit"]
            || a["selection"] != sync["selection"]
            || a["list"] != "F_LIST_FIXTURE"
            || a["row"] != "R_LIST_003"
            || a["column"] != "C_CHANNEL_OBSERVED"
            || a["key"] != "channel_observed"
            || a["cells"]
                != json!([{"row_id":"R_LIST_003","column_id":"C_CHANNEL_OBSERVED","channel":wire(i)}])
            || a["owner"] != sync["owner"]
            || a["owner"].as_array().map(Vec::len) != Some(3)
            || pre[..i].iter().any(|p| {
                a["serial"] == p["attempt"]["serial"] || a["owner"] != p["attempt"]["owner"]
            })
            || *p
                != json!({"attempt":a,"document":document,"population":pages[i],"allow":"full","phase":"Saving"})
        {
            return Err("Actual typed cell custody differs".into());
        }
        let cells =
            json!([{"row_id":"R_LIST_003","column_id":"C_CHANNEL_OBSERVED","channel":wire(i)}])
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
    let unresolved = matches!(stage % 4, 1 | 2);
    let phase = if stage % 4 == 1 {
        "Saving"
    } else {
        "Uncertain"
    };
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
        if r["readback"] != json!({"known":true,"value":value(i)})
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

pub(super) fn candidates() -> Value {
    json!([
        ["C00000001", "#alpha-public"],
        ["G00000002", "#beta-private"],
        ["C00000003", "#gamma-external"],
        ["C00000004", "#lane-04"],
        ["C00000005", "#lane-05"],
        ["C00000006", "#lane-06"],
        ["C00000007", "#lane-07"],
        ["C00000008", "#lane-08"],
        ["C00000009", "#lane-09"],
        ["C00000010", "#lane-10"],
        ["G00000011", "#record-member"],
        ["C00000012", "#zulu-external"]
    ])
}
