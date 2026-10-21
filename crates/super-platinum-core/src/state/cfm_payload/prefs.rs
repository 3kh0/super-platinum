use super::*;
use std::collections::BTreeMap;
mod number;
mod plan_info;
const JSON_PREFS: &[&str] = &[
    "emoji_use",
    "frecency",
    "recents",
    "channel_canvas_layouts",
    "channel_sections",
    "all_notifications_prefs",
    "user_colors",
    "ia_theme",
    "a11y_message_verbosity_settings",
    "slack_scribe_dictionary",
    "focus_mode_status",
    "scheduled_dnd_status",
    "non_member_mention_invite_opt_out",
    "autosort_channel_placements",
];
pub(super) fn deserialize(raw: &Value) -> Result<Value, Error> {
    let raw = raw.as_object().ok_or(Error::Malformed("prefs object"))?;
    let mut out = serde_json::Map::new();
    for (k, v) in raw {
        if !JSON_PREFS.contains(&k.as_str()) {
            out.insert(k.clone(), v.clone());
        }
    }
    for key in JSON_PREFS {
        if let Some(v) = raw.get(*key).filter(|v| truth(v)) {
            let parsed = match v {
                Value::String(s) => serde_json::from_str(s).unwrap_or(json!({})),
                _ => v.clone(),
            };
            out.insert((*key).into(), parsed);
        }
    }
    Ok(Value::Object(out))
}
pub(super) fn number(v: &Value) -> Result<f64, Error> {
    Ok(match v {
        Value::Null => 0.0,
        Value::Bool(b) => {
            if *b {
                1.0
            } else {
                0.0
            }
        }
        Value::Number(n) => n.as_f64().ok_or(Error::Malformed("mute number"))?,
        Value::String(s) => number::parse(s),
        _ => return Err(Error::Unavailable("non-scalar temp mute timestamp")),
    })
}
/// Neutral stores with exact retained Nt5j/BWei/JdT4/XcV0/GSp semantics.
/// This is payload state, not a second shared directory cache. Parent must
/// keep it under the original workspace and project relevant fields into the
/// existing native consumers. No owner/currentness or freshness is inferred.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Stores {
    /// Raw n4D5 state after an accepted action, never currentness authority.
    /// Older caches and stores with no observed action remain unobserved.
    #[serde(default)]
    pub plan_info: Option<Value>,
    pub screen_reader_enabled: Option<bool>,
    pub member_roles: BTreeMap<String, BTreeMap<String, Value>>,
    pub workspace_roles: BTreeMap<String, Value>,
    pub enterprise_roles: BTreeMap<String, Value>,
    pub user_prefs: BTreeMap<String, Value>,
    pub team_prefs: BTreeMap<String, BTreeMap<String, Value>>,
    pub hidden_users: BTreeMap<String, Value>,
    pub muted_channels: BTreeMap<String, Value>,
    pub user_group_membership: BTreeMap<String, Value>,
    pub internal_workspace_ids: Option<Value>,
    pub default_workspace: Option<Value>,
}
impl Stores {
    /// Applies only the listed neutral reducers. Unsupported is mandatory for
    /// every other action; callers may not discard it and report publication.
    pub fn apply(&mut self, e: &Effect) -> Result<(), Error> {
        match e.kind {
            Kind::PlanInfo => plan_info::apply(&mut self.plan_info, &e.payload)?,
            Kind::ScreenReaderEnabled => {
                if let Some(value) = e.payload.as_bool() {
                    self.screen_reader_enabled = Some(value);
                }
            }
            Kind::MemberPrimaryOwner
            | Kind::MemberOwner
            | Kind::MemberAdmin
            | Kind::MemberRoleClear => {
                let member = e.payload["memberId"].as_str().unwrap_or("");
                let team = e.payload["teamId"].as_str().unwrap_or("");
                if member.is_empty() || team.is_empty() {
                    return Ok(());
                }
                if e.kind == Kind::MemberRoleClear {
                    if let Some(roles) = self.member_roles.get_mut(team) {
                        roles.remove(member);
                    }
                } else {
                    let key = match e.kind {
                        Kind::MemberPrimaryOwner => "isPrimaryOwner",
                        Kind::MemberOwner => "isOwner",
                        _ => "isAdmin",
                    };
                    // A+lF replaces this member's role object, not independent bits.
                    self.member_roles
                        .entry(team.into())
                        .or_default()
                        .insert(member.into(), json!({(key):true}));
                }
            }
            Kind::PrimaryOwner
            | Kind::Owner
            | Kind::Admin
            | Kind::EnterprisePrimaryOwner
            | Kind::EnterpriseOwner
            | Kind::EnterpriseAdmin => {
                let (enterprise, key) = match e.kind {
                    Kind::PrimaryOwner => (false, "isWorkspacePrimaryOwner"),
                    Kind::Owner => (false, "isWorkspaceOwner"),
                    Kind::Admin => (false, "isWorkspaceAdmin"),
                    Kind::EnterprisePrimaryOwner => (true, "isEnterprisePrimaryOwner"),
                    Kind::EnterpriseOwner => (true, "isEnterpriseOwner"),
                    _ => (true, "isEnterpriseAdmin"),
                };
                let id = e.payload[if enterprise { "enterpriseId" } else { "teamId" }]
                    .as_str()
                    .ok_or(Error::Malformed("role identity"))?;
                let value = e.payload["value"]
                    .as_bool()
                    .ok_or(Error::Malformed("role value"))?;
                let roles = if enterprise {
                    &mut self.enterprise_roles
                } else {
                    &mut self.workspace_roles
                };
                let entry = roles.entry(id.into()).or_insert_with(|| json!({}));
                if !enterprise {
                    entry["id"] = id.into();
                }
                entry[key] = value.into();
            }
            Kind::UserPrefs => self.user_prefs.extend(object(&e.payload)?),
            Kind::TeamPrefs => {
                let id = e
                    .payload
                    .get("teamId")
                    .and_then(Value::as_str)
                    .ok_or(Error::Malformed("team prefs id"))?;
                let raw = e
                    .payload
                    .get("teamPrefs")
                    .ok_or(Error::Malformed("team prefs"))?;
                // Nt5j returns before copying any empty lodash collection, including null.
                if matches!(raw, Value::Null | Value::Bool(_) | Value::Number(_))
                    || raw.as_array().is_some_and(Vec::is_empty)
                    || raw.as_str() == Some("")
                {
                    return Ok(());
                }
                let prefs = object(raw)?;
                if !id.is_empty() && !prefs.is_empty() {
                    self.team_prefs.entry(id.into()).or_default().extend(prefs);
                }
            }
            Kind::HiddenUsers => self.hidden_users = object(&e.payload)?,
            Kind::MutedChannels => self.muted_channels = object(&e.payload)?,
            Kind::Subteams => {
                let ids = e
                    .payload
                    .get("userGroups")
                    .and_then(Value::as_array)
                    .ok_or(Error::Malformed("subteams array"))?;
                let next = ids
                    .iter()
                    .map(|v| {
                        v.as_str()
                            .filter(|id| *id != "__proto__")
                            .map(|id| (id.into(), json!({"isMember":true})))
                            .ok_or(Error::Malformed("subteam id"))
                    })
                    .collect::<Result<_, _>>()?;
                self.user_group_membership = next;
            }
            Kind::InternalWorkspaces => {
                self.internal_workspace_ids = Some(
                    e.payload
                        .get("internalIds")
                        .ok_or(Error::Malformed("internal ids"))?
                        .clone(),
                )
            }
            Kind::DefaultWorkspace => {
                self.default_workspace = Some(
                    e.payload
                        .get("id")
                        .ok_or(Error::Malformed("default workspace"))?
                        .clone(),
                )
            }
            _ => return Err(Error::Unsupported(e.kind)),
        }
        Ok(())
    }
}
fn object(v: &Value) -> Result<BTreeMap<String, Value>, Error> {
    Ok(v.as_object()
        .ok_or(Error::Malformed("reducer object"))?
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect())
}
