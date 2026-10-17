//! Literal ordered-mode oracle and explicit structural flow; shared real worker.
use super::*;
pub(super) const STAGES: [&str; 14] = [
    "structure-added",
    "structure-removed",
    "multi-edit",
    "multi-saving",
    "uncertain",
    "stale",
    "retired",
    "multi",
    "reorder-edit",
    "reordered",
    "remove-edit",
    "single",
    "clear-edit",
    "cleared",
];
pub(super) const CANONICAL: [usize; 14] = [0, 0, 0, 0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 0];
pub(super) const CHECKPOINTS: [(usize, usize, usize); 14] = [
    (0, 0, 0),
    (0, 0, 0),
    (0, 0, 0),
    (4, 0, 0),
    (4, 1, 1),
    (7, 2, 1),
    (8, 3, 1),
    (11, 4, 1),
    (11, 4, 1),
    (18, 6, 2),
    (18, 6, 2),
    (25, 8, 3),
    (25, 8, 3),
    (32, 10, 0),
];
pub(super) fn order(mode: usize) -> Vec<String> {
    let rows = match mode {
        0 => [0, 1, 2, 3, 4, 5, 6, 7, 8],
        1 => [0, 8, 4, 2, 6, 5, 7, 1, 3],
        2 => [0, 5, 8, 7, 4, 1, 3, 2, 6],
        3 => [0, 5, 8, 7, 1, 4, 3, 2, 6],
        _ => panic!("Unknown ordered projection"),
    };
    rows.into_iter().map(|i| format!("R_LIST_{i:03}")).collect()
}
pub(super) fn documents(before: &Value) -> Vec<Value> {
    let mut docs = vec![before.clone(); 4];
    docs[1]["list_metadata"]["views"][0]["sorts"] =
        json!([{"key":"done","ascending":true},{"key":"estimate","ascending":false}]);
    docs[2]["list_metadata"]["views"][0]["sorts"] =
        json!([{"key":"estimate","ascending":false},{"key":"done","ascending":true}]);
    docs[3]["list_metadata"]["views"][0]["sorts"] = json!([{"key":"estimate","ascending":false}]);
    docs
}
fn requests() -> Vec<Value> {
    let file = json!({"method":"POST","path":"/api/files.info","fields":[["token","fixture-not-a-token"],["file","F_LIST_FIXTURE"]]});
    let team = json!({"method":"POST","path":"/api/team.prefs.get","fields":[["token","fixture-not-a-token"],["target_team","T1"],["_x_reason","native-list-permission"]]});
    let user = json!({"method":"POST","path":"/api/users.prefs.get","fields":[["token","fixture-not-a-token"],["prefs","task_lists,assigned_to_list"],["_x_reason","native-list-completion-permission"]]});
    let reads = || vec![file.clone(), team.clone(), user.clone()];
    let write = |v: &str| json!({"method":"POST","path":"/api/lists.views.update","fields":[["token","fixture-not-a-token"],["list_id","F_LIST_FIXTURE"],["id","V_SORT"],["sorts",v],["_x_reason","update-view"]]});
    let mut out = reads();
    out.push(write(
        "[{\"ascending\":true,\"key\":\"done\"},{\"ascending\":false,\"key\":\"estimate\"}]",
    ));
    out.extend(reads());
    out.push(file.clone());
    out.extend(reads());
    for value in [
        "[{\"ascending\":false,\"key\":\"estimate\"},{\"ascending\":true,\"key\":\"done\"}]",
        "[{\"ascending\":false,\"key\":\"estimate\"}]",
        "[]",
    ] {
        out.extend(reads());
        out.push(write(value));
        out.extend(reads());
    }
    out
}
fn responses(docs: &[Value]) -> Vec<Value> {
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
    for (before, after) in [(1, 2), (2, 3), (3, 0)] {
        out.extend(reads(before));
        out.push(ack(after));
        out.extend(reads(after));
    }
    out
}
fn completions(ids: [u64; 4]) -> Vec<Value> {
    [
        (0, "write", false),
        (0, "check", true),
        (0, "check", false),
        (0, "check", true),
        (1, "write", false),
        (1, "check", true),
        (2, "write", false),
        (2, "check", true),
        (3, "write", false),
        (3, "check", true),
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
    docs: &[Value],
    ids: [u64; 4],
) -> Result<(), String> {
    if count > 32 || finished > 10 || server > 3 || docs.len() != 4 {
        return Err("Unknown ordered checkpoint".into());
    }
    for (index, min_finished) in [(0, 0), (1, 4), (2, 6), (3, 8)] {
        if finished > min_finished && (ids[index] == 0 || ids[..index].contains(&ids[index])) {
            return Err("Missing distinct ordered attempts".into());
        }
    }
    if count > 0 && ids[0] == 0 {
        return Err("Missing first ordered attempt".into());
    }
    if l["requests"] != json!(&requests()[..count])
        || l["responses"] != json!(&responses(docs)[..count])
        || l["finished"] != json!(&completions(ids)[..finished])
        || l["writePending"] != pending
        || l["readPending"] != read
        || l["server"] != docs[server]
    {
        return Err("Exact ordered ledger differs".into());
    }
    Ok(())
}
pub(super) async fn open(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    mode: usize,
) -> Result<Value, String> {
    let opened = rows::open(state, d).await?;
    let ledger = rename_fixture::snapshot(&state.peek())?;
    let mut actions = vec![];
    match mode {
        1 => {
            actions.push(rows::add(state, d, "C_CHECK").await?);
            actions.push(rows::add(state, d, "C_NUMBER").await?);
            actions.push(
                rows::action(state, d, Some("C_NUMBER"), "direction", Some("descending")).await?,
            );
        }
        2 => actions.push(rows::action(state, d, Some("C_NUMBER"), "up", None).await?),
        3 => actions.push(rows::action(state, d, Some("C_CHECK"), "remove", None).await?),
        0 => actions.push(rows::action(state, d, None, "clear", None).await?),
        _ => return Err("Unknown ordered edit".into()),
    }
    let model = draft(state)?;
    let expected = match mode {
        0 => json!({"column":null,"ascending":null}),
        1 => {
            json!({"ordered":[{"column":"C_CHECK","ascending":true},{"column":"C_NUMBER","ascending":false}]})
        }
        2 => {
            json!({"ordered":[{"column":"C_NUMBER","ascending":false},{"column":"C_CHECK","ascending":true}]})
        }
        _ => json!({"column":"C_NUMBER","ascending":false}),
    };
    if model["desired"] != expected || rename_fixture::snapshot(&state.peek())? != ledger {
        return Err("Ordered edit/ledger differs".into());
    }
    wait_dom(
        d,
        "({ok:document.getElementById('slack-list-rename-confirm')?.disabled===false})",
    )
    .await?;
    let dom = checked(d, &format!("window.__listViewSort.sameDraft({model})")).await?;
    Ok(json!({"model":model,"dom":dom["dom"],"opened":opened,"actions":actions}))
}
pub(super) async fn prelude(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    proof: &mut Proof,
) -> Result<Value, String> {
    let opened = rows::open(state, d).await?;
    let mut actions = vec![];
    for column in ["C_NUMBER", "C_CHECK", "C_NAME"] {
        actions.push(rows::add(state, d, column).await?);
    }
    proof.stage(state, d, "structure-added").await?;
    actions.push(rows::action(state, d, Some("C_CHECK"), "remove", None).await?);
    proof.stage(state, d, "structure-removed").await?;
    for (column, kind, value) in [
        (Some("C_NAME"), "field", Some("C_SELECT")),
        (Some("C_SELECT"), "field", Some("C_NAME")),
        (Some("C_NAME"), "direction", Some("descending")),
        (Some("C_NAME"), "up", None),
        (Some("C_NAME"), "down", None),
        (None, "clear", None),
    ] {
        actions.push(rows::action(state, d, column, kind, value).await?);
    }
    let model = draft(state)?;
    if model["desired"] != model["original"] || model["rows"] != json!([]) {
        return Err("Structural prelude did not restore empty draft".into());
    }
    let disabled = wait_dom(
        d,
        "({ok:document.getElementById('slack-list-rename-confirm')?.disabled===true})",
    )
    .await?;
    proof.exact(
        &rename_fixture::snapshot(&state.peek())?,
        0,
        0,
        false,
        false,
        0,
    )?;
    let dom = checked(d, &format!("window.__listViewSort.sameDraft({model})")).await?;
    Ok(
        json!({"model":model,"dom":dom["dom"],"opened":opened,"actions":actions,"disabled":disabled}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ordered_oracle_rejects_wrong_priority_extra_form_and_applied_retired_read() {
        let before = json!({"id":"F_LIST_FIXTURE","list_metadata":{"views":[{"id":"V_SORT","sorts":[],"opaque":7},{"id":"V_PRECISION_BOARD","sorts":[],"keep":true}]}});
        let docs = documents(&before);
        let ids = [17, 18, 19, 20];
        let base = json!({"requests":requests(),"responses":responses(&docs),"finished":completions(ids),"server":docs[0],"writePending":false,"readPending":false});
        exact(&base, 32, 10, false, false, 0, &docs, ids).unwrap();
        for index in 0..4 {
            let mut bad = base.clone();
            match index {
                0 => {
                    bad["requests"][3]["fields"][3][1] = bad["requests"][14]["fields"][3][1].clone()
                }
                1 => bad["requests"].as_array_mut().unwrap().push(json!({})),
                2 => bad["finished"][2]["applied"] = json!(true),
                _ => {
                    bad["server"]["list_metadata"]["views"][1]["sorts"] =
                        json!([{"key":"done","ascending":true}])
                }
            }
            assert!(exact(&bad, 32, 10, false, false, 0, &docs, ids).is_err());
        }
        assert_eq!(
            requests()
                .iter()
                .filter(|r| r["path"] == "/api/lists.views.update")
                .count(),
            4
        );
        assert_eq!(
            docs[3]["list_metadata"]["views"][1],
            before["list_metadata"]["views"][1]
        );
    }
}
