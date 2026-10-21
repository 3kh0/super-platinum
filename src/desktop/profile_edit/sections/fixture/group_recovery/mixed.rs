//! Original canonical boot member alongside automatically acquired groups.
use super::*;
use std::collections::BTreeSet;
pub(crate) const MEMBER: &str = "UMIXED001";
pub(crate) const MEMBER_LABEL: &str = "@Known mixed member";
pub(super) fn raw(team: &str) -> Value {
    json!({"id":MEMBER,"team_id":team,"name":"username-must-not-win",
        "real_name":"Real fallback","profile":{"display_name":"Known mixed member",
        "real_name":"Profile fallback"},"updated":101})
}
pub(super) fn snapshot(shell: &ShellState) -> Value {
    if !matches!(case().as_str(), "mixed-member" | "mixed-file-group") {
        return Value::Null;
    }
    let observed = (|| {
        let team = shell.core.active_team.as_ref()?;
        let account = shell.core.active_account.as_ref()?;
        let workspace = shell.core.workspaces.get(team)?;
        let session = shell.core.session.as_ref()?.workspaces.get(team)?;
        let generation = "fixture-group-mixed-member";
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
        let row = subset.rows().iter().find(|r| r.source["id"] == MEMBER)?;
        Some(
            json!({"source":row.source,"native":workspace.users.get(MEMBER),
            "current":subset.current(&workspace.directory_members,account,session,
                shell.core.account_epoch,generation,&workspace.users)}),
        )
    })();
    json!({"observation":observed,
        "absentDriver":crate::directory_member_fetches::driver::fixture_status(shell),
        "knownDriver":crate::directory_member_fetches::known::fixture_status(shell),
        "automaticDriver":crate::directory_member_fetches::automatic::fixture_status(shell,
            shell.core.active_team.as_deref().unwrap_or(""))})
}
