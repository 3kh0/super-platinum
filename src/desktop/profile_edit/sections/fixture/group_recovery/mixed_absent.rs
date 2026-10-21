//! Exact closed mixed cases: no canonical or native missing-member seed.
use super::*;
use std::collections::BTreeSet;

pub(crate) use super::super::member_recovery::{LABEL, MEMBER};

fn seed() -> &'static OnceLock<Value> {
    static SEED: OnceLock<Value> = OnceLock::new();
    &SEED
}
pub(super) fn record_seed(boot: &Value, init: &Value) -> Result<(), String> {
    if !enabled() {
        return Err("Closed mixed absent-member seed required".into());
    }
    let users = boot["users"]
        .as_array()
        .ok_or("Missing original boot users")?;
    if users.iter().any(|row| row["id"] == MEMBER)
        || boot["self"]["id"] == MEMBER
        || init["self"]["id"] == MEMBER
    {
        return Err("Missing member must not exist in original seed".into());
    }
    seed()
        .set(json!({"boot":boot,"init":init}))
        .map_err(|_| "Mixed absent-member seed was already recorded".into())
}
fn admitted(protected: bool, group: bool, case: &str, member_flag: bool) -> bool {
    protected
        && group
        && member_flag
        && matches!(
            case,
            "mixed-absent-member" | "mixed-absent-member-cancel" | "mixed-absent-member-offline"
        )
}
pub(crate) fn enabled() -> bool {
    admitted(
        cfg!(any(test, feature = "protected-native-fixture")),
        super::enabled(),
        &case(),
        std::env::var(super::super::member_recovery::FLAG).as_deref() == Ok("1"),
    )
}
pub(super) fn tokens() -> Vec<Value> {
    [("bold", 4), ("italic", 5)]
        .into_iter()
        .map(|(style, occurrence)| {
            json!({"type":"user","user_id":MEMBER,"fixture_occurrence":occurrence,
                "tab_id":"opaque-member-tab","style":{(style):true},
                "opaque":{"keep":[occurrence,"original"]}})
        })
        .collect()
}
pub(crate) fn snapshot(shell: &ShellState) -> Value {
    if !enabled() {
        return Value::Null;
    }
    let observation = (|| {
        let team = shell.core.active_team.as_ref()?;
        let account = shell.core.active_account.as_ref()?;
        let workspace = shell.core.workspaces.get(team)?;
        let session = shell.core.session.as_ref()?.workspaces.get(team)?;
        let generation = "fixture-group-mixed-absent-member";
        let ids = BTreeSet::from([MEMBER.to_string()]);
        let subset = workspace
            .directory_members
            .subset(
                account,
                session,
                shell.core.account_epoch,
                generation,
                &ids,
                &workspace.users,
            )
            .ok()?;
        let row = subset
            .rows()
            .iter()
            .find(|row| row.source["id"] == MEMBER)?;
        Some(
            json!({"source":row.source,"native":workspace.users.get(MEMBER),
            "current":subset.current(&workspace.directory_members,account,session,
                shell.core.account_epoch,generation,&workspace.users)}),
        )
    })();
    json!({"seed":seed().get(),"member":super::super::member_recovery::snapshot(shell),"observation":observation})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_three_protected_group_cases_with_member_opt_in_admit_io() {
        for case in [
            "mixed-absent-member",
            "mixed-absent-member-cancel",
            "mixed-absent-member-offline",
        ] {
            assert!(admitted(true, true, case, true));
            assert!(!admitted(false, true, case, true));
            assert!(!admitted(true, false, case, true));
            assert!(!admitted(true, true, case, false));
        }
        for case in [
            "legacy",
            "mixed-member",
            "mixed-file-group",
            "automatic",
            "",
        ] {
            assert!(!admitted(true, true, case, true));
        }
    }

    #[test]
    fn absent_tokens_keep_two_original_occurrences_and_styles() {
        let raw = tokens();
        assert_eq!(raw.len(), 2);
        for (index, token) in raw.iter().enumerate() {
            assert_eq!(token["user_id"], "UMISSING01");
            assert_eq!(token["fixture_occurrence"], index + 4);
            assert_eq!(token["tab_id"], "opaque-member-tab");
            assert_eq!(token["opaque"]["keep"], json!([index + 4, "original"]));
            assert!(token.get("text").is_none());
        }
        assert_eq!(raw[0]["style"], json!({"bold":true}));
        assert_eq!(raw[1]["style"], json!({"italic":true}));
        assert_ne!(MEMBER, super::super::mixed::MEMBER);
        assert_eq!(LABEL, "  Cafe\u{0301} Harbor  ");
    }
}
