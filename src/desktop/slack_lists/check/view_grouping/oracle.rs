//! Independent literal Group by method/forms, snapshots and projection membership.
use serde_json::{Value, json};
pub(super) const STAGES: [&str; 9] = [
    "select-edit",
    "select-saving",
    "uncertain",
    "conflict",
    "retired",
    "select",
    "checkbox",
    "clear-edit",
    "cleared",
];
pub(super) const MODES: [usize; 9] = [0, 0, 0, 2, 2, 1, 0, 0, 3];
pub(super) fn documents(before: &Value) -> [Value; 4] {
    let mut docs = [
        before.clone(),
        before.clone(),
        before.clone(),
        before.clone(),
    ];
    for (i, key) in [(1, "status"), (2, "other"), (3, "")] {
        docs[i]["list_metadata"]["views"][0]["grouping"] = json!({"group_by":key});
    }
    docs
}
fn ids(rows: &[usize]) -> Value {
    json!(
        rows.iter()
            .map(|i| format!("R_LIST_{i:03}"))
            .collect::<Vec<_>>()
    )
}
pub(super) fn groups(mode: usize, board: bool) -> Value {
    let g = |key: &str, label: &str, rows: &[usize]| json!({"key":key,"label":label,"rows":ids(rows),"count":rows.len()});
    match mode {
        0 | 2 => {
            if board {
                json!([
                    g("", "Unchecked", &[2, 4, 8, 0]),
                    g("true", "Checked", &[3, 1, 7, 5])
                ])
            } else {
                json!([
                    g("true", "Checked", &[3, 1, 7, 5]),
                    g("", "Ungrouped", &[2, 4, 8, 0])
                ])
            }
        }
        1 => json!([
            g("OPT_PROGRESS", "In progress", &[1, 4, 8, 0, 5]),
            g("OPT_DONE", "Done", &[2, 3, 7])
        ]),
        3 => json!([g(
            "",
            if board { "Ungrouped" } else { "" },
            &[2, 3, 1, 4, 7, 8, 0, 5]
        )]),
        _ => panic!("Unknown grouping projection"),
    }
}
pub(super) fn columns(mode: usize, board: bool) -> Value {
    if !board || matches!(mode, 2 | 3) {
        json!(["C_NAME", "C_CHECK", "C_NUMBER", "C_SELECT"])
    } else if mode == 1 {
        json!(["C_NAME", "C_CHECK", "C_NUMBER"])
    } else {
        json!(["C_NAME", "C_NUMBER", "C_SELECT"])
    }
}
fn requests() -> Vec<Value> {
    let file = json!({"method":"POST","path":"/api/files.info","fields":[["token","fixture-not-a-token"],["file","F_LIST_FIXTURE"]]});
    let team = json!({"method":"POST","path":"/api/team.prefs.get","fields":[["token","fixture-not-a-token"],["target_team","T1"],["_x_reason","native-list-permission"]]});
    let user = json!({"method":"POST","path":"/api/users.prefs.get","fields":[["token","fixture-not-a-token"],["prefs","task_lists,assigned_to_list"],["_x_reason","native-list-completion-permission"]]});
    let write = |grouping: &str| json!({"method":"POST","path":"/api/lists.views.update","fields":[["token","fixture-not-a-token"],["list_id","F_LIST_FIXTURE"],["id","V_GROUP"],["grouping",grouping],["_x_reason","update-view"]]});
    let reads = || vec![file.clone(), team.clone(), user.clone()];
    let mut out = reads();
    out.push(write("{\"group_by\":\"status\"}"));
    out.extend(reads());
    out.push(file.clone());
    out.extend(reads());
    out.extend(reads());
    out.push(write("{\"group_by\":\"done\"}"));
    out.extend(reads());
    out.extend(reads());
    out.push(write("{\"group_by\":\"\"}"));
    out.extend(reads());
    out
}
fn responses(docs: &[Value; 4]) -> Vec<Value> {
    let file = |i: usize| json!({"ok":true,"file":docs[i]});
    let team = json!({"ok":true,"prefs":{"allow_lists":"full"}});
    let user = json!({"ok":true,"prefs":{"task_lists":"[]","assigned_to_list":""}});
    let reads = |i| vec![file(i), team.clone(), user.clone()];
    let ack = |i: usize| json!({"ok":true,"list":{"id":docs[i]["id"],"list_metadata":docs[i]["list_metadata"]},"opaque_reply":{"grouping":true}});
    let mut out = reads(0);
    out.push(json!({"syntheticFailure":"response_lost"}));
    out.extend(reads(2));
    out.push(file(1));
    out.extend(reads(1));
    out.extend(reads(1));
    out.push(ack(0));
    out.extend(reads(0));
    out.extend(reads(0));
    out.push(ack(3));
    out.extend(reads(3));
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
    docs: &[Value; 4],
    ids: [u64; 3],
) -> Result<(), String> {
    if ids[0] == 0
        || finished > 4 && (ids[1] == 0 || ids[1] == ids[0])
        || finished > 6 && (ids[2] == 0 || ids[..2].contains(&ids[2]))
    {
        return Err("Missing distinct grouping attempt identities".into());
    }
    let req = requests();
    let res = responses(docs);
    let done = completions(ids);
    if count > req.len() || finished > done.len() || server > 3 {
        return Err("Unknown Group by checkpoint".into());
    }
    if l["requests"] != json!(&req[..count])
        || l["responses"] != json!(&res[..count])
        || l["finished"] != json!(&done[..finished])
        || l["writePending"] != pending
        || l["readPending"] != read
        || l["server"] != docs[server]
    {
        return Err("Exact Group by form/response/completion ledger differs".into());
    }
    Ok(())
}

