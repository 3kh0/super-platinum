//! Exact forms and independently specified schema/record deltas for the controlled seed.
use serde_json::{Value, json};
pub(super) const STAGES: [&str; 7] = [
    "edit",
    "saving",
    "uncertain",
    "partial",
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
pub(super) fn options() -> Value {
    json!({"choices":[{"value":"OPT_DONE","label":"Done","color":"green"}]})
}
pub(super) fn cells() -> Value {
    json!(
        (0..9)
            .map(|i| json!({"row_id":format!("R_LIST_{i:03}"),"column_id":"C_SELECT","select":[]}))
            .collect::<Vec<_>>()
    )
}
pub(super) fn documents(before: &Value) -> Result<[Value; 2], String> {
    if before["id"] != "F_LIST_FIXTURE"
        || before["title"] != "Delete Board group"
        || before["list_metadata"]["default_view"] != "V_LANE"
        || before["list_metadata"]["views"].as_array().map(Vec::len) != Some(3)
        || before["list_metadata"]["views"][1]["id"] != "V_LANE"
    {
        return Err("Wrong group deletion seed".into());
    }
    let mut after = before.clone();
    let columns = after["list_metadata"]["schema"]
        .as_array_mut()
        .ok_or("Missing schema")?;
    let c = columns
        .iter_mut()
        .find(|c| c["id"] == "C_SELECT" && c["key"] == "status")
        .ok_or("Missing grouping field")?;
    if c["options"]
        != json!({"choices":[{"value":"OPT_PROGRESS","label":"In progress","color":"blue"},{"value":"OPT_DONE","label":"Done","color":"green"}]})
    {
        return Err("Unexpected grouping options".into());
    }
    c["options"] = options();
    Ok([before.clone(), after])
}
pub(super) fn populations(before: &Value) -> Result<[Value; 2], String> {
    if before["ok"] != true
        || before["records"].as_array().map(Vec::len) != Some(73)
        || before["response_metadata"]["next_cursor"] != "precision-opaque+/="
    {
        return Err("Wrong original page".into());
    }
    let mut after = before.clone();
    for (n, row) in after["records"].as_array_mut().unwrap()[..9]
        .iter_mut()
        .enumerate()
    {
        if row["id"] != format!("R_LIST_{n:03}") || row["list_id"] != "F_LIST_FIXTURE" {
            return Err("Wrong matching record".into());
        }
        let field = row["fields"]
            .as_array_mut()
            .ok_or("Missing fields")?
            .iter_mut()
            .find(|f| f["key"] == "status")
            .ok_or("Missing status")?;
        if field["select"] != json!(["OPT_PROGRESS"]) {
            return Err("Wrong matching scalar".into());
        }
        field["select"] = json!([]);
    }
    Ok([before.clone(), after])
}
pub(super) fn requests() -> Vec<Value> {
    let file = json!({"method":"POST","path":"/api/files.info","fields":[["token","fixture-not-a-token"],["file","F_LIST_FIXTURE"]]});
    let team = json!({"method":"POST","path":"/api/team.prefs.get","fields":[["token","fixture-not-a-token"],["target_team","T1"],["_x_reason","native-list-permission"]]});
    let user = json!({"method":"POST","path":"/api/users.prefs.get","fields":[["token","fixture-not-a-token"],["prefs","task_lists,assigned_to_list"],["_x_reason","native-list-completion-permission"]]});
    let page = json!({"method":"POST","path":"/api/lists.records.list","fields":[["token","fixture-not-a-token"],["list_id","F_LIST_FIXTURE"],["include_subtasks","true"],["archived","false"],["include_suggested","false"],["_x_reason","get-list-records"]]});
    let read = || vec![file.clone(), team.clone(), user.clone(), page.clone()];
    let mut out = read();
    out.push(json!({"method":"POST","path":"/api/lists.cells.update","fields":[["token","fixture-not-a-token"],["list_id","F_LIST_FIXTURE"],["cells",cells().to_string()],["_x_reason","bulk-update-cells"]]}));
    out.push(json!({"method":"POST","path":"/api/lists.columns.update","fields":[["token","fixture-not-a-token"],["list_id","F_LIST_FIXTURE"],["key","status"],["options",options().to_string()],["backfill_missing_cells_with_default","true"],["_x_reason","update-column"]]}));
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
    out.push(json!({"syntheticFailure":"response_lost","effect":"cells"}));
    out.push(json!({"syntheticFailure":"response_lost","effect":"schema"}));
    out.extend(read(1, 0));
    out.push(file(1));
    out.extend(read(1, 1));
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
    if count > 15 || finished > 4 || server > 1 || serial == 0 && count != 0 {
        return Err("Invalid deletion checkpoint".into());
    }
    let req = requests();
    let res = responses(docs, pages);
    let done = json!([{ "serial":serial,"kind":"write","applied":false },{ "serial":serial,"kind":"check","applied":true },{ "serial":serial,"kind":"check","applied":false },{ "serial":serial,"kind":"check","applied":true }]);
    if l != &json!({"requests":&req[..count],"responses":&res[..count],"finished":&done.as_array().unwrap()[..finished],"server":docs[server],"writePending":pending,"readPending":read})
    {
        return Err("Exact two-effect requests/responses/completions differ".into());
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
        (6, 0, 0),
        (6, 1, 1),
        (10, 2, 1),
        (11, 3, 1),
        (15, 4, 1),
        (15, 4, 1),
    ]
    .get(index)
    .ok_or("Unknown deletion stage")?;
    exact(l, n, f, index == 1, false, s, docs, pages, serial)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_forms_keep_two_effects_and_only_one_retired_read() {
        let req = requests();
        assert_eq!(req.len(), 15);
        assert_eq!(req[4]["path"], "/api/lists.cells.update");
        assert_eq!(req[5]["path"], "/api/lists.columns.update");
        assert_eq!(req[10]["path"], "/api/files.info");
        assert_eq!(
            req.iter()
                .filter(|r| r["path"] == "/api/lists.records.list")
                .count(),
            3
        );
        assert_eq!(req[4]["fields"].as_array().unwrap().len(), 4);
        assert_eq!(req[5]["fields"].as_array().unwrap().len(), 6);
        let docs = [json!({"opaque":0}), json!({"opaque":1})];
        let pages = [json!({"page":0}), json!({"page":1})];
        let done = json!([{ "serial":17,"kind":"write","applied":false },{ "serial":17,"kind":"check","applied":true },{ "serial":17,"kind":"check","applied":false },{ "serial":17,"kind":"check","applied":true }]);
        let valid = json!({"requests":req,"responses":responses(&docs,&pages),"finished":done,"server":docs[1],"writePending":false,"readPending":false});
        exact(&valid, 15, 4, false, false, 1, &docs, &pages, 17).unwrap();
        for case in 0..4 {
            let mut bad = valid.clone();
            match case {
                0 => bad["finished"][2]["applied"] = json!(true),
                1 => {
                    bad["requests"]
                        .as_array_mut()
                        .unwrap()
                        .push(requests()[4].clone());
                }
                2 => bad["responses"][9] = pages[1].clone(),
                _ => bad["extra"] = json!(true),
            }
            assert!(exact(&bad, 15, 4, false, false, 1, &docs, &pages, 17).is_err());
        }
    }
}
