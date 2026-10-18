//! Independent fixed Sort vectors and complete new workflow request ledger.
use super::*;
pub(super) fn sorts(step: usize, ordered: bool) -> Value {
    match (ordered, step) {
        (false, 0) => json!([{"key":"estimate","ascending":true}]),
        (false, 1) | (true, 2) => json!([{"key":"estimate","ascending":false}]),
        (true, 0) => json!([{"key":"done","ascending":true},{"key":"estimate","ascending":false}]),
        (true, 1) => json!([{"key":"estimate","ascending":false},{"key":"done","ascending":true}]),
        (false, 2) | (true, 3) => json!([]),
        _ => panic!("Unknown Sort step"),
    }
}
pub(super) fn documents(before: &Value, ordered: bool) -> Vec<Value> {
    let mut out = vec![before.clone()];
    for i in 0..if ordered { 4 } else { 3 } {
        let mut doc = before.clone();
        let v = &mut doc["list_metadata"]["views"][0];
        v["sorts"] = sorts(i, ordered);
        v["filters"] = json!([]);
        v["info_column_filters"] = json!([]);
        v["type"] = json!("table");
        v["grouping"] = json!({});
        v["show_completed_items"] = json!(false);
        out.push(doc);
    }
    out
}
fn request(path: &str, fields: Vec<(&str, String)>) -> Value {
    let mut f = vec![json!(["token", "fixture-not-a-token"])];
    f.extend(fields.into_iter().map(|(k, v)| json!([k, v])));
    json!({"method":"POST","path":path,"fields":f})
}
fn pref(saved: bool) -> Value {
    json!({"ok":true,"prefs":{"lists_default_view":if saved {"{\"F_LIST_FIXTURE\":\"V_SORT\",\"F_OTHER\":\"V_OTHER\",\"opaque\":\"retained\"}"}else{"{ \"F_OTHER\": \"V_OTHER\", \"F_LIST_FIXTURE\": \"V_PRECISION\", \"opaque\": \"retained\" }"}}})
}
pub(super) fn timeline(docs: &[Value], ordered: bool) -> (Vec<Value>, Vec<Value>) {
    let file = request("/api/files.info", vec![("file", "F_LIST_FIXTURE".into())]);
    let team = request(
        "/api/team.prefs.get",
        vec![
            ("target_team", "T1".into()),
            ("_x_reason", "native-list-permission".into()),
        ],
    );
    let user = request(
        "/api/users.prefs.get",
        vec![
            ("prefs", "task_lists,assigned_to_list".into()),
            ("_x_reason", "native-list-completion-permission".into()),
        ],
    );
    let pr = request(
        "/api/users.prefs.get",
        vec![
            ("prefs", "lists_default_view".into()),
            ("_x_reason", "native-list-default-view-read".into()),
        ],
    );
    let pw = request(
        "/api/users.prefs.set",
        vec![
            ("name", "lists_default_view".into()),
            (
                "value",
                pref(true)["prefs"]["lists_default_view"]
                    .as_str()
                    .unwrap()
                    .into(),
            ),
            ("_x_reason", "prefs-api/setUserPrefByApi".into()),
        ],
    );
    let mut req = vec![];
    let mut res = vec![];
    let reads = |req: &mut Vec<Value>, res: &mut Vec<Value>, i: usize| {
        req.extend([file.clone(), team.clone(), user.clone()]);
        res.extend([
            json!({"ok":true,"file":docs[i]}),
            json!({"ok":true,"prefs":{"allow_lists":"full"}}),
            json!({"ok":true,"prefs":{"task_lists":"[]","assigned_to_list":""}}),
        ]);
    };
    for step in 0..if ordered { 4 } else { 3 } {
        reads(&mut req, &mut res, step);
        req.push(pr.clone());
        res.push(pref(step > 1));
        req.push(request(
            "/api/lists.views.update",
            vec![
                ("list_id", "F_LIST_FIXTURE".into()),
                ("id", "V_SORT".into()),
                ("type", "table".into()),
                ("filters", "[]".into()),
                ("info_column_filters", "[]".into()),
                ("sorts", sorts(step, ordered).to_string()),
                ("grouping", "{}".into()),
                ("show_completed_items", "false".into()),
                ("_x_reason", "update-view".into()),
            ],
        ));
        res.push(if step == 0 {
            json!({"syntheticFailure":"response_lost"})
        } else {
            json!({"ok":true,"list":docs[step+1],"opaque_reply":{"sort_refinement":true}})
        });
        if step == 0 {
            reads(&mut req, &mut res, 0);
            req.push(file.clone());
            res.push(json!({"ok":true,"file":docs[1]}));
        } else {
            req.push(pw.clone());
            res.push(json!({"ok":true}));
            req.push(pr.clone());
            res.push(pref(true));
        }
        reads(&mut req, &mut res, step + 1);
    }
    (req, res)
}
pub(super) fn exact(
    l: &Value,
    docs: &[Value],
    ordered: bool,
    ids: &[u64],
    count: usize,
    finished: usize,
    server: usize,
    write: bool,
    read: bool,
    child: bool,
) -> Result<(), String> {
    let (requests, responses) = timeline(docs, ordered);
    let mut completions = vec![];
    for (i, id) in ids.iter().enumerate() {
        completions.push(json!({"serial":id,"kind":"write","applied":false}));
        if i == 0 {
            completions.push(json!({"serial":id,"kind":"check","applied":true}));
            completions.push(json!({"serial":id,"kind":"check","applied":false}));
        }
        completions.push(json!({"serial":id,"kind":"check","applied":true}));
    }
    if requests.get(..count) != l["requests"].as_array().map(Vec::as_slice)
        || responses.get(..count) != l["responses"].as_array().map(Vec::as_slice)
        || completions.get(..finished) != l["finished"].as_array().map(Vec::as_slice)
        || l["server"] != docs[server]
        || l["writePending"] != write
        || l["readPending"] != read
        || l["preferencePending"] != child
    {
        return Err("Exact local Apply/shared Save Sort ledger mismatch".into());
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn independent_full_save_timeline_has_no_child_for_lost_ack_and_preserves_unknowns() {
        let before = json!({"list_metadata":{"views":[{"id":"V_SORT","opaque":7,"sorts":[]},{"id":"other","opaque":9}]}});
        for ordered in [false, true] {
            let docs = documents(&before, ordered);
            let (q, r) = timeline(&docs, ordered);
            assert_eq!(q.len(), if ordered { 42 } else { 32 });
            assert_eq!(q.len(), r.len());
            assert!(q[..12].iter().all(|r| r["path"] != "/api/users.prefs.set"));
            assert_eq!(
                q.iter()
                    .filter(|r| r["path"] == "/api/users.prefs.set")
                    .count(),
                if ordered { 3 } else { 2 }
            );
            assert_eq!(
                docs.last().unwrap()["list_metadata"]["views"][0]["sorts"],
                json!([])
            );
            for d in docs {
                assert_eq!(
                    d["list_metadata"]["views"][1],
                    before["list_metadata"]["views"][1]
                );
            }
        }
    }
}
