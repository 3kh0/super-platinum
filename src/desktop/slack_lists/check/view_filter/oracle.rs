//! Independent literal forms, responses, completions and expected visible order.
use serde_json::{Value, json};
pub(super) const STAGES: [&str; 8] = [
    "checked-edit",
    "checked-saving",
    "uncertain",
    "stale",
    "checked",
    "unchecked",
    "clear-edit",
    "cleared",
];
pub(super) fn order(mode: usize, number: bool) -> Vec<String> {
    let rows: &[usize] = match (number, mode) {
        (true, 1) => &[2, 3, 1, 4, 7, 8],
        (true, 2) => &[6],
        (_, 0) => &[2, 3, 1, 4, 7, 8, 0, 5, 6],
        (_, 1) => &[3, 1, 7, 5],
        (_, 2) => &[2, 4, 8, 0, 6],
        _ => panic!("unknown filter"),
    };
    rows.iter().map(|i| format!("R_LIST_{i:03}")).collect()
}

pub(super) fn documents(before: &Value, number: bool) -> [Value; 3] {
    let mut checked_document = before.clone();
    checked_document["list_metadata"]["views"][0]["filters"] =
        json!([{"key":"done","operator":"is_not_empty","values":[]}]);
    let mut unchecked_document = before.clone();
    unchecked_document["list_metadata"]["views"][0]["filters"] =
        json!([{"key":"done","operator":"is_empty","values":[]}]);
    checked_document["list_metadata"]["views"][0]["info_column_filters"] = json!([]);
    unchecked_document["list_metadata"]["views"][0]["info_column_filters"] = json!([]);
    if number {
        checked_document["list_metadata"]["views"][0]["filters"] =
            json!([{"key":"estimate","operator":"less_than","values":["12.50"]}]);
        unchecked_document["list_metadata"]["views"][0]["filters"] =
            json!([{"key":"estimate","operator":"is_empty","values":[]}]);
    }
    [before.clone(), checked_document, unchecked_document]
}
fn requests(number: bool) -> Vec<Value> {
    let file = json!({"method":"POST","path":"/api/files.info","fields":[["token","fixture-not-a-token"],["file","F_LIST_FIXTURE"]]});
    let team = json!({"method":"POST","path":"/api/team.prefs.get","fields":[["token","fixture-not-a-token"],["target_team","T1"],["_x_reason","native-list-permission"]]});
    let user = json!({"method":"POST","path":"/api/users.prefs.get","fields":[["token","fixture-not-a-token"],["prefs","task_lists,assigned_to_list"],["_x_reason","native-list-completion-permission"]]});
    let write = |filters: &str| json!({"method":"POST","path":"/api/lists.views.update","fields":[["token","fixture-not-a-token"],["list_id","F_LIST_FIXTURE"],["id","V_FILTER"],["filters",filters],["info_column_filters","[]"],["_x_reason","update-view"]]});
    let reads = || vec![file.clone(), team.clone(), user.clone()];
    let mut out = reads();
    out.push(write(
        "[{\"key\":\"done\",\"operator\":\"is_not_empty\",\"values\":[]}]",
    ));
    out.extend(reads());
    out.push(file.clone());
    out.extend(reads());
    out.extend(reads());
    out.push(write(
        "[{\"key\":\"done\",\"operator\":\"is_empty\",\"values\":[]}]",
    ));
    out.extend(reads());
    out.extend(reads());
    out.push(write("[]"));
    out.extend(reads());
    if number {
        out[3]["fields"][3][1] =
            json!("[{\"key\":\"estimate\",\"operator\":\"less_than\",\"values\":[\"12.50\"]}]");
        out[14]["fields"][3][1] =
            json!("[{\"key\":\"estimate\",\"operator\":\"is_empty\",\"values\":[]}]");
    }
    out
}
fn requests_set(number: bool, compound: bool) -> Vec<Value> {
    let mut req = requests(number);
    if compound {
        req[3]["fields"][3][1] = json!(
            "[{\"key\":\"done\",\"operator\":\"is_not_empty\",\"values\":[]},{\"key\":\"estimate\",\"operator\":\"less_than\",\"values\":[\"12.50\"]}]"
        );
        req[14]["fields"][3][1] =
            json!("[{\"key\":\"estimate\",\"operator\":\"less_than\",\"values\":[\"12.50\"]}]");
    }
    req
}
fn responses(docs: &[Value; 3]) -> Vec<Value> {
    let file = |i: usize| json!({"ok":true,"file":docs[i]});
    let team = json!({"ok":true,"prefs":{"allow_lists":"full"}});
    let user = json!({"ok":true,"prefs":{"task_lists":"[]","assigned_to_list":""}});
    let reads = |i| vec![file(i), team.clone(), user.clone()];
    let ack = |i: usize| json!({"ok":true,"list":{"id":docs[i]["id"],"list_metadata":docs[i]["list_metadata"]},"opaque_reply":{"filter":true}});
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
    number: bool,
) -> Result<(), String> {
    exact_set(
        l, count, finished, pending, read, server, docs, ids, number, false,
    )
}
pub(super) fn exact_set(
    l: &Value,
    count: usize,
    finished: usize,
    pending: bool,
    read: bool,
    server: usize,
    docs: &[Value; 3],
    ids: [u64; 3],
    number: bool,
    compound: bool,
) -> Result<(), String> {
    if ids[0] == 0
        || finished > 4 && (ids[1] == 0 || ids[1] == ids[0])
        || finished > 6 && (ids[2] == 0 || ids[..2].contains(&ids[2]))
    {
        return Err("Missing distinct filter attempt identities".into());
    }
    let req = requests_set(number, compound);
    let res = responses(docs);
    let done = completions(ids);
    if count > req.len() || finished > done.len() || server > 2 {
        return Err("Unknown Filter checkpoint".into());
    }
    if l["requests"] != json!(&req[..count])
        || l["responses"] != json!(&res[..count])
        || l["finished"] != json!(&done[..finished])
        || l["writePending"] != pending
        || l["readPending"] != read
        || l["server"] != docs[server]
    {
        return Err("Exact Filter form/response/completion ledger differs".into());
    }
    Ok(())
}
pub(super) fn stage_name(index: usize, number: bool) -> &'static str {
    if number {
        [
            "compare-edit",
            "compare-saving",
            "uncertain",
            "stale",
            "compared",
            "empty",
            "clear-edit",
            "cleared",
        ][index]
    } else {
        STAGES[index]
    }
}
pub(super) fn stage(
    l: &Value,
    index: usize,
    docs: &[Value; 3],
    ids: [u64; 3],
    number: bool,
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
    .ok_or("Unknown filter stage")?;
    exact(
        l,
        count,
        finished,
        index == 1,
        false,
        server,
        docs,
        ids,
        number,
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn compound_literal_vectors_keep_both_ordered_rules_and_retired_false() {
        let before = json!({"id":"F_LIST_FIXTURE","list_metadata":{"views":[{"id":"V_FILTER","filters":[],"info_column_filters":[],"sorts":[{"key":"estimate","ascending":true}],"opaque":{"keep":true}}]}});
        let docs = documents_set(&before, false, true);
        assert_eq!(docs[0], before);
        assert_eq!(
            docs[1]["list_metadata"]["views"][0]["sorts"],
            before["list_metadata"]["views"][0]["sorts"]
        );
        assert_eq!(
            order_set(1, false, true),
            [3, 1, 7].map(|i| format!("R_LIST_{i:03}"))
        );
        assert_eq!(
            order_set(2, false, true),
            [2, 3, 1, 4, 7, 8].map(|i| format!("R_LIST_{i:03}"))
        );
        let req = requests_set(false, true);
        let ids = [41, 42, 43];
        assert_eq!(req.len(), 25);
        assert_eq!(
            req.iter()
                .filter(|r| r["path"] == "/api/lists.views.update")
                .count(),
            3
        );
        let base = json!({"requests":req,"responses":responses(&docs),"finished":completions(ids),"server":docs[0],"writePending":false,"readPending":false});
        stage_set(&base, 9, &docs, ids, false, true).unwrap();
        for branch in 0..6 {
            let mut bad = base.clone();
            match branch {
                0 => bad["requests"][3]["fields"][3][1] = json!("[]"),
                1 => {
                    let mut rows = docs[1]["list_metadata"]["views"][0]["filters"]
                        .as_array()
                        .unwrap()
                        .clone();
                    rows.reverse();
                    bad["requests"][3]["fields"][3][1] =
                        json!(serde_json::to_string(&rows).unwrap());
                }
                2 => bad["requests"][3]["fields"][4][1] = json!("null"),
                3 => bad["finished"][2]["applied"] = json!(true),
                4 => {
                    bad["requests"][14]["fields"][3][1] =
                        base["requests"][3]["fields"][3][1].clone()
                }
                _ => bad["server"]["list_metadata"]["views"][0]["sorts"] = json!([]),
            }
            assert!(
                stage_set(&bad, 9, &docs, ids, false, true).is_err(),
                "branch {branch}"
            );
        }
        // Count and server progression is fixed independently for every capture.
        for (index, (count, finished, server)) in [
            (0, 0, 0),
            (4, 0, 0),
            (4, 1, 1),
            (7, 2, 1),
            (8, 3, 1),
            (11, 4, 1),
            (11, 4, 1),
            (18, 6, 2),
            (18, 6, 2),
            (25, 8, 0),
        ]
        .into_iter()
        .enumerate()
        {
            let sample = json!({"requests":&requests_set(false,true)[..count],"responses":&responses(&docs)[..count],"finished":&completions(ids)[..finished],"server":docs[server],"writePending":index==1,"readPending":false});
            stage_set(&sample, index, &docs, ids, false, true).unwrap();
        }
    }
    #[test]
    fn number_oracle_keeps_string_and_literal_negative_zero_ties_empty_order() {
        assert_eq!(
            order(1, true),
            [2, 3, 1, 4, 7, 8].map(|i| format!("R_LIST_{i:03}"))
        );
        assert_eq!(order(2, true), ["R_LIST_006"]);
        let r = requests(true);
        let value: Value = serde_json::from_str(r[3]["fields"][3][1].as_str().unwrap()).unwrap();
        assert_eq!(
            value,
            json!([{"key":"estimate","operator":"less_than","values":["12.50"]}])
        );
        assert_eq!(r.len(), 25);
        let docs = documents(
            &json!({"list_metadata":{"views":[{"filters":[],"info_column_filters":[],"sorts":[{"key":"estimate","ascending":true}]}]}}),
            true,
        );
        let ids = [1, 2, 3];
        let base = json!({"requests":r,"responses":responses(&docs),"finished":completions(ids),"server":docs[0],"writePending":false,"readPending":false});
        exact(&base, 25, 8, false, false, 0, &docs, ids, true).unwrap();
        for bad_value in [
            "[{\"key\":\"estimate\",\"operator\":\"less_than\",\"values\":[12.5]}]",
            "[{\"key\":\"estimate\",\"operator\":\"less_than\",\"values\":[\"12.5\"]}]",
        ] {
            let mut bad = base.clone();
            bad["requests"][3]["fields"][3][1] = json!(bad_value);
            assert!(exact(&bad, 25, 8, false, false, 0, &docs, ids, true).is_err());
        }
    }
    #[test]
    fn oracle_rejects_wrong_vector_extra_request_and_retired_projection() {
        let docs = documents(
            &json!({"id":"F_LIST_FIXTURE","list_metadata":{"views":[{"id":"V_FILTER","filters":[],"info_column_filters":[],"sorts":[{"key":"estimate","ascending":true}]}]}}),
            false,
        );
        let ids = [17, 18, 19];
        let base = json!({"requests":requests(false),"responses":responses(&docs),"finished":completions(ids),"server":docs[0],"writePending":false,"readPending":false});
        exact(&base, 25, 8, false, false, 0, &docs, ids, false).unwrap();
        for branch in 0..4 {
            let mut bad = base.clone();
            match branch {
                0 => bad["requests"][3]["fields"][3][1] = json!("[]"),
                1 => bad["requests"].as_array_mut().unwrap().push(json!({})),
                2 => bad["finished"][2]["applied"] = json!(true),
                _ => bad["requests"][3]["fields"][4][1] = json!("null"),
            };
            assert!(exact(&bad, 25, 8, false, false, 0, &docs, ids, false).is_err());
        }
        assert_eq!(
            requests(false)
                .iter()
                .filter(|r| r["path"] == "/api/lists.views.update")
                .count(),
            3
        );
    }
}

pub(super) const COMPOUND_STAGES: [&str; 10] = [
    "mixed-edit",
    "mixed-saving",
    "uncertain",
    "conflict",
    "retired",
    "mixed",
    "single-edit",
    "single",
    "clear-edit",
    "cleared",
];
pub(super) fn order_set(index: usize, number: bool, compound: bool) -> Vec<String> {
    if !compound {
        return order(index, number);
    }
    let rows: &[usize] = match index {
        0 => &[2, 3, 1, 4, 7, 8, 0, 5, 6],
        1 => &[3, 1, 7],
        2 => &[2, 3, 1, 4, 7, 8],
        _ => panic!("unknown compound document"),
    };
    rows.iter().map(|i| format!("R_LIST_{i:03}")).collect()
}
pub(super) fn documents_set(before: &Value, number: bool, compound: bool) -> [Value; 3] {
    let mut docs = documents(before, number);
    if compound {
        docs[1]["list_metadata"]["views"][0]["filters"] = json!([{"key":"done","operator":"is_not_empty","values":[]},{"key":"estimate","operator":"less_than","values":["12.50"]}]);
        docs[2]["list_metadata"]["views"][0]["filters"] =
            json!([{"key":"estimate","operator":"less_than","values":["12.50"]}]);
    }
    docs
}
pub(super) fn stage_set(
    l: &Value,
    index: usize,
    docs: &[Value; 3],
    ids: [u64; 3],
    number: bool,
    compound: bool,
) -> Result<(), String> {
    if !compound {
        return stage(l, index, docs, ids, number);
    }
    let &(count, finished, server) = [
        (0, 0, 0),
        (4, 0, 0),
        (4, 1, 1),
        (7, 2, 1),
        (8, 3, 1),
        (11, 4, 1),
        (11, 4, 1),
        (18, 6, 2),
        (18, 6, 2),
        (25, 8, 0),
    ]
    .get(index)
    .ok_or("Wrong compound stage")?;
    exact_set(
        l,
        count,
        finished,
        index == 1,
        false,
        server,
        docs,
        ids,
        number,
        compound,
    )
}
