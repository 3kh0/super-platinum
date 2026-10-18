//! Captured XCcf/geao/PW6x display policy over exact About/profile observations.
use super::*;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Flags {
    pub connection: bool,
    pub about: bool,
    pub parity: bool,
}
impl Flags {
    pub fn decode(raw: &Value) -> Result<Self, Error> {
        let connection = connection_assignment(raw)?;
        let flag = |name: &str| -> Result<bool, Error> {
            match raw["assignments"].get(name) {
                None => Ok(false),
                Some(v) => v
                    .get("group")
                    .and_then(Value::as_str)
                    .map(|v| v == "on")
                    .ok_or_else(invalid),
            }
        };
        Ok(Self {
            connection,
            about: flag("agent_about_page")?,
            parity: flag("flexpane_content_parity")?,
        })
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct Profile {
    app: String,
    raw: Value,
}
impl Profile {
    pub fn decode(app: &str, raw: Value) -> Result<Self, Error> {
        if !id(app)
            || raw["ok"] != true
            || raw["app_profile"]["id"].as_str() != Some(app)
            || serde_json::to_vec(&raw).map_or(true, |v| v.len() > 1024 * 1024)
        {
            return Err(invalid());
        }
        for key in [
            "is_ai_app",
            "is_agent_app",
            "is_distributed",
            "is_directory_published",
            "is_slack_integration",
            "is_workflow_app",
            "is_installed",
        ] {
            if raw["app_profile"]
                .get(key)
                .is_some_and(|v| !v.is_null() && !v.is_boolean())
            {
                return Err(invalid());
            }
        }
        Ok(Self {
            app: app.into(),
            raw,
        })
    }
    pub fn raw(&self) -> &Value {
        &self.raw
    }
    pub fn is_ai(&self, info: &Info) -> bool {
        let p = &self.raw["app_profile"];
        info.raw["is_ai_app"] == true
            || info.raw["is_agent_app"] == true
            || p["is_ai_app"] == true
            || p["is_agent_app"] == true
            || [
                "can_show_in_sunroof",
                "can_respond_in_public_channel",
                "can_respond_in_private_channel",
                "can_respond_in_mpdm",
            ]
            .iter()
            .any(|k| p["agent_capabilities"][k] == true)
    }
    pub fn show_permissions(&self, info: &Info, flags: Flags, top: &str) -> bool {
        if self.app != info.app || !flags.about {
            return false;
        }
        let p = &self.raw["app_profile"];
        let native = native_flow(&self.app);
        let external = p["enterprise_id"]
            .as_str()
            .or_else(|| p["team_id"].as_str())
            != Some(top);
        let directory = !native
            && p["native_agent"]["type"] != "channel_expert"
            && (p["is_slack_integration"] == true
                || p["config"]["is_custom_integration"] == true
                || if external {
                    p["is_directory_published"] == true
                } else {
                    p["is_distributed"] == true
                });
        let agentforce = info
            .raw
            .get("agentforce")
            .is_some_and(|v| !v.is_null() && v != &json!(false));
        let has_scopes = info.raw["scope_info"]
            .get("content")
            .is_some_and(|v| !v.is_null())
            || info.raw["scope_info"]
                .get("actions")
                .is_some_and(|v| !v.is_null());
        let renderable = [false, true].into_iter().any(|optional| {
            ["content", "actions"]
                .into_iter()
                .any(|kind| info.permissions(kind, optional).is_some())
        });
        let ordinary_internal = info.raw["is_distributed"] != true
            && info.raw["is_workflow_app"] != true
            && info.raw.get("native_agent").is_none_or(Value::is_null)
            && !native;
        renderable
            && if flags.parity {
                !agentforce && (directory || self.is_ai(info) && has_scopes || ordinary_internal)
            } else {
                directory || self.is_ai(info) && !agentforce && has_scopes
            }
    }
}
// UJYZ/4ahw and EYEw enumerate production, development and Gov native apps.
// All are excluded from directory/configuration treatment, regardless of realm.
pub(super) fn native_flow(app: &str) -> bool {
    matches!(
        app,
        "A01126R8Y2X" | "A0112L5RZ5E" | "A014JPFMT8T" | "A04T99UKKQE" | "A07846N4L" | "A1W53KSJQ"
    )
}
