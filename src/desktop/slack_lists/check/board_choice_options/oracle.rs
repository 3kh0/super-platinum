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
pub(super) fn documents(before: &Value, action: &Value) -> Result<[Value; 2], String> {
    let add = action["kind"] == "add";
    if before["id"] != "F_LIST_FIXTURE"
        || before["title"]
            != if add {
                "Board option addition"
            } else {
                "Board option color"
            }
        || before["list_metadata"]["default_view"] != "V_LANE"
        || before["list_metadata"]["views"].as_array().map(Vec::len) != Some(3)
        || action["color"] != "orange"
    {
        return Err("Wrong controlled option scenario".into());
    }
    let mut after = before.clone();
    let column = after["list_metadata"]["schema"]
        .as_array_mut()
        .ok_or("Missing schema")?
        .iter_mut()
        .find(|c| c["id"] == "C_SELECT" && c["key"] == "status")
        .ok_or("Missing grouping column")?;
    let mut options = json!({"choices":[{"value":"OPT_PROGRESS","label":"In progress","color":"blue","opaque":{"rank":2,"label_hint":"Preserve raw"}},{"value":"OPT_DONE","label":"Done","color":"green"}],"opaque":{"keep":[1,null,true]}});
    if column["options"] != options {
        return Err("Original raw options differ".into());
    }
    if add {
        let value = action["value"]
            .as_str()
            .ok_or("Missing generated identity")?;
        if action["label"] != "Later"
            || super_platinum_core::slack_lists::SelectCreateOption::capture(
                "Later", value, "orange",
            )
            .is_err()
        {
            return Err("Wrong captured Add identity/name".into());
        }
        options["choices"]
            .as_array_mut()
            .unwrap()
            .push(json!({"value":value,"label":"Later","color":"orange"}));
    } else {
        if action["kind"] != "color"
            || action["value"] != "OPT_PROGRESS"
            || !action["label"].is_null()
        {
            return Err("Wrong Color intent".into());
        }
        options["choices"][0]["color"] = json!("orange");
    }
    column["options"] = options;
    Ok([before.clone(), after])
}
fn options(docs: &[Value; 2]) -> Value {
    docs[1]["list_metadata"]["schema"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == "C_SELECT")
        .unwrap()["options"]
        .clone()
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

pub(super) fn requests(docs: &[Value; 2]) -> Vec<Value> {
    let file = json!({"method":"POST","path":"/api/files.info","fields":[["token","fixture-not-a-token"],["file","F_LIST_FIXTURE"]]});
    let team = json!({"method":"POST","path":"/api/team.prefs.get","fields":[["token","fixture-not-a-token"],["target_team","T1"],["_x_reason","native-list-permission"]]});
    let user = json!({"method":"POST","path":"/api/users.prefs.get","fields":[["token","fixture-not-a-token"],["prefs","task_lists,assigned_to_list"],["_x_reason","native-list-completion-permission"]]});
    let page = json!({"method":"POST","path":"/api/lists.records.list","fields":[["token","fixture-not-a-token"],["list_id","F_LIST_FIXTURE"],["include_subtasks","true"],["archived","false"],["include_suggested","false"],["_x_reason","get-list-records"]]});
    let read = || vec![file.clone(), team.clone(), user.clone(), page.clone()];
    let mut out = vec![file.clone(), team.clone(), user.clone()];
    out.push(json!({"method":"POST","path":"/api/lists.columns.update","fields":[["token","fixture-not-a-token"],["list_id","F_LIST_FIXTURE"],["key","status"],["options",options(docs).to_string()],["backfill_missing_cells_with_default","true"],["_x_reason","update-column"]]}));
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
    let req = requests(docs);
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
    fn seed(add: bool) -> Value {
        json!({"id":"F_LIST_FIXTURE","title":if add {"Board option addition"}else{"Board option color"},
          "opaque":{"root":[null,true,7]},"list_metadata":{"default_view":"V_LANE",
          "views":[{"id":"A","opaque":3},{"id":"V_LANE","grouping":{"order":[{"select":[]},{"select":["OPT_PROGRESS"]}]}},{"id":"Z"}],
          "schema":[{"id":"C_SELECT","key":"status","type":"select","opaque":false,"options":{"choices":[{"value":"OPT_PROGRESS","label":"In progress","color":"blue","opaque":{"rank":2,"label_hint":"Preserve raw"}},{"value":"OPT_DONE","label":"Done","color":"green"}],"opaque":{"keep":[1,null,true]}}}]}})
    }
    fn action(add: bool) -> Value {
        if add {
            json!({"kind":"add","value":"OptABCD1234","label":"Later","color":"orange"})
        } else {
            json!({"kind":"color","value":"OPT_PROGRESS","label":null,"color":"orange"})
        }
    }
    #[test]
    fn independently_expected_add_and_color_preserve_complete_siblings() {
        for add in [true, false] {
            let before = seed(add);
            let docs = documents(&before, &action(add)).unwrap();
            let mut want = before.clone();
            let choices = &mut want["list_metadata"]["schema"][0]["options"]["choices"];
            if add {
                choices
                    .as_array_mut()
                    .unwrap()
                    .push(json!({"value":"OptABCD1234","label":"Later","color":"orange"}));
            } else {
                choices[0]["color"] = json!("orange");
            }
            assert_eq!(docs, [before, want]);
        }
    }
    #[test]
    fn refuses_wrong_action_identity_raw_options_and_scenario() {
        for add in [true, false] {
            for mutation in 0..5 {
                let mut before = seed(add);
                let mut action = action(add);
                match mutation {
                    0 => action["kind"] = json!("rename"),
                    1 => action["value"] = json!("foreign"),
                    2 => action["color"] = json!("blue"),
                    3 => {
                        before["list_metadata"]["schema"][0]["options"]["opaque"]["keep"][0] =
                            json!(true)
                    }
                    _ => before["title"] = json!("foreign"),
                }
                assert!(documents(&before, &action).is_err());
            }
        }
    }
    #[test]
    fn exact_ledger_refuses_extra_retired_read_and_false_completion() {
        let docs = documents(&seed(true), &action(true)).unwrap();
        let pages = [
            json!({"ok":true,"records":[]}),
            json!({"ok":true,"records":[]}),
        ];
        let done = json!([{"serial":7,"kind":"write","applied":false},{"serial":7,"kind":"check","applied":true},{"serial":7,"kind":"check","applied":false}]);
        let good = json!({"requests":&requests(&docs)[..9],"responses":&responses(&docs,&pages)[..9],"finished":done,"server":docs[1],"writePending":false,"readPending":false});
        assert!(stage(&good, 4, &docs, &pages, 7).is_ok());
        let mut extra = good.clone();
        extra["requests"]
            .as_array_mut()
            .unwrap()
            .push(requests(&docs)[10].clone());
        assert!(stage(&extra, 4, &docs, &pages, 7).is_err());
        let mut applied = good;
        applied["finished"][2]["applied"] = json!(true);
        assert!(stage(&applied, 4, &docs, &pages, 7).is_err());
    }
}
