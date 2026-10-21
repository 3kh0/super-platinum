//! bD6x/61UB acquisition. Missing matches default true only after a complete read.
use super::*;

#[derive(Clone, Debug, PartialEq)]
pub struct TriggerTypes {
    raw: Value,
}
impl TriggerTypes {
    pub fn request(app_id: Option<&str>) -> Result<RequestPlan, Error> {
        let args = match app_id {
            Some(value) if id(value) => json!({"app_id":value}),
            Some(_) => return Err(invalid()),
            None => json!({}),
        };
        Ok(RequestPlan::new(
            "workflows.triggers.types.list",
            "fetching-workflow-trigger-types-list-wfb",
            args,
        ))
    }
    pub fn decode(raw: Value) -> Result<Self, Error> {
        success(&raw)?;
        for row in array(&raw, "trigger_types")? {
            identifier(&row["trigger_type"], "id")?;
            // JS accepts truthy objects too. Native admission deliberately
            // supports booleans/nullish only, retaining all original fields.
            if row
                .get("is_available_to_user")
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
    pub fn availability(&self, trigger: Option<&Value>) -> Result<bool, Error> {
        let Some(trigger) = trigger else {
            return Ok(true);
        };
        if !trigger.is_object() {
            return Err(invalid());
        }
        let selected = trigger.get("trigger_type_id");
        let row = array(&self.raw, "trigger_types")?
            .iter()
            .find(|row| row["trigger_type"].get("id") == selected);
        Ok(row
            .and_then(|row| row["is_available_to_user"].as_bool())
            .unwrap_or(true))
    }
}
