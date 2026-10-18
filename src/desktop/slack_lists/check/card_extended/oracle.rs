//! Exact forms and independently specified schema/record deltas for the controlled seed.
use super::adapter::{Case, case as adapter_case};
use serde_json::{Value, json};
pub(super) const STAGES: [&str; 7] = [
    "baseline",
    "saving",
    "uncertain",
    "conflict",
    "retired",
    "observed",
    "board",
];
pub(super) fn views() -> Value {
    json!([
        ["V_PRECISION", "Estimates"],
        ["V_LANE", "Delivery lanes"],
        ["V_PRECISION_BOARD", "Estimate board"]
    ])
}
pub(super) fn documents(before: &Value) -> Result<[Value; 2], String> {
    if before["id"] != "F_LIST_FIXTURE"
        || adapter_case().is_none_or(|m| before["title"] != m.title())
        || before["list_metadata"]["default_view"] != "V_LANE"
    {
        return Err("Wrong card seed".into());
    }
    Ok([before.clone(), before.clone()])
}
pub(super) fn populations(before: &Value) -> Result<[Value; 2], String> {
    populations_for(before, adapter_case().ok_or("Missing exact case")?)
}
fn populations_for(before: &Value, mode: Case) -> Result<[Value; 2], String> {
    if before["ok"] != true
        || before["records"].as_array().map(Vec::len) != Some(73)
        || before["response_metadata"]["next_cursor"] != "precision-opaque+/="
    {
        return Err("Wrong original card page".into());
    }
    for (i, row) in before["records"].as_array().unwrap().iter().enumerate() {
        if row["position"] != format!("{:010}", i + 1)
            || row["view_positions"] != json!({"V_OTHER":"0000000009.V"})
        {
            return Err("Wrong card raw positions".into());
        }
    }
    let mut after = before.clone();
    let row = after["records"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|r| r["id"] == mode.row())
        .ok_or("Missing moved row")?;
    row["view_positions"]["V_LANE"] = json!(mode.position());
    if mode == Case::Cross {
        row["fields"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|f| f["key"] == "status")
            .ok_or("Missing grouping cell")?["select"] = json!(["OPT_DONE"]);
    }
    Ok([before.clone(), after])
}

