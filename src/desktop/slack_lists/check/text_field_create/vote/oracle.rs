//! Literal Vote forms, pages and stage ledger independent of diagnostic hooks.
use serde_json::{Value, json};
pub(super) const STAGES: [&str; 14] = [
    "vote-ready",
    "vote-saving",
    "vote-uncertain",
    "vote-different",
    "vote-retry-saving",
    "vote-acknowledged",
    "vote-observed",
    "vote-board",
    "vote-detail",
    "remove-saving",
    "remove-acknowledged",
    "removed",
    "removed-board",
    "restored-table",
];
pub(super) const COUNTS: [(usize, usize, usize); 14] = [
    (0, 0, 0),
    (1, 0, 0),
    (1, 1, 0),
    (1, 1, 1),
    (2, 1, 1),
    (2, 2, 1),
    (2, 2, 2),
    (2, 2, 2),
    (2, 2, 2),
    (3, 2, 2),
    (3, 3, 2),
    (3, 3, 3),
    (3, 3, 3),
    (3, 3, 3),
];
pub(super) const LOADED: [usize; 14] = [0, 0, 0, 2, 2, 2, 3, 3, 3, 3, 3, 4, 4, 4];
pub(super) const SERVER: [usize; 14] = [0, 0, 1, 2, 2, 3, 3, 3, 3, 3, 4, 4, 4, 4];
const NAV: [usize; 14] = [0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 2];
const OPENS: [usize; 14] = [0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 1];
fn users(index: usize) -> Value {
    match index {
        0 => json!(["U0"]),
        1 => json!(["U_OTHER_B", "U_OTHER_A", "U0"]),
        2 => json!(["U_OTHER_B", "U_OTHER_A"]),
        _ => unreachable!(),
    }
}
pub(super) fn pages(original: &Value) -> Result<[Value; 5], String> {
    let records = original["records"]
        .as_array()
        .ok_or("Missing original raw records")?;
    if records.len() != 73
        || records.iter().filter(|r| r["id"] == "R_LIST_003").count() != 1
        || records.iter().any(|r| {
            r["fields"].as_array().is_none_or(|fs| {
                fs.iter()
                    .any(|f| f["key"] == "vote_observed" || f["column_id"] == "C_VOTE_OBSERVED")
            })
        })
    {
        return Err("Vote baseline not wholly absent".into());
    }
    let changed = |v: Value| {
        let mut p = original.clone();
        p["records"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|r| r["id"] == "R_LIST_003")
            .unwrap()["fields"]
            .as_array_mut()
            .unwrap()
            .push(json!({"key":"vote_observed","user":v}));
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
fn actual_fixture_owner(sync: &Value) -> bool {
    let Some(owner) = sync["owner"].as_array() else {
        return false;
    };
    if owner.len() != 3 || owner[1] != "T1" || owner[2] != "U0" {
        return false;
    }
    let Some(digits) = owner[0]
        .as_str()
        .and_then(|s| s.strip_prefix("number-precision-fixture-"))
    else {
        return false;
    };
    let Ok(allocation) = digits.parse::<u64>() else {
        return false;
    };
    allocation > 0
        && allocation.to_string() == digits
        && sync["visit"]
            .as_u64()
            .is_some_and(|visit| allocation < visit)
}

pub(super) fn exact(
    l: &Value,
    stage: usize,
    document: &Value,
    pages: &[Value; 5],
    sync: &Value,
) -> Result<(), String> {
    let &(writes, done, reads) = COUNTS.get(stage).ok_or("Unknown Vote stage")?;
    let pre = l["preflights"].as_array().ok_or("Missing preflights")?;
    let requests = l["requests"].as_array().ok_or("Missing forms")?;
    let finished = l["completed"].as_array().ok_or("Missing completions")?;
    let local = l["reads"].as_array().ok_or("Missing reads")?;
    let nav = l["navigations"].as_array().ok_or("Missing navigations")?;
    let opens = l["recordOpens"].as_array().ok_or("Missing record opens")?;
    if pre.len() != writes
        || requests.len() != writes
        || finished.len() != done
        || local.len() != reads
        || nav.len() != NAV[stage]
        || opens.len() != OPENS[stage]
        || l["sync"] != *sync
        || l["attemptCount"] != writes
        || l["preparedValid"] != true
        || l["server"] != pages[SERVER[stage]]
        || !actual_fixture_owner(sync)
    {
        return Err("Vote ledger count/server/owner drift".into());
    }
    let mut selection = sync["selection"].clone();
    for (i, n) in nav.iter().enumerate() {
        if n["fromSelection"] != selection
            || n["selection"] == selection
            || !n["selection"].as_u64().is_some_and(|v| v > 0)
            || n["view"] != ["V_VOTE_BOARD", "V_FILTER"][i]
            || n["reads"] != [2, 3][i]
            || n["find"]["input"] != "Launch item 0"
            || n["find"]["applied"] != "Launch item 0"
            || !n["find"]["revision"].as_u64().is_some_and(|v| v > 0)
        {
            return Err("Vote navigation/Find drift".into());
        }
        selection = n["selection"].clone();
    }
    if l["selection"] != selection {
        return Err("Vote diagnostic selection differs".into());
    }
    for o in opens {
        if !o["generation"].as_u64().is_some_and(|v| v > 0)
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
            return Err("Wrong Vote record-open form".into());
        }
    }
    for (i, p) in pre.iter().enumerate() {
        let a = &p["attempt"];
        let cells = json!([{"row_id":"R_LIST_003","column_id":"C_VOTE_OBSERVED","user":users(i)}]);
        let authority = &a["authority"];
        if (i < 2 && *authority != json!({"kind":"table"}))
            || (i == 2
                && (authority["kind"] != "detail"
                    || !authority["generation"].as_u64().is_some_and(|v| v > 0)))
        {
            return Err("Wrong actual Vote authority".into());
        }
        if !a["serial"].as_u64().is_some_and(|v| v > 0)
            || !a["revision"].as_u64().is_some_and(|v| v > 0)
            || a["visit"] != sync["visit"]
            || a["selection"]
                != if i < 2 {
                    sync["selection"].clone()
                } else {
                    nav[0]["selection"].clone()
                }
            || a["list"] != "F_LIST_FIXTURE"
            || a["row"] != "R_LIST_003"
            || a["column"] != "C_VOTE_OBSERVED"
            || a["key"] != "vote_observed"
            || a["cells"] != cells
            || a["owner"] != sync["owner"]
            || (i > 0
                && !a["serial"]
                    .as_u64()
                    .zip(pre[i - 1]["attempt"]["serial"].as_u64())
                    .is_some_and(|(n, p)| n > p))
            || *p
                != json!({"attempt":a,"document":document,"population":pages[[0,2,3][i]],"allow":"full","phase":"Saving"})
        {
            return Err("Wrong typed Vote custody".into());
        }
        let expected = json!({"method":"POST","path":"/api/lists.cells.update","fields":[["token","fixture-not-a-token"],["list_id","F_LIST_FIXTURE"],["cells",cells.to_string()],["_x_reason","bulk-update-cells"]],"retrySafe":false,"attempt":a});
        if requests[i] != expected {
            return Err("Wrong Vote form/retry policy".into());
        }
        if i < done
            && finished[i]
                != json!({"attempt":a,"result":if i==0{"response_lost"}else{"positive_ack"},"operation":{"attempt":a,"phase":"Uncertain"}})
        {
            return Err("Wrong Vote completion".into());
        }
    }
    let a = pre.last().map(|p| &p["attempt"]).unwrap_or(&Value::Null);
    let op = if writes > reads {
        json!({"attempt":a,"phase":if writes>done{"Saving"}else{"Uncertain"}})
    } else {
        Value::Null
    };
    if l["operation"] != op
        || l["pending"]
            != if writes > done {
                a.clone()
            } else {
                Value::Null
            }
    {
        return Err("Wrong Vote pending receipt".into());
    }
    for (i, r) in local.iter().enumerate() {
        if r["readback"] != json!({"known":true,"value":if i==1{users(1)}else{users(2)}})
            || r["visit"] != sync["visit"]
            || r["before"] != json!({"attempt":pre[i]["attempt"],"phase":"Uncertain"})
            || r["document"] != *document
            || r["population"] != pages[[2, 3, 4][i]]
            || !r["read"].as_u64().is_some_and(|v| v > 0)
            || r["after"]["read"] != 0
            || !r["after"]["operation"].is_null()
            || !r["after"]["revision"]
                .as_u64()
                .zip(r["revision"].as_u64())
                .is_some_and(|(a, b)| a > b)
            || (i > 0
                && !r["read"]
                    .as_u64()
                    .zip(local[i - 1]["read"].as_u64())
                    .is_some_and(|(a, b)| a > b))
        {
            return Err("Wrong consumed Vote readback".into());
        }
    }
    let different = if reads > 0 {
        json!({"before":pages[1],"after":pages[2],"operation":{"attempt":pre[0]["attempt"],"phase":"Uncertain"},"owner":sync["owner"],"visit":sync["visit"],"selection":sync["selection"],"synthetic":true})
    } else {
        Value::Null
    };
    if l["different"] != different {
        return Err("Undisclosed different-voter server transition".into());
    }
    Ok(())
}