pub(super) fn stage(
    l: &Value,
    index: usize,
    docs: &[Value; 4],
    ids: [u64; 3],
) -> Result<(), String> {
    let &(count, finished, server) = [
        (0, 0, 0),
        (4, 0, 0),
        (4, 1, 1),
        (7, 2, 1),
        (8, 3, 1),
        (11, 4, 1),
        (18, 6, 0),
        (18, 6, 0),
        (25, 8, 3),
    ]
    .get(index)
    .ok_or("Unknown grouping stage")?;
    exact(l, count, finished, index == 1, false, server, docs, ids)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn full_literal_ledger_rejects_extra_fields_raw_mutations_and_obsolete_apply() {
        let docs = documents(
            &json!({"id":"F_LIST_FIXTURE","list_metadata":{"views":[{"id":"V_GROUP","grouping":{"group_by":"done"},"filters":[{"key":"estimate","operator":"greater_than","values":["-4"]}],"sorts":[{"key":"estimate","ascending":true}]}]}}),
        );
        let ids = [17, 18, 19];
        let l = json!({"requests":requests(),"responses":responses(&docs),"finished":completions(ids),"server":docs[3],"writePending":false,"readPending":false});
        exact(&l, 25, 8, false, false, 3, &docs, ids).unwrap();
        assert_eq!(requests().len(), 25);
        assert_eq!(
            requests()
                .iter()
                .filter(|v| v["path"] == "/api/lists.views.update")
                .count(),
            3
        );
        for branch in 0..6 {
            let mut bad = l.clone();
            match branch {
                0 => bad["requests"][3]["fields"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!(["sorts", "[]"])),
                1 => bad["requests"][3]["fields"][3][1] = json!("{\"group_by\":\"C_SELECT\"}"),
                2 => bad["finished"][2]["applied"] = json!(true),
                3 => bad["responses"][4]["file"] = docs[0].clone(),
                4 => bad["server"]["list_metadata"]["views"][0]["sorts"] = json!([]),
                _ => {
                    bad["requests"].as_array_mut().unwrap().pop();
                }
            };
            assert!(exact(&bad, 25, 8, false, false, 3, &docs, ids).is_err());
        }
    }
    #[test]
    fn checkbox_same_bucket_conflict_and_board_clear_order_are_literal() {
        assert_eq!(groups(0, false), groups(2, false));
        assert_eq!(groups(0, true), groups(2, true));
        assert_eq!(groups(3, true)[0]["count"], 8);
        assert_eq!(groups(1, true)[0]["rows"], ids(&[1, 4, 8, 0, 5]));
    }
}
