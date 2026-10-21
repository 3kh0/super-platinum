//! Caller-specific selector facts; no assignment rewriting.
use super::*;
pub(super) enum Policy {
    Profile(policy::Policy),
    List {
        policy: policy::ListPolicy,
        membership: BTreeMap<String, policy::ListMembership>,
    },
}
impl Policy {
    pub(super) fn capture(
        search: &Search<impl Host>,
        shell: &ShellState,
        assignments: &Value,
    ) -> Result<Self, Error> {
        let Some(context) = search.lease.member_context(shell)? else {
            return Ok(Self::Profile(policy::Policy::decode(assignments)?));
        };
        Self::list(assignments, &context)
    }
    fn list(assignments: &Value, context: &Value) -> Result<Self, Error> {
        let channel = context.get("channelSelectors").ok_or_else(unavailable)?;
        let flag = |key: &str| channel[key].as_bool().ok_or_else(unavailable);
        let channel = if channel.is_null() {
            None
        } else {
            Some(policy::ListChannel {
                is_im_or_mpim: flag("isImOrMpim")?,
                unknown_or_nonexistent: flag("unknownOrNonexistent")?,
                externally_shared: flag("externallyShared")?,
            })
        };
        let assistant = match context.get("assistantUserId") {
            Some(Value::Null) => None,
            Some(Value::String(s)) => Some(s.clone()),
            _ => return Err(unavailable()),
        };
        if context["membershipObserved"] != true {
            return Err(unavailable());
        }
        let membership = context["membership"]
            .as_object()
            .ok_or_else(unavailable)?
            .iter()
            .map(|(id, v)| {
                let known = v["isKnown"].as_bool().ok_or_else(unavailable)?;
                let member = v["isMember"].as_bool().ok_or_else(unavailable)?;
                Ok((
                    id.clone(),
                    if known {
                        policy::ListMembership::Known(member)
                    } else {
                        policy::ListMembership::Unknown
                    },
                ))
            })
            .collect::<Result<_, Error>>()?;
        Ok(Self::List {
            policy: policy::ListPolicy::decode(assignments, channel, assistant)?,
            membership,
        })
    }
    pub(super) fn requirements(&self, user: &User) -> Result<policy::Requirements, Error> {
        match self {
            Self::Profile(p) => p.requirements(user),
            Self::List { policy, .. } => policy.requirements(user),
        }
    }
    pub(super) fn allows(
        &self,
        row: &Value,
        user: &User,
        facts: policy::Facts<'_>,
    ) -> Result<bool, Error> {
        match self {
            Self::Profile(p) => p.allows(user, facts),
            Self::List { policy, membership } => policy.allows(
                user,
                facts,
                policy::ListMemberFacts {
                    external_bot: Some(
                        truthy(&row["isExternal"])
                            && (truthy(&row["is_app_user"])
                                || truthy(&row["is_bot"])
                                || user.id == "USLACK"),
                    ),
                    membership: Some(
                        membership
                            .get(&user.id)
                            .copied()
                            .unwrap_or(policy::ListMembership::Unknown),
                    ),
                },
            ),
        }
    }
}
fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(v) => *v,
        Value::Number(v) => v.as_f64().is_some_and(|v| v != 0.0),
        Value::String(v) => !v.is_empty(),
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn assignments() -> Value {
        json!({"ok":true,"assignments":{"at_menu_hide_external_bots":{"group":"on"}}})
    }
    fn context() -> Value {
        json!({"channelSelectors":{"isImOrMpim":false,"unknownOrNonexistent":false,"externallyShared":true},"assistantUserId":null,"membershipObserved":true,"membership":{}})
    }
    #[test]
    fn source_membership_unknown_is_not_known_false_and_partial_context_refuses() {
        let row = json!({"id":"U00000002","isExternal":true,"is_bot":true,"profile":{}});
        let user: User = serde_json::from_value(row.clone()).unwrap();
        let facts = || policy::Facts {
            restricted_app: None,
            agent_allowed: None,
            salesforce_app: None,
        };
        let mut ctx = context();
        assert!(
            Policy::list(&assignments(), &ctx)
                .unwrap()
                .allows(&row, &user, facts())
                .unwrap()
        );
        ctx["membership"][&user.id] = json!({"isKnown":true,"isMember":false});
        assert!(
            !Policy::list(&assignments(), &ctx)
                .unwrap()
                .allows(&row, &user, facts())
                .unwrap()
        );
        ctx["membership"][&user.id] = json!({"isKnown":false,"isMember":false});
        assert!(
            Policy::list(&assignments(), &ctx)
                .unwrap()
                .allows(&row, &user, facts())
                .unwrap()
        );
        ctx["membershipObserved"] = json!(false);
        assert!(Policy::list(&assignments(), &ctx).is_err());
        ctx = context();
        ctx.as_object_mut().unwrap().remove("assistantUserId");
        assert!(Policy::list(&assignments(), &ctx).is_err());
    }
    #[test]
    fn profile_policy_retains_original_noncontextual_route() {
        let row = json!({"id":"U00000002","isExternal":true,"is_bot":true,"profile":{}});
        let user: User = serde_json::from_value(row.clone()).unwrap();
        let p = Policy::Profile(policy::Policy::decode(&assignments()).unwrap());
        assert!(
            p.allows(
                &row,
                &user,
                policy::Facts {
                    restricted_app: None,
                    agent_allowed: None,
                    salesforce_app: None
                }
            )
            .unwrap()
        );
    }
}
