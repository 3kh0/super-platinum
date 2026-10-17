//! Independent literal forms, responses, completions and expected visible order.
use super::rename_fixture::FieldCreateMode;
use serde_json::{Value, json};
mod entity;
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
// Literal oracle choices do not call the fixture option constructor.
fn rating_options(mode: FieldCreateMode) -> Value {
    match mode {
        FieldCreateMode::RatingDefault => json!({"emoji":":star:","max":1}),
        FieldCreateMode::RatingStandard => json!({"emoji":":heart:","max":5,"emoji_team_id":"T1"}),
        FieldCreateMode::RatingCustom => {
            json!({"emoji":":fixture_quality:","max":3,"emoji_team_id":"T1"})
        }
        _ => unreachable!(),
    }
}
fn vote_options(mode: FieldCreateMode) -> Value {
    match mode {
        FieldCreateMode::VoteDefault => json!({"emoji":":sparkling_heart:"}),
        FieldCreateMode::VoteCustom => json!({"emoji":":fixture_vote:","emoji_team_id":"T1"}),
        _ => unreachable!(),
    }
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
        FieldCreateMode::Entity(case) => (
            mode.observed().0,
            mode.observed().1,
            mode.name(),
            mode.kind().as_str(),
            Some(entity::options(case)),
        ),
        FieldCreateMode::VoteDefault | FieldCreateMode::VoteCustom => (
            "C_VOTE_OBSERVED",
            "vote_observed",
            "Support Équipe 東京",
            "vote",
            Some(vote_options(mode)),
        ),
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
        FieldCreateMode::Link => (
            "C_LINK_OBSERVED",
            "link_observed",
            "Reference \u{c9}quipe \u{6771}\u{4eac}",
            "link",
            None,
        ),
        FieldCreateMode::People => (
            "C_PEOPLE_OBSERVED",
            "people_observed",
            "Reviewer Équipe 東京",
            "user",
            Some(json!({"format":"single_entity","show_member_name":true,"default_value":null})),
        ),
        FieldCreateMode::Channel => (
            "C_CHANNEL_OBSERVED",
            "channel_observed",
            "Channel Équipe 東京",
            "channel",
            Some(json!({"format":"single_entity","show_member_name":true,"default_value":null})),
        ),
        FieldCreateMode::RatingDefault
        | FieldCreateMode::RatingStandard
        | FieldCreateMode::RatingCustom => (
            "C_RATING_OBSERVED",
            "rating_observed",
            "Quality",
            "rating",
            Some(rating_options(mode)),
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
        FieldCreateMode::NumberPrecision => (
            "C_NUMBER_OBSERVED",
            "number_observed",
            "Release count",
            "number",
            Some(json!({"precision":3})),
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
    if mode.is_rating() || mode.is_vote() {
        let board_id = if mode.is_rating() {
            "V_RATING_BOARD"
        } else {
            "V_VOTE_BOARD"
        };
        let views = after["list_metadata"]["views"]
            .as_array_mut()
            .expect("Fixture views");
        assert_eq!(
            views.iter().filter(|view| view["id"] == board_id).count(),
            1,
            "Unique named diagnostic Board"
        );
        let board = views
            .iter_mut()
            .find(|view| view["id"] == board_id)
            .unwrap();
        board["columns"]
            .as_array_mut()
            .expect("Rating Board")
            .insert(1, json!({"id":id,"key":key,"visible":true}));
    }
    [before.clone(), after]
}
fn requests(mode: FieldCreateMode) -> Vec<Value> {
    requests_with_options(mode, None)
}
fn requests_with_options(mode: FieldCreateMode, select: Option<&Value>) -> Vec<Value> {
    let wire = if let FieldCreateMode::Entity(case) = mode {
        Some(entity::options(case).to_string())
    } else if mode.is_vote() {
        Some(vote_options(mode).to_string())
    } else if mode.is_rating() {
        Some(rating_options(mode).to_string())
    } else {
        select.map(Value::to_string)
    };
    let (name, kind, options) = match mode {
        FieldCreateMode::Entity(_) => (mode.name(), mode.kind().as_str(), wire.as_deref()),
        FieldCreateMode::Select | FieldCreateMode::MultiSelect => (
            "Release phase",
            "select",
            Some(
                wire.as_deref()
                    .expect("Select request oracle needs owned options"),
            ),
        ),
        FieldCreateMode::RatingDefault
        | FieldCreateMode::RatingStandard
        | FieldCreateMode::RatingCustom => ("Quality", "rating", wire.as_deref()),
        FieldCreateMode::VoteDefault | FieldCreateMode::VoteCustom => {
            ("Support Équipe 東京", "vote", wire.as_deref())
        }
        FieldCreateMode::Text => ("Release notes", "text", Some("{\"format\":\"text\"}")),
        FieldCreateMode::Number => ("Release count", "number", Some("{\"precision\":0}")),
        FieldCreateMode::NumberPrecision => ("Release count", "number", Some("{\"precision\":3}")),
        FieldCreateMode::Checkbox => ("Verified", "checkbox", None),
        FieldCreateMode::Link => ("Reference \u{c9}quipe \u{6771}\u{4eac}", "link", None),
        FieldCreateMode::People => (
            "Reviewer Équipe 東京",
            "user",
            Some("{\"default_value\":null,\"format\":\"single_entity\",\"show_member_name\":true}"),
        ),
        FieldCreateMode::Channel => (
            "Channel Équipe 東京",
            "channel",
            Some("{\"default_value\":null,\"format\":\"single_entity\",\"show_member_name\":true}"),
        ),
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

#[cfg(test)]
mod people_tests {
    use super::*;
    #[test]
    fn exact_people_forms_and_observation_keep_three_options_and_old_metadata() {
        let forms = requests(FieldCreateMode::People);
        assert_eq!(forms.len(), 11);
        assert_eq!(
            forms
                .iter()
                .filter(|r| r["path"] == "/api/lists.columns.create")
                .count(),
            1
        );
        assert_eq!(
            forms[3]["fields"],
            json!([
                ["token", "fixture-not-a-token"],
                ["list_id", "F_LIST_FIXTURE"],
                ["view_id", "V_FILTER"],
                ["name", "Reviewer \u{00c9}quipe \u{6771}\u{4eac}"],
                ["type", "user"],
                [
                    "options",
                    r#"{"default_value":null,"format":"single_entity","show_member_name":true}"#
                ],
                ["_x_reason", "create-column"]
            ])
        );
        let before = json!({"list_metadata":{"schema":[{"id":"old","opaque":7}],"views":[{"columns":[{"id":"old"}],"opaque":[1,2]}]},"opaque":"retained"});
        let docs = documents(&before, FieldCreateMode::People);
        assert_eq!(docs[0], before);
        assert_eq!(
            docs[1]["list_metadata"]["schema"][1]["options"],
            json!({"format":"single_entity","show_member_name":true,"default_value":null})
        );
        let mut restored = docs[1].clone();
        restored["list_metadata"]["schema"]
            .as_array_mut()
            .unwrap()
            .remove(1);
        restored["list_metadata"]["views"][0]["columns"]
            .as_array_mut()
            .unwrap()
            .remove(1);
        assert_eq!(restored, before);
    }
}

#[cfg(test)]
mod link_tests {
    use super::*;
    #[test]
    fn link_six_field_request_omits_options_and_only_adds_observed_identity() {
        let forms = requests(FieldCreateMode::Link);
        assert_eq!(forms.len(), 11);
        assert_eq!(
            forms
                .iter()
                .filter(|r| r["path"] == "/api/lists.columns.create")
                .count(),
            1
        );
        assert_eq!(
            forms[3]["fields"],
            json!([
                ["token", "fixture-not-a-token"],
                ["list_id", "F_LIST_FIXTURE"],
                ["view_id", "V_FILTER"],
                ["name", "Reference \u{c9}quipe \u{6771}\u{4eac}"],
                ["type", "link"],
                ["_x_reason", "create-column"]
            ])
        );
        let before = json!({"list_metadata":{"schema":[{"id":"old","opaque":7}],"views":[{"columns":[{"id":"old"}],"opaque":[1,2]}]},"opaque":"retained"});
        let docs = documents(&before, FieldCreateMode::Link);
        assert_eq!(docs[0], before);
        assert!(
            docs[1]["list_metadata"]["schema"][1]
                .get("options")
                .is_none()
        );
        let mut restored = docs[1].clone();
        restored["list_metadata"]["schema"]
            .as_array_mut()
            .unwrap()
            .remove(1);
        restored["list_metadata"]["views"][0]["columns"]
            .as_array_mut()
            .unwrap()
            .remove(1);
        assert_eq!(restored, before);
    }
}

#[cfg(test)]
mod configured_number_tests {
    use super::*;
    #[test]
    fn exact_configured_form_changes_only_precision_from_legacy() {
        let old = requests(FieldCreateMode::Number);
        let mut configured = requests(FieldCreateMode::NumberPrecision);
        assert_eq!(configured.len(), 11);
        let create = configured
            .iter_mut()
            .find(|r| r["path"] == "/api/lists.columns.create")
            .unwrap();
        assert_eq!(create["fields"][5], json!(["options", "{\"precision\":3}"]));
        create["fields"][5] = json!(["options", "{\"precision\":0}"]);
        assert_eq!(configured, old);
    }
}

#[cfg(test)]
mod rating_tests {
    use super::*;
    #[test]
    fn three_literal_rating_forms_preserve_ten_reads_and_exact_optional_team() {
        for (mode, options) in [
            (
                FieldCreateMode::RatingDefault,
                json!({"emoji":":star:","max":1}),
            ),
            (
                FieldCreateMode::RatingStandard,
                json!({"emoji":":heart:","max":5,"emoji_team_id":"T1"}),
            ),
            (
                FieldCreateMode::RatingCustom,
                json!({"emoji":":fixture_quality:","max":3,"emoji_team_id":"T1"}),
            ),
        ] {
            let actual = requests(mode);
            assert_eq!(actual.len(), 11);
            let writes: Vec<_> = actual
                .iter()
                .filter(|r| r["path"] == "/api/lists.columns.create")
                .collect();
            assert_eq!(writes.len(), 1);
            assert_eq!(writes[0]["fields"].as_array().unwrap().len(), 7);
            assert_eq!(
                writes[0]["fields"][5],
                json!(["options", options.to_string()])
            );
            let mut retained_reads: Vec<_> = actual
                .into_iter()
                .filter(|r| r["path"] != "/api/lists.columns.create")
                .collect();
            let legacy_reads: Vec<_> = requests(FieldCreateMode::Text)
                .into_iter()
                .filter(|r| r["path"] != "/api/lists.columns.create")
                .collect();
            assert_eq!(retained_reads.len(), 10);
            assert_eq!(retained_reads, legacy_reads);
            retained_reads.clear();
        }
    }
}

#[cfg(test)]
mod vote_tests {
    use super::*;
    #[test]
    fn two_vote_forms_are_literal_unicode_and_never_carry_rating_quantity() {
        for (mode, wire) in [
            (
                FieldCreateMode::VoteDefault,
                "{\"emoji\":\":sparkling_heart:\"}",
            ),
            (
                FieldCreateMode::VoteCustom,
                "{\"emoji\":\":fixture_vote:\",\"emoji_team_id\":\"T1\"}",
            ),
        ] {
            let all = requests(mode);
            let writes: Vec<_> = all
                .iter()
                .filter(|r| r["path"] == "/api/lists.columns.create")
                .collect();
            assert_eq!(all.len(), 11);
            assert_eq!(writes.len(), 1);
            assert_eq!(
                writes[0]["fields"],
                json!([
                    ["token", "fixture-not-a-token"],
                    ["list_id", "F_LIST_FIXTURE"],
                    ["view_id", "V_FILTER"],
                    ["name", "Support Équipe 東京"],
                    ["type", "vote"],
                    ["options", wire],
                    ["_x_reason", "create-column"]
                ])
            );
            let reads: Vec<_> = all
                .iter()
                .filter(|r| r["path"] != "/api/lists.columns.create")
                .cloned()
                .collect();
            let old: Vec<_> = requests(FieldCreateMode::Text)
                .into_iter()
                .filter(|r| r["path"] != "/api/lists.columns.create")
                .collect();
            assert_eq!(reads, old);
            assert_eq!(reads.len(), 10);
        }
    }
}

#[cfg(test)]
mod named_board_tests {
    use super::*;
    #[test]
    fn created_reference_targets_named_board_and_preserves_existing_board() {
        for (mode, board) in [
            (FieldCreateMode::RatingDefault, "V_RATING_BOARD"),
            (FieldCreateMode::VoteDefault, "V_VOTE_BOARD"),
        ] {
            let before = json!({"list_metadata":{"schema":[{"id":"C_NAME","key":"NAME"}],"views":[{"id":"V_FILTER","columns":[{"id":"C_NAME"}]},{"id":"V_PRECISION_BOARD","columns":[{"id":"C_NAME"}],"opaque":{"untouched":true}},{"id":board,"columns":[{"id":"C_NAME"}]}]}});
            let mut after = documents(&before, mode)[1].clone();
            assert_eq!(
                after["list_metadata"]["views"][1],
                before["list_metadata"]["views"][1]
            );
            let reference = json!({"id":if mode.is_vote(){"C_VOTE_OBSERVED"}else{"C_RATING_OBSERVED"},"key":if mode.is_vote(){"vote_observed"}else{"rating_observed"},"visible":true});
            for i in [0, 2] {
                assert_eq!(
                    after["list_metadata"]["views"][i]["columns"]
                        .as_array_mut()
                        .unwrap()
                        .remove(1),
                    reference
                );
            }
            after["list_metadata"]["schema"]
                .as_array_mut()
                .unwrap()
                .remove(1);
            assert_eq!(after, before);
        }
    }
}
