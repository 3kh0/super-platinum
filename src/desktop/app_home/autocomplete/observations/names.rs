//! Exact complete factories/decoders shared by account startup and Share.
use super::*;
use dioxus::prelude::{ReadableExt, Signal};
use serde_json::json;
use std::collections::BTreeSet;
use super_platinum_core::{block_actions::shortcuts::grid, slack::PreparedRequest};
async fn read<F, Fut>(
    request: &F,
    prepared: PreparedRequest,
    reads: &mut Vec<Value>,
) -> Result<Value, Error>
where
    F: Fn(PreparedRequest) -> Fut,
    Fut: std::future::Future<Output = Result<Value, Error>>,
{
    let method = prepared.method.to_owned();
    let raw = request(prepared)
        .await
        .map_err(|e| super::acquire::at(&format!("observations.names.read.{method}"), e))?;
    reads.push(json!({"method":method,"raw":raw}));
    Ok(raw)
}
pub(super) async fn capture<F, Fut>(
    state: Signal<ShellState>,
    ctx: &Context,
    authority: &Authority,
    request: &F,
) -> Result<Names, Error>
where
    F: Fn(PreparedRequest) -> Fut,
    Fut: std::future::Future<Output = Result<Value, Error>>,
{
    let (membership, enterprise) = authority
        .name_policy_membership(&state.peek().core, policy::OrdinaryClientV2)
        .map_err(|_| super::acquire::at("observations.names.membership", unavailable()))?;
    let mut reads = Vec::new();
    let raw = read(
        request,
        grid::discovery(&ctx.client, &ctx.session),
        &mut reads,
    )
    .await?;
    grid::Membership::decode(raw.clone(), &ctx.session)
        .map_err(|e| super::acquire::at("observations.names.gantry", e))?;
    if raw.get("error").is_some_and(|v| !v.is_null()) {
        return Err(unavailable());
    }
    let workspaces = raw["workspaces"].as_array().ok_or_else(unavailable)?;
    let actual = workspaces
        .iter()
        .filter(|v| v["allow_external_users"] != true)
        .map(|v| v["id"].as_str().ok_or_else(unavailable))
        .collect::<Result<BTreeSet<_>, _>>()?;
    if actual != membership.workspace_ids().collect() {
        return Err(super::acquire::at(
            "observations.names.workspace-set",
            unavailable(),
        ));
    }
    let default_workspace = match raw.get("default_workspace") {
        None | Some(Value::Null) => None,
        Some(Value::String(v)) if v.starts_with('T') => Some(v.clone()),
        _ => return Err(unavailable()),
    };
    let self_user = raw["self"].clone();
    let prefs = read(
        request,
        policy::request::user(&ctx.client, &ctx.session),
        &mut reads,
    )
    .await?;
    let user = policy::UserPrefs::decode(&prefs)
        .map_err(|_| super::acquire::at("observations.names.user-prefs", unavailable()))?;
    let history = match prefs["prefs"].get("frecency") {
        Some(v) => History::from_pref(v)
            .map_err(|_| super::acquire::at("observations.names.frecency", unavailable()))?,
        None => History::default(), // Complete prefs observation, not a failed read.
    };
    let enterprise_prefs = if let Some(id) = enterprise.as_ref() {
        let raw = read(
            request,
            policy::request::enterprise(&ctx.client, &ctx.session),
            &mut reads,
        )
        .await?;
        Some(policy::EnterprisePrefs::decode(id, &raw).map_err(|_| unavailable())?)
    } else {
        None
    };
    let mut policy =
        policy::Snapshot::new(policy::OrdinaryClientV2, membership, user, enterprise_prefs)
            .map_err(|_| unavailable())?;
    for row in workspaces {
        let id = row["id"].as_str().ok_or_else(unavailable)?;
        if !policy.workspace_ids().any(|v| v == id) {
            continue;
        }
        match row.get("prefs") {
            Some(Value::Object(prefs)) => policy
                .observe_team(
                    policy::TeamPrefs::from_complete(id, prefs).map_err(|_| unavailable())?,
                )
                .map_err(|_| unavailable())?,
            None | Some(Value::Null) => {}
            _ => return Err(unavailable()),
        }
    }
    for id in policy.missing_team_preferences() {
        let raw = read(
            request,
            policy::request::team_for(&ctx.client, &ctx.session, &id).map_err(|_| unavailable())?,
            &mut reads,
        )
        .await?;
        policy
            .observe_team(policy::TeamPrefs::decode(&id, &raw).map_err(|_| unavailable())?)
            .map_err(|_| unavailable())?;
        if policy.should_display_real_names() == Ok(true) {
            break;
        }
    }
    policy
        .should_display_real_names()
        .map_err(|_| super::acquire::at("observations.names.complete", unavailable()))?;
    Ok(Names {
        policy,
        history,
        enterprise,
        self_user,
        default_workspace,
        reads,
    })
}

pub(crate) fn assignment<'a>(raw: &'a Value, key: &str) -> Result<Option<&'a str>, Error> {
    if raw["ok"] != true || raw.get("error").is_some_and(|v| !v.is_null()) {
        return Err(unavailable());
    }
    match raw["assignments"]
        .as_object()
        .ok_or_else(unavailable)?
        .get(key)
    {
        None => Ok(None),
        Some(v) => v["group"].as_str().map(Some).ok_or_else(unavailable),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn complete_absent_assignment_is_distinct_from_unavailable_or_malformed() {
        assert_eq!(
            assignment(&json!({"ok":true,"assignments":{}}), "key").unwrap(),
            None
        );
        for raw in [
            json!({"ok":false,"assignments":{}}),
            json!({"ok":true}),
            json!({"ok":true,"assignments":null}),
            json!({"ok":true,"assignments":{"key":null}}),
            json!({"ok":true,"assignments":{"key":{"group":true}}}),
            json!({"ok":true,"assignments":{},"error":"restricted"}),
        ] {
            assert!(assignment(&raw, "key").is_err());
        }
    }

    #[test]
    fn source_passthrough_treatment_does_not_conflate_on_or_control() {
        for group in ["on", "control", "treatment", "off"] {
            let raw = json!({"ok":true,"assignments":{"key":{"group":group}}});
            assert_eq!(
                assignment(&raw, "key").unwrap() == Some("treatment"),
                group == "treatment"
            );
        }
    }
}
