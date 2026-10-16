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
    documents_with_options(before, mode, None)
}
pub(super) fn documents_with_options(
    before: &Value,
    mode: FieldCreateMode,
    select: Option<&Value>,
) -> [Value; 2] {
    let (id, key, name, kind, options) = match mode {
        FieldCreateMode::Select | FieldCreateMode::MultiSelect => (
            "C_SELECT_OBSERVED",
            "select_observed",
            "Release phase",
            "select",
            Some(
                select
                    .expect("Select oracle needs independently captured options")
                    .clone(),
            ),
        ),
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
        FieldCreateMode::Date => (
            "C_DATE_OBSERVED",
            "date_observed",
            "Target date",
            "date",
            Some(json!({"format":"single","date_format":"default"})),
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
    requests_with_options(mode, None)
}
fn requests_with_options(mode: FieldCreateMode, select: Option<&Value>) -> Vec<Value> {
    let wire = select.map(Value::to_string);
    let (name, kind, options) = match mode {
        FieldCreateMode::Select | FieldCreateMode::MultiSelect => (
            "Release phase",
            "select",
            Some(
                wire.as_deref()
                    .expect("Select request oracle needs owned options"),
            ),
        ),
        FieldCreateMode::Text => ("Release notes", "text", Some("{\"format\":\"text\"}")),
        FieldCreateMode::Number => ("Release count", "number", Some("{\"precision\":0}")),
        FieldCreateMode::Checkbox => ("Verified", "checkbox", None),
        FieldCreateMode::Date => (
            "Target date",
            "date",
            Some("{\"date_format\":\"default\",\"format\":\"single\"}"),
        ),
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
    exact_with_options(
        l, count, finished, pending, read, server, docs, id, mode, None,
    )
}
pub(super) fn exact_with_options(
    l: &Value,
    count: usize,
    finished: usize,
    pending: bool,
    read: bool,
    server: usize,
    docs: &[Value; 2],
    id: u64,
    mode: FieldCreateMode,
    select: Option<&Value>,
) -> Result<(), String> {
    if id == 0 {
        return Err("Missing Text attempt identity".into());
    }
    let req = requests_with_options(mode, select);
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
    stage_with_options(l, index, docs, id, mode, None)
}
pub(super) fn stage_with_options(
    l: &Value,
    index: usize,
    docs: &[Value; 2],
    id: u64,
    mode: FieldCreateMode,
    select: Option<&Value>,
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
    exact_with_options(
        l,
        count,
        finished,
        index == 1,
        false,
        server,
        docs,
        id,
        mode,
        select,
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

#[cfg(test)]
mod date_tests {
    use super::*;
    #[test]
    fn date_create_literal_is_seven_fields_and_full_exact_options() {
        let calls = requests(FieldCreateMode::Date);
        assert_eq!(calls.len(), 11);
        assert_eq!(
            calls
                .iter()
                .filter(|r| r["path"] == "/api/lists.columns.create")
                .count(),
            1
        );
        assert_eq!(
            calls[3]["fields"],
            json!([
                ["token", "fixture-not-a-token"],
                ["list_id", "F_LIST_FIXTURE"],
                ["view_id", "V_FILTER"],
                ["name", "Target date"],
                ["type", "date"],
                ["options", r#"{"date_format":"default","format":"single"}"#],
                ["_x_reason", "create-column"]
            ])
        );
        let docs = documents(
            &json!({"list_metadata":{"schema":[{"id":"old"}],"views":[{"columns":[{"id":"old"}]}]}}),
            FieldCreateMode::Date,
        );
        assert_eq!(
            docs[1]["list_metadata"]["schema"][1]["options"],
            json!({"date_format":"default","format":"single"})
        );
        assert_eq!(
            docs[1]["list_metadata"]["views"][0]["columns"][1],
            json!({"id":"C_DATE_OBSERVED","key":"date_observed","visible":true})
        );
        let id = 29;
        let base = json!({"requests":calls,"responses":responses(&docs),"finished":completions(id),"server":docs[1],"writePending":false,"readPending":false});
        exact(
            &base,
            11,
            4,
            false,
            false,
            1,
            &docs,
            id,
            FieldCreateMode::Date,
        )
        .unwrap();
        for change in 0..4 {
            let mut bad = base.clone();
            match change {
                0 => {
                    bad["requests"][3]["fields"][5][1] =
                        json!(r#"{"dateFormat":"default","format":"single"}"#)
                }
                1 => {
                    bad["requests"][3]["fields"][5][1] =
                        json!(r#"{"date_format":"default","format":"single","timezone":"UTC"}"#)
                }
                2 => bad["requests"][3]["fields"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!(["position", "end"])),
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
                    FieldCreateMode::Date
                )
                .is_err()
            );
        }
    }
}

#[cfg(test)]
mod select_tests {
    use super::*;
    #[test]
    fn select_wire_uses_independent_ordered_dom_options_and_refuses_drift() {
        for mode in [FieldCreateMode::Select, FieldCreateMode::MultiSelect] {
            let options = json!({"format":mode.select_format(),"choices":[{"label":"Ready","value":"OptAAAAAAA0","color":"blue"},{"label":"\u{c9}quipe \u{6771}\u{4eac}","value":"OptAAAAAAA1","color":"cyan"}]});
            let calls = requests_with_options(mode, Some(&options));
            assert_eq!(calls.len(), 11);
            assert_eq!(
                calls[3]["fields"],
                json!([
                    ["token", "fixture-not-a-token"],
                    ["list_id", "F_LIST_FIXTURE"],
                    ["view_id", "V_FILTER"],
                    ["name", "Release phase"],
                    ["type", "select"],
                    ["options", serde_json::to_string(&options).unwrap()],
                    ["_x_reason", "create-column"]
                ])
            );
            let before = json!({"opaque":{"keep":1},"list_metadata":{"schema":[{"id":"old"}],"views":[{"columns":[{"id":"old"}]}]}});
            let docs = documents_with_options(&before, mode, Some(&options));
            assert_eq!(docs[1]["list_metadata"]["schema"][1]["options"], options);
            let id = 29;
            let ledger = json!({"requests":calls,"responses":responses(&docs),"finished":completions(id),"server":docs[1],"writePending":false,"readPending":false});
            let verify = |v: &Value| {
                exact_with_options(v, 11, 4, false, false, 1, &docs, id, mode, Some(&options))
            };
            verify(&ledger).unwrap();
            for n in 0..5 {
                let mut bad = ledger.clone();
                match n {
                    0 => {
                        let mut altered = options.clone();
                        altered["choices"].as_array_mut().unwrap().reverse();
                        bad["requests"][3]["fields"][5][1] =
                            json!(serde_json::to_string(&altered).unwrap());
                    }
                    1 => bad["requests"][3]["fields"]
                        .as_array_mut()
                        .unwrap()
                        .push(json!(["position", "end"])),
                    2 => {
                        bad["server"]["list_metadata"]["schema"][1]["options"]["format"] =
                            json!(if mode == FieldCreateMode::Select {
                                "multi_select"
                            } else {
                                "single_select"
                            })
                    }
                    3 => bad["finished"][2]["applied"] = json!(true),
                    _ => bad["requests"][3]["retrySafe"] = json!(true),
                }
                assert!(verify(&bad).is_err());
            }
            let mut stripped = docs[1].clone();
            stripped["list_metadata"]["schema"]
                .as_array_mut()
                .unwrap()
                .remove(1);
            stripped["list_metadata"]["views"][0]["columns"]
                .as_array_mut()
                .unwrap()
                .remove(1);
            assert_eq!(stripped, before);
        }
    }
}
