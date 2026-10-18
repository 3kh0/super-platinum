use super::super::*;
use serde_json::Map;
#[derive(Clone, Debug, PartialEq)]
pub struct ConnectorApps {
    raw: Value,
}
#[derive(Clone, Debug, PartialEq)]
pub struct EditPermissions {
    pub is_publish_restricted: bool,
    pub apps: Value,
}
impl ConnectorApps {
    pub fn request() -> RequestPlan {
        RequestPlan::new("apps.certified.list", "fetch-connector-apps", json!({}))
    }
    pub fn decode(raw: Value) -> Result<Self, Error> {
        success(&raw)?;
        for app in array(&raw, "apps")? {
            identifier(app, "id")?;
        }
        for key in ["is_aaa_enabled", "can_user_manage_app"] {
            if raw
                .get(key)
                .is_some_and(|v| !v.is_null() && !v.is_boolean())
            {
                return Err(invalid());
            }
        }
        Ok(Self { raw })
    }
    pub fn raw(&self) -> &Value {
        &self.raw
    }
    /// `visibility` is the actual CtFl.IN resolved preference. None means its
    /// producer is unavailable; Some(Null) means observed absence. Capabilities
    /// own the org/team selection, not this decoder.
    pub fn permissions(
        &self,
        workflow: &Value,
        visibility: Option<&Value>,
    ) -> Result<EditPermissions, Error> {
        let mut apps = Map::new();
        for app in array(&self.raw, "apps")? {
            apps.insert(identifier(app, "id")?, app.clone());
        }
        let aaa = self.raw["is_aaa_enabled"] == json!(true);
        let mut result = EditPermissions {
            is_publish_restricted: false,
            apps: json!({}),
        };
        fn walk(
            steps: &[Value],
            apps: &Map<String, Value>,
            aaa: bool,
            visibility: Option<&Value>,
            result: &mut EditPermissions,
        ) -> Result<(), Error> {
            for step in steps {
                if step.get("function").is_some() && step.get("config").is_none() {
                    if let Some(app) = step["function"]["app_id"]
                        .as_str()
                        .and_then(|id| apps.get(id))
                    {
                        let id = identifier(app, "id")?;
                        let mut entry = Map::new();
                        if let Some(resolution) = app.get("app_resolution") {
                            entry.insert("appResolution".into(), resolution.clone());
                        }
                        result.apps[&id] = Value::Object(entry);
                        let denied = app["app_resolution"] == json!("denied");
                        if denied {
                            result.is_publish_restricted = true;
                        } else if aaa && app["app_resolution"] != json!("approved") {
                            let visibility = visibility.ok_or_else(invalid)?;
                            // FSSD's APP_HIDDEN is intentionally absent from
                            // 1njB's restricted set; do not silently change it.
                            if visibility.as_str() != Some("hidden") {
                                result.is_publish_restricted = true;
                            }
                        }
                    }
                }
                if let Some(children) = step["steps"].as_array() {
                    walk(children, apps, aaa, visibility, result)?;
                }
            }
            Ok(())
        }
        walk(
            array(workflow, "steps")?,
            &apps,
            aaa,
            visibility,
            &mut result,
        )?;
        Ok(result)
    }
}
