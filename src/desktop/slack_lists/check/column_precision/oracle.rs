//! Independent literal request/response expectations, not rebuilt by the API under test.
use serde_json::{Value, json};
pub(super) const STAGES: [&str; 10] = [
    "initial",
    "editing",
    "saving",
    "uncertain",
    "conflict",
    "checking",
    "stale",
    "observed",
    "table",
    "board",
];
const COUNTS: [usize; 10] = [0, 0, 4, 4, 7, 8, 8, 11, 11, 11];
fn requests() -> Vec<Value> {
    let file = json!({"method":"POST","path":"/api/files.info","fields":[["token","fixture-not-a-token"],["file","F_LIST_FIXTURE"]]});
    let team = json!({"method":"POST","path":"/api/team.prefs.get","fields":[["token","fixture-not-a-token"],["target_team","T1"],["_x_reason","native-list-permission"]]});
    let user = json!({"method":"POST","path":"/api/users.prefs.get","fields":[["token","fixture-not-a-token"],["prefs","task_lists,assigned_to_list"],["_x_reason","native-list-completion-permission"]]});
    let write = json!({"method":"POST","path":"/api/lists.columns.update","fields":[["token","fixture-not-a-token"],["list_id","F_LIST_FIXTURE"],["key","estimate"],["options","{\"precision\":2}"],["backfill_missing_cells_with_default","true"],["_x_reason","update-column"]]});
    vec![
        file.clone(),
        team.clone(),
        user.clone(),
        write,
        file.clone(),
        team.clone(),
        user.clone(),
        file.clone(),
        file,
        team,
        user,
    ]
}
fn responses(before: &Value, expected: &Value) -> Vec<Value> {
    let file = json!({"ok":true,"file":before});
    let updated = json!({"ok":true,"file":expected});
    let team = json!({"ok":true,"prefs":{"allow_lists":"full"}});
    let user = json!({"ok":true,"prefs":{"task_lists":"[]","assigned_to_list":""}});
    vec![
        file.clone(),
        team.clone(),
        user.clone(),
        json!({"syntheticFailure":"response_lost"}),
        file,
        team.clone(),
        user.clone(),
        updated.clone(),
        updated,
        team,
        user,
    ]
}
pub(super) fn ledger(
    l: &Value,
    stage: usize,
    before: &Value,
    expected: &Value,
    serial: Option<u64>,
) -> Result<(), String> {
    let count = *COUNTS.get(stage).ok_or("Unknown precision stage")?;
    let req = requests();
    let resp = responses(before, expected);
    if l["requests"] != json!(&req[..count]) || l["responses"] != json!(&resp[..count]) {
        return Err(format!(
            "Exact precision requests/responses differ at {}",
            STAGES[stage]
        ));
    }
    if l["writePending"] != json!(stage == 2) || l["readPending"] != json!(stage == 5) {
        return Err("Wrong held response state".into());
    }
    if l["server"] != *if stage <= 2 { before } else { expected } {
        return Err("Synthetic server drift".into());
    }
    let finished = l["finished"]
        .as_array()
        .ok_or("Missing completion ledger")?;
    let n = match stage {
        0..=2 => 0,
        3 => 1,
        4..=5 => 2,
        6 => 3,
        _ => 4,
    };
    if finished.len() != n {
        return Err("Wrong completion count".into());
    }
    if n > 0 {
        let serial = serial
            .filter(|v| *v > 0)
            .ok_or("Missing real retained request serial")?;
        let expected = [
            json!({"serial":serial,"kind":"write","applied":false}),
            json!({"serial":serial,"kind":"check","applied":true}),
            json!({"serial":serial,"kind":"check","applied":false}),
            json!({"serial":serial,"kind":"check","applied":true}),
        ];
        if *finished != expected[..n] {
            return Err("Changed completion identity or admission".into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn final_ledger() -> Value {
        json!({"requests":requests(),"responses":responses(&json!({"precision":0}),&json!({"precision":2})),
            "writePending":false,"readPending":false,"server":{"precision":2},"finished":[
                {"serial":91,"kind":"write","applied":false},{"serial":91,"kind":"check","applied":true},
                {"serial":91,"kind":"check","applied":false},{"serial":91,"kind":"check","applied":true}]})
    }
    fn valid(v: &Value) -> bool {
        ledger(
            v,
            9,
            &json!({"precision":0}),
            &json!({"precision":2}),
            Some(91),
        )
        .is_ok()
    }
    #[test]
    fn exact_one_write_and_every_read_form_are_required() {
        assert!(valid(&final_ledger()));
        for (field, value) in [
            ("key", "C_NUMBER"),
            ("id", "C_NUMBER"),
            ("options", "{\"precision\":3}"),
            ("options", "{\"precision\":2.0}"),
        ] {
            let mut l = final_ledger();
            let index = if field == "options" { 3 } else { 2 };
            l["requests"][3]["fields"][index] = json!([field, value]);
            assert!(!valid(&l));
        }
        for index in [0, 1, 2, 4, 5, 6, 7, 8, 9, 10] {
            let mut l = final_ledger();
            l["requests"][index]["fields"]
                .as_array_mut()
                .unwrap()
                .push(json!(["extra", "x"]));
            assert!(!valid(&l));
        }
        let mut replay = final_ledger();
        let write = replay["requests"][3].clone();
        replay["requests"].as_array_mut().unwrap().push(write);
        assert!(!valid(&replay));
    }
    #[test]
    fn response_provenance_and_late_owner_refusal_are_not_count_only() {
        for index in [4, 7, 8] {
            let mut l = final_ledger();
            l["responses"][index]["file"]["precision"] = json!(99);
            assert!(!valid(&l));
        }
        for index in 0..4 {
            let mut l = final_ledger();
            l["finished"][index]["serial"] = json!(92);
            assert!(!valid(&l));
        }
        let mut l = final_ledger();
        l["finished"][2]["applied"] = json!(true);
        assert!(!valid(&l));
        let mut l = final_ledger();
        l["server"]["extra"] = json!(true);
        assert!(!valid(&l));
        let mut l = final_ledger();
        for entry in l["finished"].as_array_mut().unwrap() {
            entry["serial"] = json!(92);
        }
        assert!(!valid(&l));
    }
    #[test]
    fn every_stage_requires_its_exact_prefix_and_pending_boundary() {
        let full = final_ledger();
        for stage in 0..10 {
            let mut l = full.clone();
            let n = COUNTS[stage];
            l["requests"].as_array_mut().unwrap().truncate(n);
            l["responses"].as_array_mut().unwrap().truncate(n);
            let finished = match stage {
                0..=2 => 0,
                3 => 1,
                4..=5 => 2,
                6 => 3,
                _ => 4,
            };
            l["finished"].as_array_mut().unwrap().truncate(finished);
            l["writePending"] = json!(stage == 2);
            l["readPending"] = json!(stage == 5);
            if stage <= 2 {
                l["server"] = json!({"precision":0});
            }
            assert!(
                ledger(
                    &l,
                    stage,
                    &json!({"precision":0}),
                    &json!({"precision":2}),
                    Some(91)
                )
                .is_ok()
            );
            l["writePending"] = json!(stage != 2);
            assert!(
                ledger(
                    &l,
                    stage,
                    &json!({"precision":0}),
                    &json!({"precision":2}),
                    Some(91)
                )
                .is_err()
            );
        }
    }
}
