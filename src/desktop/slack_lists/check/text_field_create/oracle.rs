//! Independent literal forms, responses, completions and expected visible order.
use super::rename_fixture::FieldCreateMode;
use serde_json::{Value, json};
pub(super) const STAGES: [&str; 7] = [
    "field-edit",
    "field-saving",
    "uncertain",
    "conflict",
    "retired",
    "observed",
    "table",
];
pub(super) fn order(_mode: usize) -> Vec<String> {
    [3, 1, 7, 5]
        .iter()
        .map(|i| format!("R_LIST_{i:03}"))
        .collect()
}
pub(super) fn documents(before: &Value, mode: FieldCreateMode) -> [Value; 2] {
    let (id, key, name, kind, options) = match mode {
        FieldCreateMode::Text => (
            "C_TEXT_OBSERVED",
            "text_observed",
            "Release notes",
            "text",
            Some(json!({"format":"text"})),
        ),
        FieldCreateMode::Number => (
            "C_NUMBER_OBSERVED",
            "number_observed",
            "Release count",
            "number",
            Some(json!({"precision":0})),
        ),
        FieldCreateMode::Checkbox => (
            "C_CHECKBOX_OBSERVED",
            "checkbox_observed",
            "Verified",
            "checkbox",
            None,
        ),
    };
    let mut after = before.clone();
    let mut column =
        json!({"id":id,"key":key,"name":name,"type":kind,"opaque":{"fixture":"retained"}});
    if let Some(options) = options {
        column["options"] = options;
    }
    after["list_metadata"]["schema"]
        .as_array_mut()
        .unwrap()
        .insert(1, column);
    after["list_metadata"]["views"][0]["columns"]
        .as_array_mut()
        .unwrap()
        .insert(1, json!({"id":id,"key":key,"visible":true}));
    [before.clone(), after]
}
fn requests(mode: FieldCreateMode) -> Vec<Value> {
    let (name, kind, options) = match mode {
        FieldCreateMode::Text => ("Release notes", "text", Some("{\"format\":\"text\"}")),
        FieldCreateMode::Number => ("Release count", "number", Some("{\"precision\":0}")),
        FieldCreateMode::Checkbox => ("Verified", "checkbox", None),
    };
    let file = json!({"method":"POST","path":"/api/files.info","fields":[["token","fixture-not-a-token"],["file","F_LIST_FIXTURE"]]});
    let team = json!({"method":"POST","path":"/api/team.prefs.get","fields":[["token","fixture-not-a-token"],["target_team","T1"],["_x_reason","native-list-permission"]]});
    let user = json!({"method":"POST","path":"/api/users.prefs.get","fields":[["token","fixture-not-a-token"],["prefs","task_lists,assigned_to_list"],["_x_reason","native-list-completion-permission"]]});
    let reads = || vec![file.clone(), team.clone(), user.clone()];
    let mut out = reads();
    let mut fields = vec![
        json!(["token", "fixture-not-a-token"]),
        json!(["list_id", "F_LIST_FIXTURE"]),
        json!(["view_id", "V_FILTER"]),
        json!(["name", name]),
        json!(["type", kind]),
    ];
    if let Some(options) = options {
        fields.push(json!(["options", options]));
    }
    fields.push(json!(["_x_reason", "create-column"]));
    out.push(json!({"method":"POST","path":"/api/lists.columns.create","fields":fields}));
    out.extend(reads());
    out.push(file.clone());
    out.extend(reads());
    out
}
#[cfg(test)]
mod checkbox_tests {
    use super::*;
    #[test]
    fn checkbox_create_omits_options_and_retains_one_request_with_ten_reads() {
        let docs = documents(
            &json!({"id":"F_LIST_FIXTURE","list_metadata":{"schema":[{"id":"old"}],"views":[{"id":"V_FILTER","columns":[{"id":"old"}]}]}}),
            FieldCreateMode::Checkbox,
        );
        assert!(
            docs[1]["list_metadata"]["schema"][1]
                .get("options")
                .is_none()
        );
        let id = 17;
        let base = json!({"requests":requests(FieldCreateMode::Checkbox),"responses":responses(&docs),"finished":completions(id),"server":docs[1],"writePending":false,"readPending":false});
        exact(
            &base,
            11,
            4,
            false,
            false,
            1,
            &docs,
            id,
            FieldCreateMode::Checkbox,
        )
        .unwrap();
        assert_eq!(base["requests"][3]["fields"].as_array().unwrap().len(), 6);
        for option in [json!(null), json!("{}"), json!("null")] {
            let mut bad = base.clone();
            bad["requests"][3]["fields"]
                .as_array_mut()
                .unwrap()
                .insert(5, json!(["options", option]));
            assert!(
                exact(
                    &bad,
                    11,
                    4,
                    false,
                    false,
                    1,
                    &docs,
                    id,
                    FieldCreateMode::Checkbox
                )
                .is_err()
            );
        }
        let mut bad = base.clone();
        bad["server"]["list_metadata"]["schema"][1]["options"] = json!(null);
        assert!(
            exact(
                &bad,
                11,
                4,
                false,
                false,
                1,
                &docs,
                id,
                FieldCreateMode::Checkbox
            )
            .is_err()
        );
    }
}
fn responses(docs: &[Value; 2]) -> Vec<Value> {
    let file = |i: usize| json!({"ok":true,"file":docs[i]});
    let team = json!({"ok":true,"prefs":{"allow_lists":"full"}});
    let user = json!({"ok":true,"prefs":{"task_lists":"[]","assigned_to_list":""}});
    let reads = |i| vec![file(i), team.clone(), user.clone()];
    let mut out = reads(0);
    out.push(json!({"syntheticFailure":"response_lost"}));
    out.extend(reads(0));
    out.push(file(1));
    out.extend(reads(1));
    out
}
fn completions(id: u64) -> Vec<Value> {
    [
        ("write", false),
        ("check", true),
        ("check", false),
        ("check", true),
    ]
    .into_iter()
    .map(|(kind, applied)| json!({"serial":id,"kind":kind,"applied":applied}))
    .collect()
}
pub(super) fn exact(
    l: &Value,
    count: usize,
    finished: usize,
    pending: bool,
    read: bool,
    server: usize,
    docs: &[Value; 2],
    id: u64,
    mode: FieldCreateMode,
) -> Result<(), String> {
    if id == 0 {
        return Err("Missing Text attempt identity".into());
    }
    let req = requests(mode);
    let res = responses(docs);
    let done = completions(id);
    if count > req.len() || finished > done.len() || server > 1 {
        return Err("Unknown Text creation checkpoint".into());
    }
    if l["requests"] != json!(&req[..count])
        || l["responses"] != json!(&res[..count])
        || l["finished"] != json!(&done[..finished])
        || l["writePending"] != pending
        || l["readPending"] != read
        || l["server"] != docs[server]
    {
        return Err("Exact Text creation form/response/completion ledger differs".into());
    }
    Ok(())
}
pub(super) fn stage(
    l: &Value,
    index: usize,
    docs: &[Value; 2],
    id: u64,
    mode: FieldCreateMode,
) -> Result<(), String> {
    let &(count, finished, server) = [
        (0, 0, 0),
        (4, 0, 0),
        (4, 1, 1),
        (7, 2, 1),
        (8, 3, 1),
        (11, 4, 1),
        (11, 4, 1),
    ]
    .get(index)
    .ok_or("Unknown Text creation stage")?;
    exact(
        l,
        count,
        finished,
        index == 1,
        false,
        server,
        docs,
        id,
        mode,
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_one_create_rejects_replay_invented_identity_and_retired_projection() {
        let docs = documents(
            &json!({"id":"F_LIST_FIXTURE","list_metadata":{"schema":[{"id":"old"}],"views":[{"id":"V_FILTER","columns":[{"id":"old"}],"sorts":[{"key":"estimate","ascending":true}],"filters":[{"key":"done","operator":"is_not_empty","values":[]}],"info_column_filters":[]}]}}),
            FieldCreateMode::Text,
        );
        let id = 17;
        let base = json!({"requests":requests(FieldCreateMode::Text),"responses":responses(&docs),"finished":completions(id),"server":docs[1],"writePending":false,"readPending":false});
        exact(
            &base,
            11,
            4,
            false,
            false,
            1,
            &docs,
            id,
            FieldCreateMode::Text,
        )
        .unwrap();
        for branch in 0..5 {
            let mut bad = base.clone();
            match branch {
                0 => {
                    bad["requests"][3]["fields"]
                        .as_array_mut()
                        .unwrap()
                        .push(json!(["id", "C_TEXT_OBSERVED"]));
                }
                1 => {
                    bad["requests"]
                        .as_array_mut()
                        .unwrap()
                        .push(requests(FieldCreateMode::Text)[3].clone());
                }
                2 => bad["finished"][2]["applied"] = json!(true),
                3 => bad["responses"][4]["file"] = docs[1].clone(),
                _ => bad["server"]["list_metadata"]["views"][0]["sorts"] = json!([]),
            };
            assert!(
                exact(
                    &bad,
                    11,
                    4,
                    false,
                    false,
                    1,
                    &docs,
                    id,
                    FieldCreateMode::Text
                )
                .is_err()
            );
        }
        assert_eq!(
            requests(FieldCreateMode::Text)
                .iter()
                .filter(|r| r["path"] == "/api/lists.columns.create")
                .count(),
            1
        );
        assert_eq!(requests(FieldCreateMode::Text).len(), 11);
    }
}

#[cfg(test)]
mod number_tests {
    use super::*;
    #[test]
    fn exact_number_mode_refuses_text_options_replay_and_extra_fields() {
        let docs = documents(
            &json!({"list_metadata":{"schema":[{"id":"old"}],"views":[{"columns":[{"id":"old"}]}]}}),
            FieldCreateMode::Number,
        );
        let id = 29;
        let base = json!({"requests":requests(FieldCreateMode::Number),"responses":responses(&docs),"finished":completions(id),"server":docs[1],"writePending":false,"readPending":false});
        exact(
            &base,
            11,
            4,
            false,
            false,
            1,
            &docs,
            id,
            FieldCreateMode::Number,
        )
        .unwrap();
        assert_eq!(
            base["requests"][3]["fields"],
            json!([
                ["token", "fixture-not-a-token"],
                ["list_id", "F_LIST_FIXTURE"],
                ["view_id", "V_FILTER"],
                ["name", "Release count"],
                ["type", "number"],
                ["options", "{\"precision\":0}"],
                ["_x_reason", "create-column"]
            ])
        );
        for branch in 0..5 {
            let mut bad = base.clone();
            match branch {
                0 => bad["requests"][3]["fields"][5][1] = json!("{\"format\":\"text\"}"),
                1 => bad["requests"][3]["fields"][4][1] = json!("text"),
                2 => bad["requests"][3]["fields"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!(["position", "end"])),
                3 => bad["requests"]
                    .as_array_mut()
                    .unwrap()
                    .push(base["requests"][3].clone()),
                _ => bad["finished"][2]["applied"] = json!(true),
            }
            assert!(
                exact(
                    &bad,
                    11,
                    4,
                    false,
                    false,
                    1,
                    &docs,
                    id,
                    FieldCreateMode::Number
                )
                .is_err()
            );
        }
    }
}
