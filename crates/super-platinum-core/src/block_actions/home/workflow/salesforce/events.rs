//! GGBU virtual token mapping and t0MN actual realtime names; no external OAuth substitution.
use super::*;
impl Preflight {
    pub fn salesforce_event(
        &mut self,
        target: &Target,
        event: &Value,
        current: &View,
        w: &WorkspaceSession,
    ) -> Result<bool, Error> {
        if self.workflow() != &target.workflow || !target.current(current, w) {
            return Err(invalid());
        }
        let disconnected = event["type"] == "salesforce_org_disconnected";
        if !disconnected && event["type"] != "salesforce_org_authed" {
            return Ok(false);
        }
        if event["salesforce_org_id"].as_str() != Some(target.org())
            || event
                .get("user_id")
                .is_some_and(|u| u.as_str() != Some(&w.user_id))
        {
            return Ok(false);
        }
        let auth = self
            .auth
            .iter_mut()
            .find(|a| {
                a.function == target.auth.function
                    && a.provider == target.auth.provider
                    && a.app == target.auth.app
                    && a.salesforce_org.as_deref() == Some(target.org())
            })
            .ok_or_else(invalid)?;
        if disconnected {
            let virtual_default = auth
                .tokens
                .iter()
                .any(|t| t.default && t.id.starts_with("sat_"));
            auth.tokens.retain(|t| !t.id.starts_with("sat_"));
            if virtual_default {
                for (index, t) in auth.tokens.iter_mut().enumerate() {
                    t.default = index == 0;
                }
            }
        } else {
            if event["app_id"].as_str() != Some(&auth.app)
                || event["provider_key"].as_str() != Some(&auth.provider)
            {
                return Ok(false);
            }
            let id = bounded(event, "virtual_auth_token", 255)?;
            let user = bounded(event, "username", 8192)?;
            if auth.tokens.len() >= 100 && !auth.tokens.iter().any(|t| t.id == id) {
                return Err(invalid());
            }
            auth.tokens.retain(|t| t.id != id);
            for t in &mut auth.tokens {
                t.default = false;
            }
            auth.tokens.insert(
                0,
                AuthToken {
                    id,
                    user,
                    default: true,
                },
            );
        }
        self.auth_confirmed = false;
        Ok(true)
    }
}
