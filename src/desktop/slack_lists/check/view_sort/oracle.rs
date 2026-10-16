//! Independent literal forms, responses, completions and expected visible order.
use serde_json::{Value, json};
pub(super) const STAGES: [&str; 8] = [
    "asc-edit",
    "asc-saving",
    "uncertain",
    "stale",
    "ascending",
    "descending",
    "clear-edit",
    "cleared",
];
pub(super) fn order(mode: usize) -> Vec<String> {
    let rows = match mode {
        0 => [0, 1, 2, 3, 4, 5, 6, 7, 8],
        1 => [2, 3, 1, 4, 7, 8, 0, 5, 6],
        2 => [0, 5, 8, 7, 1, 4, 3, 2, 6],
        _ => panic!("unknown sort"),
    };
    rows.into_iter().map(|i| format!("R_LIST_{i:03}")).collect()
}
pub(super) fn documents(before: &Value) -> [Value; 3] {
    let mut asc = before.clone();
    asc["list_metadata"]["views"][0]["sorts"] = json!([{"key":"estimate","ascending":true}]);
    let mut desc = before.clone();
    desc["list_metadata"]["views"][0]["sorts"] = json!([{"key":"estimate","ascending":false}]);
    [before.clone(), asc, desc]
}
fn requests() -> Vec<Value> {
    let file = json!({"method":"POST","path":"/api/files.info","fields":[["token","fixture-not-a-token"],["file","F_LIST_FIXTURE"]]});
    let team = json!({"method":"POST","path":"/api/team.prefs.get","fields":[["token","fixture-not-a-token"],["target_team","T1"],["_x_reason","native-list-permission"]]});
    let user = json!({"method":"POST","path":"/api/users.prefs.get","fields":[["token","fixture-not-a-token"],["prefs","task_lists,assigned_to_list"],["_x_reason","native-list-completion-permission"]]});
    let write = |sorts: &str| json!({"method":"POST","path":"/api/lists.views.update","fields":[["token","fixture-not-a-token"],["list_id","F_LIST_FIXTURE"],["id","V_SORT"],["sorts",sorts],["_x_reason","update-view"]]});
    let reads = || vec![file.clone(), team.clone(), user.clone()];
    let mut out = reads();
    out.push(write("[{\"ascending\":true,\"key\":\"estimate\"}]"));
    out.extend(reads());
    out.push(file.clone());
    out.extend(reads());
    out.extend(reads());
    out.push(write("[{\"ascending\":false,\"key\":\"estimate\"}]"));
    out.extend(reads());
    out.extend(reads());
    out.push(write("[]"));
    out.extend(reads());
    out
}
fn responses(docs: &[Value; 3]) -> Vec<Value> {
    let file = |i: usize| json!({"ok":true,"file":docs[i]});
    let team = json!({"ok":true,"prefs":{"allow_lists":"full"}});
    let user = json!({"ok":true,"prefs":{"task_lists":"[]","assigned_to_list":""}});
    let reads = |i| vec![file(i), team.clone(), user.clone()];
    let ack = |i: usize| json!({"ok":true,"list":{"id":docs[i]["id"],"list_metadata":docs[i]["list_metadata"]},"opaque_reply":{"sort":true}});
    let mut out = reads(0);
    out.push(json!({"syntheticFailure":"response_lost"}));
    out.extend(reads(0));
    out.push(file(1));
    out.extend(reads(1));
    out.extend(reads(1));
    out.push(ack(2));
    out.extend(reads(2));
    out.extend(reads(2));
    out.push(ack(0));
    out.extend(reads(0));
    out
}
fn completions(ids: [u64; 3]) -> Vec<Value> {
    [
        (0, "write", false),
        (0, "check", true),
        (0, "check", false),
        (0, "check", true),
        (1, "write", false),
        (1, "check", true),
        (2, "write", false),
        (2, "check", true),
    ]
    .into_iter()
    .map(|(i, kind, applied)| json!({"serial":ids[i],"kind":kind,"applied":applied}))
    .collect()
}
pub(super) fn exact(
    l: &Value,
    count: usize,
    finished: usize,
    pending: bool,
    read: bool,
    server: usize,
    docs: &[Value; 3],
    ids: [u64; 3],
) -> Result<(), String> {
    if ids[0] == 0
        || finished > 4 && (ids[1] == 0 || ids[1] == ids[0])
        || finished > 6 && (ids[2] == 0 || ids[..2].contains(&ids[2]))
    {
        return Err("Missing distinct sort attempt identities".into());
    }
    let req = requests();
    let res = responses(docs);
    let done = completions(ids);
    if count > req.len() || finished > done.len() || server > 2 {
        return Err("Unknown Sort checkpoint".into());
    }
    if l["requests"] != json!(&req[..count])
        || l["responses"] != json!(&res[..count])
        || l["finished"] != json!(&done[..finished])
        || l["writePending"] != pending
        || l["readPending"] != read
        || l["server"] != docs[server]
    {
        return Err("Exact Sort form/response/completion ledger differs".into());
    }
    Ok(())
}
pub(super) fn stage(
    l: &Value,
    index: usize,
    docs: &[Value; 3],
    ids: [u64; 3],
) -> Result<(), String> {
    let &(count, finished, server) = [
        (0, 0, 0),
        (4, 0, 0),
        (4, 1, 1),
        (7, 2, 1),
        (11, 4, 1),
        (18, 6, 2),
        (18, 6, 2),
        (25, 8, 0),
    ]
    .get(index)
    .ok_or("Unknown sort stage")?;
    exact(l, count, finished, index == 1, false, server, docs, ids)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn oracle_rejects_wrong_vector_extra_request_and_retired_projection() {
        let docs = documents(
            &json!({"id":"F_LIST_FIXTURE","list_metadata":{"views":[{"id":"V_SORT","sorts":[]}]}}),
        );
        let ids = [17, 18, 19];
        let base = json!({"requests":requests(),"responses":responses(&docs),"finished":completions(ids),"server":docs[0],"writePending":false,"readPending":false});
        exact(&base, 25, 8, false, false, 0, &docs, ids).unwrap();
        for branch in 0..3 {
            let mut bad = base.clone();
            match branch {
                0 => bad["requests"][3]["fields"][3][1] = json!("[]"),
                1 => bad["requests"].as_array_mut().unwrap().push(json!({})),
                _ => bad["finished"][2]["applied"] = json!(true),
            };
            assert!(exact(&bad, 25, 8, false, false, 0, &docs, ids).is_err());
        }
        assert_eq!(
            requests()
                .iter()
                .filter(|r| r["path"] == "/api/lists.views.update")
                .count(),
            3
        );
    }
}
