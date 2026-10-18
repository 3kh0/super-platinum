//! Exact forms and independently specified schema/record deltas for the controlled seed.
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
        || before["title"] != "Board card positions"
        || before["list_metadata"]["default_view"] != "V_LANE"
    {
        return Err("Wrong card seed".into());
    }
    Ok([before.clone(), before.clone()])
}
pub(super) fn populations(before: &Value) -> Result<[Value; 2], String> {
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
    if after["records"][2]["id"] != "R_LIST_002" {
        return Err("Wrong moved record".into());
    }
    after["records"][2]["view_positions"]["V_LANE"] = json!("0000000001.V");
    Ok([before.clone(), after])
}

pub(super) fn requests() -> Vec<Value> {
    let file = json!({"method":"POST","path":"/api/files.info","fields":[["token","fixture-not-a-token"],["file","F_LIST_FIXTURE"]]});
    let team = json!({"method":"POST","path":"/api/team.prefs.get","fields":[["token","fixture-not-a-token"],["target_team","T1"],["_x_reason","native-list-permission"]]});
    let user = json!({"method":"POST","path":"/api/users.prefs.get","fields":[["token","fixture-not-a-token"],["prefs","task_lists,assigned_to_list"],["_x_reason","native-list-completion-permission"]]});
    let page = json!({"method":"POST","path":"/api/lists.records.list","fields":[["token","fixture-not-a-token"],["list_id","F_LIST_FIXTURE"],["include_subtasks","true"],["archived","false"],["include_suggested","false"],["_x_reason","get-list-records"]]});
    let read = || vec![file.clone(), team.clone(), user.clone(), page.clone()];
    let mut out = read();
    out.push(json!({"method":"POST","path":"/api/lists.records.reorder","fields":[["token","fixture-not-a-token"],["list_id","F_LIST_FIXTURE"],["id","R_LIST_002"],["view_id","V_LANE"],["position","0000000001.V"],["_x_reason","reorder-record"]]}));
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
    #[test]
    fn exact_card_write_is_between_four_read_preflight_and_current_page_checks() {
        let r = requests();
        assert_eq!(r.len(), 14);
        assert_eq!(r[4]["path"], "/api/lists.records.reorder");
        assert_eq!(r[9]["path"], "/api/files.info");
        assert_eq!(
            r.iter()
                .filter(|v| v["path"] == "/api/lists.records.list")
                .count(),
            3
        );
        let docs = [json!({"same":true}), json!({"same":true})];
        let pages = [json!({"page":0}), json!({"page":1})];
        let done = json!([{"serial":3,"kind":"write","applied":false},{"serial":3,"kind":"check","applied":true},{"serial":3,"kind":"check","applied":false},{"serial":3,"kind":"check","applied":true}]);
        let mut good = json!({"requests":r,"responses":responses(&docs,&pages),"finished":done,"server":docs[0],"writePending":false,"readPending":false});
        exact(&good, 14, 4, false, false, 0, &docs, &pages, 3).unwrap();
        good["responses"][13] = pages[0].clone();
        assert!(exact(&good, 14, 4, false, false, 0, &docs, &pages, 3).is_err());
    }
}
