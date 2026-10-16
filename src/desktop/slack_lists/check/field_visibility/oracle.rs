//! Literal form and response oracle independent of the production permit builder.
use serde_json::{Value, json};
pub(super) const STAGES: [&str; 7] = [
    "hide-edit",
    "hide-saving",
    "uncertain",
    "stale",
    "hidden",
    "show-edit",
    "shown",
];
const COUNTS: [usize; 7] = [0, 4, 4, 8, 11, 11, 18];
fn requests() -> Vec<Value> {
    let file = json!({"method":"POST","path":"/api/files.info","fields":[["token","fixture-not-a-token"],["file","F_LIST_FIXTURE"]]});
    let team = json!({"method":"POST","path":"/api/team.prefs.get","fields":[["token","fixture-not-a-token"],["target_team","T1"],["_x_reason","native-list-permission"]]});
    let user = json!({"method":"POST","path":"/api/users.prefs.get","fields":[["token","fixture-not-a-token"],["prefs","task_lists,assigned_to_list"],["_x_reason","native-list-completion-permission"]]});
    let write = |visible| json!({"method":"POST","path":"/api/lists.views.update","fields":[["token","fixture-not-a-token"],["list_id","F_LIST_FIXTURE"],["id","V_PRECISION"],["columns",if visible {"[{\"id\":\"C_NUMBER\",\"key\":\"estimate\",\"visible\":true}]"}else{"[{\"id\":\"C_NUMBER\",\"key\":\"estimate\",\"visible\":false}]"}],["_x_reason","update-view"]]});
    vec![
        file.clone(),
        team.clone(),
        user.clone(),
        write(false),
        file.clone(),
        team.clone(),
        user.clone(),
        file.clone(),
        file.clone(),
        team.clone(),
        user.clone(),
        file.clone(),
        team.clone(),
        user.clone(),
        write(true),
        file,
        team,
        user,
    ]
}
fn responses(before: &Value, hidden: &Value) -> Vec<Value> {
    let old = json!({"ok":true,"file":before});
    let hidden = json!({"ok":true,"file":hidden});
    let team = json!({"ok":true,"prefs":{"allow_lists":"full"}});
    let user = json!({"ok":true,"prefs":{"task_lists":"[]","assigned_to_list":""}});
    let ack = json!({"ok":true,"list":{"id":before["id"],"list_metadata":before["list_metadata"]},"opaque_reply":{"visibility":true}});
    vec![
        old.clone(),
        team.clone(),
        user.clone(),
        json!({"syntheticFailure":"response_lost"}),
        old.clone(),
        team.clone(),
        user.clone(),
        hidden.clone(),
        hidden.clone(),
        team.clone(),
        user.clone(),
        hidden,
        team.clone(),
        user.clone(),
        ack,
        old,
        team,
        user,
    ]
}
fn completions(hide: u64, show: u64) -> Vec<Value> {
    vec![
        json!({"serial":hide,"kind":"write","applied":false}),
        json!({"serial":hide,"kind":"check","applied":true}),
        json!({"serial":hide,"kind":"check","applied":false}),
        json!({"serial":hide,"kind":"check","applied":true}),
        json!({"serial":show,"kind":"write","applied":false}),
        json!({"serial":show,"kind":"check","applied":true}),
    ]
}
pub(super) fn ledger(
    l: &Value,
    stage: usize,
    before: &Value,
    hidden: &Value,
    hide: u64,
    show: Option<u64>,
) -> Result<(), String> {
    let count = *COUNTS.get(stage).ok_or("Unknown visibility stage")?;
    let finished = [0, 0, 1, 3, 4, 4, 6][stage];
    exact(
        l,
        count,
        finished,
        stage == 1,
        false,
        if stage <= 1 || stage == 6 {
            before
        } else {
            hidden
        },
        before,
        hidden,
        hide,
        show,
    )
}
pub(super) fn exact(
    l: &Value,
    count: usize,
    finished: usize,
    write: bool,
    read: bool,
    server: &Value,
    before: &Value,
    hidden: &Value,
    hide: u64,
    show: Option<u64>,
) -> Result<(), String> {
    if hide == 0 || finished > 4 && show.is_none_or(|s| s == 0 || s == hide) {
        return Err("Missing distinct real attempt serials".into());
    }
    if l["requests"] != json!(&requests()[..count])
        || l["responses"] != json!(&responses(before, hidden)[..count])
        || l["finished"] != json!(&completions(hide, show.unwrap_or(0))[..finished])
        || l["writePending"] != write
        || l["readPending"] != read
        || l["server"] != *server
    {
        return Err("Exact visibility request/response/owner completion ledger differs".into());
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    fn final_ledger() -> Value {
        json!({"requests":requests(),"responses":responses(&json!({"visible":true}),&json!({"visible":false})),"finished":completions(71,72),"writePending":false,"readPending":false,"server":{"visible":true}})
    }
    fn valid(v: &Value) -> bool {
        ledger(
            v,
            6,
            &json!({"visible":true}),
            &json!({"visible":false}),
            71,
            Some(72),
        )
        .is_ok()
    }
    #[test]
    fn exact_two_boolean_forms_and_no_extra_write() {
        assert!(valid(&final_ledger()));
        for (index, value) in [
            (
                3,
                "[{\"id\":\"C_NUMBER\",\"key\":\"estimate\",\"visible\":true}]",
            ),
            (
                14,
                "[{\"id\":\"C_NUMBER\",\"key\":\"estimate\",\"visible\":false}]",
            ),
        ] {
            let mut l = final_ledger();
            l["requests"][index]["fields"][3][1] = json!(value);
            assert!(!valid(&l));
        }
        let mut l = final_ledger();
        let write = l["requests"][3].clone();
        l["requests"].as_array_mut().unwrap().push(write);
        assert!(!valid(&l));
    }
    #[test]
    fn stale_owner_and_ack_provenance_are_not_counts() {
        let mut l = final_ledger();
        l["finished"][2]["applied"] = json!(true);
        assert!(!valid(&l));
        let mut l = final_ledger();
        l["finished"][4]["serial"] = json!(71);
        assert!(!valid(&l));
        let mut l = final_ledger();
        l["responses"][14] = json!({"ok":true});
        assert!(!valid(&l));
        let mut l = final_ledger();
        l["responses"][7]["file"]["visible"] = json!(true);
        assert!(!valid(&l));
    }
}
