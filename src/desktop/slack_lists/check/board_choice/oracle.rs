//! Exact forms and independently specified schema/record deltas for the controlled seed.
use serde_json::{Value, json};
pub(super) const STAGES: [&str; 7] = [
    "edit",
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
pub(super) fn options() -> Value {
    json!({"choices":[{"value":"OPT_PROGRESS","label":"In motion","color":"blue"},{"value":"OPT_DONE","label":"Done","color":"green"}]})
}
pub(super) fn documents(before: &Value) -> Result<[Value; 2], String> {
    if before["id"] != "F_LIST_FIXTURE"
        || before["title"] != "Board group options"
        || before["list_metadata"]["default_view"] != "V_LANE"
        || before["list_metadata"]["views"].as_array().map(Vec::len) != Some(3)
        || before["list_metadata"]["views"][1]["id"] != "V_LANE"
    {
        return Err("Wrong Board choice seed".into());
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
    Ok([before.clone(), before.clone()])
}

pub(super) fn requests() -> Vec<Value> {
    let file = json!({"method":"POST","path":"/api/files.info","fields":[["token","fixture-not-a-token"],["file","F_LIST_FIXTURE"]]});
    let team = json!({"method":"POST","path":"/api/team.prefs.get","fields":[["token","fixture-not-a-token"],["target_team","T1"],["_x_reason","native-list-permission"]]});
    let user = json!({"method":"POST","path":"/api/users.prefs.get","fields":[["token","fixture-not-a-token"],["prefs","task_lists,assigned_to_list"],["_x_reason","native-list-completion-permission"]]});
    let page = json!({"method":"POST","path":"/api/lists.records.list","fields":[["token","fixture-not-a-token"],["list_id","F_LIST_FIXTURE"],["include_subtasks","true"],["archived","false"],["include_suggested","false"],["_x_reason","get-list-records"]]});
    let read = || vec![file.clone(), team.clone(), user.clone(), page.clone()];
    let mut out = vec![file.clone(), team.clone(), user.clone()];
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
    let mut out = vec![file(0), team.clone(), user.clone()];
    out.push(json!({"syntheticFailure":"response_lost"}));
    out.extend(read(0, 0));
    out.push(file(1));
    out.extend(read(1, 0));
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
    if count > 13 || finished > 4 || server > 1 || serial == 0 && count != 0 {
        return Err("Invalid choice checkpoint".into());
    }
    let req = requests();
    let res = responses(docs, pages);
    let done = json!([{ "serial":serial,"kind":"write","applied":false },{ "serial":serial,"kind":"check","applied":true },{ "serial":serial,"kind":"check","applied":false },{ "serial":serial,"kind":"check","applied":true }]);
    if l != &json!({"requests":&req[..count],"responses":&res[..count],"finished":&done.as_array().unwrap()[..finished],"server":docs[server],"writePending":pending,"readPending":read})
    {
        return Err("Exact choice requests/responses/completions differ".into());
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
        (4, 0, 0),
        (4, 1, 1),
        (8, 2, 1),
        (9, 3, 1),
        (13, 4, 1),
        (13, 4, 1),
    ]
    .get(index)
    .ok_or("Unknown choice stage")?;
    exact(l, n, f, index == 1, false, s, docs, pages, serial)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_single_choice_write_and_three_owned_read_rounds() {
        let r = requests();
        assert_eq!(r.len(), 13);
        assert_eq!(r[3]["path"], "/api/lists.columns.update");
        assert_eq!(r[8]["path"], "/api/files.info");
        assert_eq!(
            r.iter()
                .filter(|r| r["path"] == "/api/lists.records.list")
                .count(),
            2
        );
        let docs = [json!({"opaque":0}), json!({"opaque":1})];
        let pages = [json!({"page":0}), json!({"page":0})];
        let done = json!([{"serial":17,"kind":"write","applied":false},{"serial":17,"kind":"check","applied":true},{"serial":17,"kind":"check","applied":false},{"serial":17,"kind":"check","applied":true}]);
        let good = json!({"requests":r,"responses":responses(&docs,&pages),"finished":done,"server":docs[1],"writePending":false,"readPending":false});
        exact(&good, 13, 4, false, false, 1, &docs, &pages, 17).unwrap();
        let mut bad = good;
        bad["finished"][2]["applied"] = json!(true);
        assert!(exact(&bad, 13, 4, false, false, 1, &docs, &pages, 17).is_err());
    }
}
