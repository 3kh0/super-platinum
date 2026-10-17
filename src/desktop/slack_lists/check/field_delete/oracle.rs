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
pub(super) fn documents(before: &Value) -> Result<[Value; 2], String> {
    let schema = before["list_metadata"]["schema"]
        .as_array()
        .ok_or("Missing schema")?;
    if schema.len() != 13
        || schema[11]["id"] != "C_HIDDEN"
        || schema[11]["key"] != "hidden"
        || schema[12]["id"] != "C_ABSENT"
        || schema[..11].iter().any(|c| c["key"] == "hidden")
        || before["list_metadata"]["views"].as_array().map(Vec::len) != Some(3)
        || before["list_metadata"]["views"][0]["id"] != "V_FIELD_DELETE"
        || before["list_metadata"]["views"][1]["id"] != "V_FIELD_DELETE_BOARD"
        || before["list_metadata"]["views"][2]["id"] != "V_FIELD_DELETE_OTHER"
    {
        return Err("Unexpected deletion seed identities".into());
    }
    let mut after = before.clone();
    let mut retained = schema[..11].to_vec();
    retained.push(schema[12].clone());
    after["list_metadata"]["schema"] = json!(retained);
    after["list_metadata"]["views"][0]["columns"] = json!([
        {"id":"C_NAME","key":"NAME","visible":true},{"id":"C_NUMBER","key":"estimate","visible":true},{"id":"C_CHECK","key":"done","visible":true}
    ]);
    after["list_metadata"]["views"][1]["columns"] = json!([
        {"id":"C_NAME","key":"NAME","visible":true},{"id":"C_CHECK","key":"done","visible":true}
    ]);
    // Third view and every non-columns view member are retained exactly, including
    // dangling saved refinements. No server cleanup/normalization is inferred.
    Ok([before.clone(), after])
}
pub(super) fn requests() -> Vec<Value> {
    let file = json!({"method":"POST","path":"/api/files.info","fields":[["token","fixture-not-a-token"],["file","F_LIST_FIXTURE"]]});
    let team = json!({"method":"POST","path":"/api/team.prefs.get","fields":[["token","fixture-not-a-token"],["target_team","T1"],["_x_reason","native-list-permission"]]});
    let user = json!({"method":"POST","path":"/api/users.prefs.get","fields":[["token","fixture-not-a-token"],["prefs","task_lists,assigned_to_list"],["_x_reason","native-list-completion-permission"]]});
    let experiments = json!({"method":"POST","path":"/api/experiments.getByUser","fields":[["token","fixture-not-a-token"],["ssb_platform",std::env::consts::OS.replace("macos","darwin")],["ssb_release_channel","prod"]]});
    let read = || {
        vec![
            file.clone(),
            team.clone(),
            user.clone(),
            experiments.clone(),
        ]
    };
    let mut out = read();
    out.push(json!({"method":"POST","path":"/api/lists.columns.delete","fields":[["token","fixture-not-a-token"],["list_id","F_LIST_FIXTURE"],["key","hidden"],["_x_reason","delete-column"]]}));
    out.extend(read());
    out.push(file.clone());
    out.extend(read());
    out
}
pub(super) fn responses(docs: &[Value; 2]) -> Vec<Value> {
    let file = |i: usize| json!({"ok":true,"file":docs[i]});
    let team = json!({"ok":true,"prefs":{"allow_lists":"full"}});
    let user = json!({"ok":true,"prefs":{"task_lists":"[]","assigned_to_list":""}});
    let experiments = json!({"ok":true,"assignments":{"lists_schema_owner_only":{"group":"off"}}});
    let read = |i| vec![file(i), team.clone(), user.clone(), experiments.clone()];
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
    if serial == 0 || count > 14 || finished > 4 || server > 1 {
        return Err("Invalid deletion checkpoint".into());
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
        return Err("Exact deletion methods/forms/responses/completions differ".into());
    }
    Ok(())
}
pub(super) fn stage(l: &Value, index: usize, docs: &[Value; 2], serial: u64) -> Result<(), String> {
    let &(n, f, s) = [
        (0, 0, 0),
        (5, 0, 0),
        (5, 1, 1),
        (9, 2, 1),
        (10, 3, 1),
        (14, 4, 1),
        (14, 4, 1),
    ]
    .get(index)
    .ok_or("Unknown stage")?;
    exact(l, n, f, index == 1, false, s, docs, serial)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn literal_ledger_rejects_wrong_key_replay_extra_permission_and_retired_projection() {
        let docs = [
            json!({"id":"F_LIST_FIXTURE","list_metadata":{"schema":[{"key":"hidden"}]}}),
            json!({"id":"F_LIST_FIXTURE","list_metadata":{"schema":[]}}),
        ];
        let l = json!({"requests":requests(),"responses":responses(&docs),"finished":completions(17),"server":docs[1],"writePending":false,"readPending":false});
        exact(&l, 14, 4, false, false, 1, &docs, 17).unwrap();
        assert_eq!(
            requests()
                .iter()
                .filter(|r| r["path"] == "/api/lists.columns.delete")
                .count(),
            1
        );
        assert_eq!(
            requests()
                .iter()
                .filter(|r| r["path"] == "/api/experiments.getByUser")
                .count(),
            3
        );
        for case in 0..6 {
            let mut bad = l.clone();
            match case {
                0 => bad["requests"][4]["fields"][2][1] = json!("C_HIDDEN"),
                1 => bad["requests"][4]["fields"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!(["column_id", "C_HIDDEN"])),
                2 => bad["finished"][2]["applied"] = json!(true),
                3 => bad["requests"]
                    .as_array_mut()
                    .unwrap()
                    .push(requests()[4].clone()),
                4 => bad["responses"][9]["file"] = docs[0].clone(),
                _ => {
                    bad["responses"][3]["assignments"]["lists_schema_owner_only"]["group"] =
                        json!("on")
                }
            };
            assert!(exact(&bad, 14, 4, false, false, 1, &docs, 17).is_err());
        }
    }
    #[test]
    fn independent_expected_removal_keeps_unknown_metadata_and_stale_refinements() {
        let mut schema = (0..11)
            .map(|i| json!({"id":format!("C{i}"),"key":format!("k{i}")}))
            .collect::<Vec<_>>();
        schema.extend([
            json!({"id":"C_HIDDEN","key":"hidden"}),
            json!({"id":"C_ABSENT","key":"absent"}),
        ]);
        let raw = json!({"list_metadata":{"schema":schema,"views":[{"id":"V_FIELD_DELETE","columns":[],"sorts":[{"key":"hidden"}]},{"id":"V_FIELD_DELETE_BOARD","columns":[],"filters":[{"key":"hidden"}]},{"id":"V_FIELD_DELETE_OTHER","columns":[],"grouping":{"group_by":"hidden"}}],"integrations":null,"opaque":17}});
        let docs = documents(&raw).unwrap();
        assert_eq!(
            docs[1]["list_metadata"]["schema"].as_array().unwrap().len(),
            12
        );
        for (i, key) in [(0, "sorts"), (1, "filters"), (2, "grouping")] {
            assert_eq!(
                docs[1]["list_metadata"]["views"][i][key],
                raw["list_metadata"]["views"][i][key]
            );
        }
        assert_eq!(docs[1]["list_metadata"]["integrations"], Value::Null);
        assert_eq!(docs[1]["list_metadata"]["opaque"], 17);
    }
}
