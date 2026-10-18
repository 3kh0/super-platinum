//! Literal request ledger and independently specified metadata removal.
use serde_json::{Value, json};
pub(super) const STAGES: [&str; 7] = [
    "edit",
    "saving",
    "uncertain",
    "conflict",
    "retired",
    "observed",
    "table",
];
pub(super) fn order(mode: usize) -> Value {
    if mode == 0 {
        json!([
            ["V_ORDER_FIRST", "Planning"],
            ["V_ORDER_SELECTED", "Delivery"],
            ["V_ORDER_LAST", "Archive"]
        ])
    } else {
        json!([
            ["V_ORDER_SELECTED", "Delivery"],
            ["V_ORDER_FIRST", "Planning"],
            ["V_ORDER_LAST", "Archive"]
        ])
    }
}
pub(super) fn documents(before: &Value) -> Result<[Value; 2], String> {
    let views = before["list_metadata"]["views"]
        .as_array()
        .ok_or("Missing views")?;
    if views.len() != 3
        || views[0]["id"] != "V_ORDER_FIRST"
        || views[1]["id"] != "V_ORDER_SELECTED"
        || views[2]["id"] != "V_ORDER_LAST"
        || views[0]["position"] != "0000000001"
        || views[1]["position"] != "0000000002"
        || views[2]["position"] != "0000000003"
        || before["list_metadata"]["default_view"] != "V_ORDER_SELECTED"
    {
        return Err("Ordering seed identity differs".into());
    }
    let mut after = before.clone();
    // Literal source-backed move result. Do not call the production key generator.
    after["list_metadata"]["views"][1]["position"] = json!("0000000000");
    Ok([before.clone(), after])
}
pub(super) fn requests() -> Vec<Value> {
    let file = json!({"method":"POST","path":"/api/files.info","fields":[["token","fixture-not-a-token"],["file","F_LIST_FIXTURE"]]});
    let team = json!({"method":"POST","path":"/api/team.prefs.get","fields":[["token","fixture-not-a-token"],["target_team","T1"],["_x_reason","native-list-permission"]]});
    let user = json!({"method":"POST","path":"/api/users.prefs.get","fields":[["token","fixture-not-a-token"],["prefs","task_lists,assigned_to_list"],["_x_reason","native-list-completion-permission"]]});
    let read = || vec![file.clone(), team.clone(), user.clone()];
    let mut out = read();
    out.push(json!({"method":"POST","path":"/api/lists.views.update","fields":[["token","fixture-not-a-token"],["list_id","F_LIST_FIXTURE"],["id","V_ORDER_SELECTED"],["position","0000000000"],["_x_reason","update-view"]]}));
    out.extend(read());
    out.push(file.clone());
    out.extend(read());
    out
}
pub(super) fn responses(docs: &[Value; 2]) -> Vec<Value> {
    let file = |i: usize| json!({"ok":true,"file":docs[i]});
    let team = json!({"ok":true,"prefs":{"allow_lists":"full"}});
    let user = json!({"ok":true,"prefs":{"task_lists":"[]","assigned_to_list":""}});
    let read = |i| vec![file(i), team.clone(), user.clone()];
    let mut out = read(0);
    out.push(json!({"syntheticFailure":"response_lost"}));
    out.extend(read(0));
    out.push(file(1));
    out.extend(read(1));
    out
}
fn completions(serial: u64) -> Value {
    json!([
        {"serial":serial,"kind":"write","applied":false},
        {"serial":serial,"kind":"check","applied":true},
        {"serial":serial,"kind":"check","applied":false},
        {"serial":serial,"kind":"check","applied":true}
    ])
}
pub(super) fn exact(
    l: &Value,
    count: usize,
    finished: usize,
    pending: bool,
    read: bool,
    server: usize,
    docs: &[Value; 2],
    serial: u64,
) -> Result<(), String> {
    if serial == 0 || count > 11 || finished > 4 || server > 1 {
        return Err("Invalid ordering checkpoint".into());
    }
    let req = requests();
    let res = responses(docs);
    let done = completions(serial);
    if l["requests"] != json!(&req[..count])
        || l["responses"] != json!(&res[..count])
        || l["finished"] != json!(&done.as_array().unwrap()[..finished])
        || l["writePending"] != pending
        || l["readPending"] != read
        || l["server"] != docs[server]
    {
        return Err("Exact ordering methods/forms/responses/completions differ".into());
    }
    Ok(())
}
pub(super) fn stage(l: &Value, index: usize, docs: &[Value; 2], serial: u64) -> Result<(), String> {
    let &(n, f, s) = [
        (0, 0, 0),
        (4, 0, 0),
        (4, 1, 1),
        (7, 2, 1),
        (8, 3, 1),
        (11, 4, 1),
        (11, 4, 1),
    ]
    .get(index)
    .ok_or("Unknown stage")?;
    exact(l, n, f, index == 1, false, s, docs, serial)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn raw() -> Value {
        json!({"id":"F_LIST_FIXTURE","list_metadata":{"schema":[{"key":"unknown","type":"future","opaque":true}],"default_view":"V_ORDER_SELECTED","views":[
 {"id":"V_ORDER_FIRST","position":"0000000001","opaque":{"keep":1}},
 {"id":"V_ORDER_SELECTED","position":"0000000002","filters":[{"opaque":true}]},
 {"id":"V_ORDER_LAST","position":"0000000003","sorts":[{"key":"unknown"}]}]}})
    }
    #[test]
    fn literal_position_only_observation_preserves_array_and_every_unrelated_member() {
        let before = raw();
        let docs = documents(&before).unwrap();
        let mut inverse = docs[1].clone();
        inverse["list_metadata"]["views"][1]["position"] = json!("0000000002");
        assert_eq!(inverse, before);
        let mut bad = before;
        bad["list_metadata"]["views"]
            .as_array_mut()
            .unwrap()
            .swap(0, 1);
        assert!(documents(&bad).is_err());
    }
    #[test]
    fn exact_ledger_rejects_wrong_position_replay_and_retired_policy_reads() {
        let docs = documents(&raw()).unwrap();
        let base = json!({"requests":requests(),"responses":responses(&docs),"finished":completions(17),"server":docs[1],"writePending":false,"readPending":false});
        exact(&base, 11, 4, false, false, 1, &docs, 17).unwrap();
        assert_eq!(
            requests()
                .iter()
                .filter(|r| r["path"] == "/api/lists.views.update")
                .count(),
            1
        );
        for case in 0..6 {
            let mut bad = base.clone();
            match case {
                0 => bad["requests"][3]["fields"][3][1] = json!("0000000001"),
                1 => bad["requests"][3]["fields"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!(["name", "Changed"])),
                2 => bad["finished"][2]["applied"] = json!(true),
                3 => bad["requests"]
                    .as_array_mut()
                    .unwrap()
                    .push(requests()[3].clone()),
                4 => bad["responses"][7]["file"] = docs[0].clone(),
                _ => bad["responses"][1]["prefs"]["allow_lists"] = json!("disabled"),
            };
            assert!(exact(&bad, 11, 4, false, false, 1, &docs, 17).is_err());
        }
    }
}
