//! hHTo member matching over acquired directory rows. This does not own final
//! frecency ordering or authorize a directory result after its owner changes.
use super::*;
use std::collections::{BTreeMap, BTreeSet};
use super_platinum_core::block_actions::home::workflow::{
    DirectoryUser,
    directory_local::{Options, derive_member_names, filter_members},
};

#[derive(Clone)]
pub(super) struct MatchInfo {
    pub(super) item: Value,
    pub(super) score: Option<u32>,
    pub(super) score_details: Option<BTreeMap<&'static str, u32>>,
    pub(super) is_exact_match: Option<bool>,
    pub(super) exact_match_details: Option<BTreeMap<&'static str, bool>>,
    pub(super) is_self: bool,
    pub(super) is_flannel_fuzzy_passthrough: bool,
    pub(super) remote: bool,
}
pub(super) struct Matched {
    pub(super) user: DirectoryUser,
    pub(super) cache: Option<Value>,
    pub(super) matching: MatchInfo,
}

pub(super) async fn matching(
    state: Signal<ShellState>,
    query: &entity_search::Query,
    users: Vec<DirectoryUser>,
) -> Result<Vec<Matched>, Error> {
    if !query.members() {
        return Ok(Vec::new());
    }
    let ctx = &query.owner().owner.ctx;
    let remote: BTreeSet<_> = users.iter().map(|u| u.user().id.clone()).collect();
    let cache: BTreeMap<String, Value> = {
        let s = state.peek();
        if !query.current(&s) {
            return Err(unavailable());
        }
        let workspace = s
            .core
            .workspaces
            .get(&ctx.session.team_id)
            .ok_or_else(unavailable)?;
        workspace
            .users
            .iter()
            .map(|(id, user)| {
                serde_json::to_value(user)
                    .map(|raw| (id.clone(), raw))
                    .map_err(|_| unavailable())
            })
            .collect::<Result<_, _>>()?
    };
    // Local member acquisition is not limited to the server's24-row page. Keep
    // all accepted local objects until the source ranker's separate shortlist.
    let mut combined = cache
        .iter()
        .map(|(id, raw)| DirectoryUser::decode(raw).map(|u| (id.clone(), u)))
        .collect::<Result<BTreeMap<_, _>, _>>()?;
    for user in users {
        combined.insert(user.user().id.clone(), user);
    }
    let users: Vec<_> = combined
        .into_values()
        .filter(|user| interactable(user.raw(), cache.get(&user.user().id)))
        .collect();
    if users.is_empty() {
        return Ok(Vec::new());
    }
    // The remote fuzzy exception and self partial-update branch are irrelevant
    // without remote rows. Empty local suggestions do not cause network work.
    let (passthrough, preserve_email) = if remote.is_empty() {
        (false, false)
    } else {
        let assignments = query
            .read(state, api::list_experiments(&ctx.client, &ctx.session))
            .await?;
        (
            assignment(&assignments, "autocomplete_flannel_fuzzy_match_passthrough")?
                == Some("treatment"),
            assignment(&assignments, "preserve_omitted_user_change_member_fields")? == Some("on"),
        )
    };
    let effective = users
        .iter()
        .map(|user| {
            let old_email = cache
                .get(&user.user().id)
                .and_then(|u| u.pointer("/profile/email"))
                .and_then(Value::as_str);
            effective(
                user.raw(),
                &ctx.session.user_id,
                preserve_email && remote.contains(&user.user().id),
                old_email,
            )
        })
        .collect::<Vec<_>>();
    let names = effective
        .iter()
        .flat_map(|raw| {
            ["real_name", "display_name", "email"]
                .map(|key| raw["profile"][key].as_str().unwrap_or("").to_owned())
        })
        .collect();
    let casing = entity_normalization::capture(state, query, names).await?;
    let mut members = Vec::with_capacity(effective.len());
    for raw in &effective {
        // No callback fallback: every string read by the core producer must
        // have been converted by the actual renderer under this query owner.
        for key in ["real_name", "display_name", "email"] {
            if casing
                .name(raw["profile"][key].as_str().unwrap_or(""))
                .is_none()
            {
                return Err(unavailable());
            }
        }
        members.push(
            derive_member_names(raw, |name| {
                casing
                    .name(name)
                    .expect("all three captured names were checked")
                    .to_owned()
            })
            .ok_or_else(unavailable)?,
        );
    }
    let mut options = Options::workflow(
        &query.text,
        &casing.query.normalized,
        &casing.query.matcher_normalized,
        // KhGM always includes these English aliases independently of its
        // utility translations. They also match this client's English UI.
        matches!(casing.query.normalized.as_str(), "me" | "you"),
    );
    options.allow_empty_query = true;
    options.email_forward_only = true;
    options.passthrough_remote_fuzzy_filtering = passthrough;
    let matched: BTreeMap<_, _> = filter_members(&members, &options, &remote, |_| true)
        .into_iter()
        .map(|v| {
            (
                v.id.to_owned(),
                MatchInfo {
                    item: v.item.clone(),
                    score: Some(v.score),
                    score_details: v.score_details,
                    is_exact_match: v.is_exact_match,
                    exact_match_details: v.exact_match_details,
                    is_self: v.is_self,
                    is_flannel_fuzzy_passthrough: v.is_flannel_fuzzy_passthrough,
                    remote: remote.contains(v.id),
                },
            )
        })
        .collect();
    if !query.current(&state.peek()) {
        return Err(unavailable());
    }
    // Eligibility remains the separately acquired actual-user/Agentforce
    // policy. A fuzzy match never supplies permission or changes the U/W ID.
    Ok(users
        .into_iter()
        .filter_map(|user| {
            let matching = matched.get(&user.user().id)?.clone();
            let stamp = cache.get(&user.user().id).cloned();
            Some(Matched {
                user,
                cache: stamp,
                matching,
            })
        })
        .collect())
}