pub(super) fn requests() -> Vec<Value> {
    requests_for(adapter_case().expect("Exact fixture case"))
}
fn requests_for(mode: Case) -> Vec<Value> {
    let file = json!({"method":"POST","path":"/api/files.info","fields":[["token","fixture-not-a-token"],["file","F_LIST_FIXTURE"]]});
    let team = json!({"method":"POST","path":"/api/team.prefs.get","fields":[["token","fixture-not-a-token"],["target_team","T1"],["_x_reason","native-list-permission"]]});
    let user = json!({"method":"POST","path":"/api/users.prefs.get","fields":[["token","fixture-not-a-token"],["prefs","task_lists,assigned_to_list"],["_x_reason","native-list-completion-permission"]]});
    let page = json!({"method":"POST","path":"/api/lists.records.list","fields":[["token","fixture-not-a-token"],["list_id","F_LIST_FIXTURE"],["include_subtasks","true"],["archived","false"],["include_suggested","false"],["_x_reason","get-list-records"]]});
    let read = || vec![file.clone(), team.clone(), user.clone(), page.clone()];
    let mut out = read();
    let mut fields = vec![
        json!(["token", "fixture-not-a-token"]),
        json!(["list_id", "F_LIST_FIXTURE"]),
        json!(["id", mode.row()]),
        json!(["view_id", "V_LANE"]),
        json!(["position", mode.position()]),
    ];
    if let Some(cells) = mode.cells() {
        fields.push(json!(["cells", cells.to_string()]));
    }
    fields.push(json!(["_x_reason", "reorder-record"]));
    out.push(json!({"method":"POST","path":"/api/lists.records.reorder","fields":fields}));
    out.extend(read());
    out.push(file.clone());
    out.extend(read());
    out
}
pub(super) fn responses(docs: &[Value; 2], pages: &[Value; 2]) -> Vec<Value> {
    let file = |d: usize| json!({"ok":true,"file":docs[d]});
    let team = json!({"ok":true,"prefs":{"allow_lists":"full"}});
    let user = json!({"ok":true,"prefs":{"task_lists":"[]","assigned_to_list":""}});
    let read = |d, p: usize| vec![file(d), team.clone(), user.clone(), pages[p].clone()];
    let mut out = read(0, 0);
    out.push(json!({"syntheticFailure":"response_lost"}));
    out.extend(read(0, 0));
    out.push(file(0));
    out.extend(read(0, 1));
    out
}
pub(super) fn exact(
    l: &Value,
    count: usize,
    finished: usize,
    pending: bool,
    read: bool,
    server: usize,
    docs: &[Value; 2],
    pages: &[Value; 2],
    serial: u64,
) -> Result<(), String> {
    if count > 14 || finished > 4 || server > 1 || serial == 0 && count != 0 {
        return Err("Invalid card move checkpoint".into());
    }
    let req = requests();
    let res = responses(docs, pages);
    let done = json!([{ "serial":serial,"kind":"write","applied":false },{ "serial":serial,"kind":"check","applied":true },{ "serial":serial,"kind":"check","applied":false },{ "serial":serial,"kind":"check","applied":true }]);
    if l != &json!({"requests":&req[..count],"responses":&res[..count],"finished":&done.as_array().unwrap()[..finished],"server":docs[server],"writePending":pending,"readPending":read})
    {
        return Err("Exact card move requests/responses/completions differ".into());
    }
    Ok(())
}
pub(super) fn stage(
    l: &Value,
    index: usize,
    docs: &[Value; 2],
    pages: &[Value; 2],
    serial: u64,
) -> Result<(), String> {
    let &(n, f, s) = [
        (0, 0, 0),
        (5, 0, 0),
        (5, 1, 0),
        (9, 2, 0),
        (10, 3, 0),
        (14, 4, 0),
        (14, 4, 0),
    ]
    .get(index)
    .ok_or("Unknown card move stage")?;
    exact(l, n, f, index == 1, false, s, docs, pages, serial)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn seed(mode: Case) -> Value {
        json!({"ok":true,"records":(0..73).map(|i|json!({
            "id":format!("R_LIST_{i:03}"),"position":format!("{:010}",i+1),
            "view_positions":{"V_OTHER":"0000000009.V"},"opaque":{"keep":[null,1,true]},
            "fields":[{"key":"status","select":[if mode==Case::Cross && matches!(i,4|5){"OPT_DONE"}else{"OPT_PROGRESS"}],"opaque":"retained"}]
        })).collect::<Vec<_>>(),"response_metadata":{"next_cursor":"precision-opaque+/=","keep":true}})
    }
    #[test]
    fn cross_expected_page_changes_only_target_override_and_group_cell() {
        let before = seed(Case::Cross);
        let [original, after] = populations_for(&before, Case::Cross).unwrap();
        let mut expected = before.clone();
        expected["records"][2]["view_positions"]["V_LANE"] = json!("0000000005.V");
        expected["records"][2]["fields"][0]["select"] = json!(["OPT_DONE"]);
        assert_eq!(original, before);
        assert_eq!(after, expected);
    }
    #[test]
    fn terminal_loaded_tail_changes_one_override_and_preserves_cursor_and_raw_array() {
        let before = seed(Case::Tail);
        let [original, after] = populations_for(&before, Case::Tail).unwrap();
        let mut expected = before.clone();
        expected["records"][70]["view_positions"]["V_LANE"] = json!("0000000074");
        assert_eq!(original, before);
        assert_eq!(after, expected);
    }
    #[test]
    fn malformed_seed_positions_and_cursor_are_not_normalized() {
        for mode in [Case::Cross, Case::Tail] {
            let mut raw = seed(mode);
            raw["records"][0]["position"] = json!("1");
            assert!(populations_for(&raw, mode).is_err());
            let mut raw = seed(mode);
            raw["response_metadata"]["next_cursor"] = json!("");
            assert!(populations_for(&raw, mode).is_err());
            let mut raw = seed(mode);
            raw["records"][0]["view_positions"]["extra"] = json!("1");
            assert!(populations_for(&raw, mode).is_err());
        }
    }
    #[test]
    fn exact_combined_request_and_read_schedule_have_no_extra_group_write() {
        for mode in [Case::Cross, Case::Tail] {
            let r = requests_for(mode);
            assert_eq!(r.len(), 14);
            assert_eq!(
                r.iter()
                    .filter(|r| r["path"] == "/api/lists.records.reorder")
                    .count(),
                1
            );
            assert_eq!(r[4]["fields"][2], json!(["id", mode.row()]));
            assert_eq!(r[4]["fields"][4], json!(["position", mode.position()]));
            if mode == Case::Cross {
                assert_eq!(r[4]["fields"][5][0], "cells");
                assert_eq!(
                    serde_json::from_str::<Value>(r[4]["fields"][5][1].as_str().unwrap()).unwrap(),
                    mode.cells().unwrap()
                );
                assert_eq!(r[4]["fields"].as_array().unwrap().len(), 7);
            } else {
                assert_eq!(r[4]["fields"].as_array().unwrap().len(), 6);
            }
            assert_eq!(r[9]["path"], "/api/files.info");
            assert_eq!(r[13]["path"], "/api/lists.records.list");
        }
    }
}
