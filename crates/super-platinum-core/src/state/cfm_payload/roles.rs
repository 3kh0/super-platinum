//! GTHu.Y and kzmv.N expand role thunks; neither replaces self/member data.
//! Source: kH+p, GTHu, kzmv in retained shared-client.8da2e1b1875c77ec;
//! A+lF/Jh6j/uVRc in shared.3b454575c4abf06c. Account-type effects stay ordered.
use super::*;

fn identity(value: &Value, prefix: u8) -> bool {
    value.as_str().is_some_and(|id| {
        id.len() >= 9
            && id.as_bytes()[0] == prefix
            && id
                .bytes()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
    })
}

pub(super) fn member<A: Adopter>(r: &mut Run<'_, A>, user: &Value) -> Result<(), Error> {
    if !truth(&user["id"]) || truth(&user["enterprise_id"]) || identity(&user["team_id"], b'E') {
        return Ok(());
    }
    let team = user
        .get("team_id")
        .filter(|v| truth(v))
        .cloned()
        .or_else(|| r.adopter.facts().team.map(Value::String))
        .unwrap_or(Value::Null);
    let kind = if truth(&user["is_primary_owner"]) {
        Kind::MemberPrimaryOwner
    } else if truth(&user["is_owner"]) {
        Kind::MemberOwner
    } else if truth(&user["is_admin"]) {
        Kind::MemberAdmin
    } else if ["is_primary_owner", "is_owner", "is_admin"]
        .iter()
        .any(|key| user.get(*key) == Some(&Value::Bool(false)))
    {
        Kind::MemberRoleClear
    } else {
        return Ok(());
    };
    r.emit(kind, json!({"memberId":user["id"],"teamId":team}))
}

pub(super) fn self_account_types<A: Adopter>(
    r: &mut Run<'_, A>,
    user: &Value,
) -> Result<(), Error> {
    if identity(&user["team_id"], b'T') && !truth(&user["enterprise_id"]) {
        for (kind, key) in [
            (Kind::PrimaryOwner, "is_primary_owner"),
            (Kind::Owner, "is_owner"),
            (Kind::Admin, "is_admin"),
        ] {
            r.emit(
                kind,
                json!({"teamId":user["team_id"],"value":truth(&user[key])}),
            )?;
        }
    }
    if let Some(enterprise_user) = user.get("enterprise_user").filter(|v| truth(v))
        && let Some(enterprise) = r.adopter.facts().enterprise.filter(|s| !s.is_empty())
    {
        for (kind, key) in [
            (Kind::EnterprisePrimaryOwner, "is_primary_owner"),
            (Kind::EnterpriseOwner, "is_owner"),
            (Kind::EnterpriseAdmin, "is_admin"),
        ] {
            r.emit(
                kind,
                json!({"enterpriseId":enterprise,"value":truth(&enterprise_user[key])}),
            )?;
        }
    }
    Ok(())
}