fn interactable(raw: &Value, previous: Option<&Value>) -> bool {
    // XbX7's actual is_stranger observation updates the local interaction fact;
    // th9D.zP rejects only explicit false. Omitted input preserves prior fact.
    raw.get("is_stranger")
        .or_else(|| previous.and_then(|v| v.get("is_stranger")))
        != Some(&Value::Bool(true))
}

pub(super) fn effective(
    raw: &Value,
    self_id: &str,
    preserve: bool,
    old_email: Option<&str>,
) -> Value {
    let mut value = raw.clone();
    if value["id"] == self_id {
        value["is_self"] = Value::Bool(true);
        if preserve && value["profile"]["email"].is_null() {
            if let (Some(profile), Some(email)) = (value["profile"].as_object_mut(), old_email) {
                profile.insert("email".into(), Value::String(email.into()));
            }
        }
    }
    value
}
pub(super) fn assignment<'a>(raw: &'a Value, key: &str) -> Result<Option<&'a str>, Error> {
    if raw["ok"] != true {
        return Err(unavailable());
    }
    let all = raw
        .get("assignments")
        .and_then(Value::as_object)
        .ok_or_else(unavailable)?;
    match all.get(key) {
        None => Ok(None), // Actual 4OuQ._Z returns null for absent assignment.
        Some(v) => v
            .get("group")
            .and_then(Value::as_str)
            .map(Some)
            .ok_or_else(unavailable),
    }
}
fn unavailable() -> Error {
    Error::Api("home_workflow_member_matching_unavailable".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn omitted_self_email_uses_only_actual_enabled_prior_profile() {
        let raw = json!({"id":"U1","profile":{"real_name":"Ada"}});
        assert_eq!(
            effective(&raw, "U1", true, Some("prior@example.invalid"))["profile"]["email"],
            "prior@example.invalid"
        );
        assert!(
            effective(&raw, "U1", false, Some("prior@example.invalid"))["profile"]["email"]
                .is_null()
        );
        assert!(
            effective(&raw, "U2", true, Some("prior@example.invalid"))["profile"]["email"]
                .is_null()
        );
        assert!(raw["profile"]["email"].is_null());
    }
    #[test]
    fn explicit_empty_email_and_raw_name_fields_are_not_replaced() {
        let raw =
            json!({"id":"U1","real_name":"Top","profile":{"email":"","display_name":"  Name  "}});
        let observed = effective(&raw, "U1", true, Some("prior@example.invalid"));
        assert_eq!(observed["profile"]["email"], "");
        assert_eq!(observed["profile"]["display_name"], "  Name  ");
        assert!(observed["profile"]["real_name"].is_null());
    }
    #[test]
    fn explicit_null_self_email_is_nullish_but_missing_prior_email_is_not_invented() {
        let raw = json!({"id":"U1","profile":{"email":null}});
        assert_eq!(
            effective(&raw, "U1", true, Some("previous@example.invalid"))["profile"]["email"],
            "previous@example.invalid"
        );
        assert!(effective(&raw, "U1", true, None)["profile"]["email"].is_null());
    }
    #[test]
    fn local_interaction_rejects_only_observed_denial_and_preserves_omission() {
        assert!(interactable(&json!({"id":"U1"}), None));
        assert!(!interactable(&json!({"id":"U1","is_stranger":true}), None));
        assert!(!interactable(
            &json!({"id":"U1"}),
            Some(&json!({"is_stranger":true}))
        ));
        assert!(interactable(
            &json!({"id":"U1","is_stranger":false}),
            Some(&json!({"is_stranger":true}))
        ));
    }
    #[test]
    fn absent_assignment_is_distinct_from_unavailable_or_malformed_read() {
        assert_eq!(
            assignment(&json!({"ok":true,"assignments":{}}), "flag").unwrap(),
            None
        );
        assert_eq!(
            assignment(
                &json!({"ok":true,"assignments":{"flag":{"group":"treatment"}}}),
                "flag"
            )
            .unwrap(),
            Some("treatment")
        );
        for raw in [
            json!({"ok":false}),
            json!({"ok":true}),
            json!({"ok":true,"assignments":{"flag":{"group":true}}}),
        ] {
            assert!(assignment(&raw, "flag").is_err());
        }
    }
}
