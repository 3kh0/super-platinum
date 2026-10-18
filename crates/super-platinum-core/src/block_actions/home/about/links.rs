//! FVzo ordinary Configuration and vpcw native authenticated Report links.
//! Opening these URLs is a user action; no request or app mutation occurs here.
use super::*;

fn workspace_url(workspace: &str, path: &str) -> Option<String> {
    let mut url = url::Url::parse(workspace).ok()?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return None;
    }
    url.set_path(path);
    url.set_query(None);
    url.set_fragment(None);
    Some(url.to_string())
}

impl Info {
    /// XCcf exact report visibility. Native 7MjS.gC uses web/open before the
    /// workspace-relative help path; the app ID is query encoded, never a path.
    pub fn report_url(&self, workspace: &str) -> Option<String> {
        if self.raw["is_directory_approved"] != true || self.raw["is_slack_integration"] == true {
            return None;
        }
        let mut url =
            url::Url::parse(&workspace_url(workspace, "/web/open/help/requests/new")?).ok()?;
        url.query_pairs_mut().append_pair("app_id", &self.app);
        Some(url.to_string())
    }
}

impl Profile {
    /// Actual apps.profile.get by app-ID has no serviceId parameter, so KNNB's
    /// service_id is empty. FVzo.BX selects the captured apps/<id> fallback.
    /// The service-specific branch requires a separately acquired service ID;
    /// an arbitrary similarly named raw response key must not invent one.
    pub fn configuration_url(
        &self,
        workspace: &str,
        top: &str,
        restricted: bool,
    ) -> Option<String> {
        let p = &self.raw()["app_profile"];
        let app = p["id"].as_str()?;
        let home = p["enterprise_id"]
            .as_str()
            .filter(|v| !v.is_empty())
            .or_else(|| p["team_id"].as_str())?;
        if restricted
            || home != top
            || policy::native_flow(app)
            || p["is_installed"] != true
            || p["is_workflow_app"] == true
            || p["sales_home_workflow_app_type"] == 1
        {
            return None;
        }
        let mut url = url::Url::parse(&workspace_url(workspace, "/")?).ok()?;
        url.path_segments_mut()
            .ok()?
            .pop_if_empty()
            .push("apps")
            .push(app);
        url.query_pairs_mut().append_pair("settings", "1");
        Some(url.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn report_matches_directory_visibility_and_native_authenticated_path() {
        let mut raw = json!({"ok":true,"app_id":"A1","is_directory_approved":true});
        let get = |v| {
            Info::decode("A1", v)
                .unwrap()
                .report_url("https://example.test/old?discard=1")
        };
        assert_eq!(
            get(raw.clone()).as_deref(),
            Some("https://example.test/web/open/help/requests/new?app_id=A1")
        );
        raw["is_slack_integration"] = json!(true);
        assert!(get(raw.clone()).is_none());
        raw["is_slack_integration"] = json!(false);
        raw["is_directory_approved"] = json!(false);
        assert!(get(raw).is_none());
    }
    #[test]
    fn exact_profile_scope_guest_install_and_workflow_boundaries_apply() {
        let base = json!({"ok":true,"app_profile":{"id":"A1","team_id":"T1","is_installed":true}});
        let get = |v, restricted| {
            Profile::decode("A1", v).unwrap().configuration_url(
                "https://example.test/",
                "T1",
                restricted,
            )
        };
        assert_eq!(
            get(base.clone(), false).as_deref(),
            Some("https://example.test/apps/A1?settings=1")
        );
        assert!(get(base.clone(), true).is_none());
        for (key, value) in [
            ("team_id", json!("T2")),
            ("is_installed", json!(false)),
            ("is_workflow_app", json!(true)),
            ("sales_home_workflow_app_type", json!(1)),
        ] {
            let mut raw = base.clone();
            raw["app_profile"][key] = value;
            assert!(get(raw, false).is_none());
        }
        let mut raw = base;
        raw["app_profile"]["service_id"] = json!("B_UNVERIFIED");
        assert_eq!(
            get(raw, false).as_deref(),
            Some("https://example.test/apps/A1?settings=1")
        );
    }
    #[test]
    fn workspace_links_refuse_credentials_or_insecure_hosts() {
        for url in [
            "https://user:secret@example.test",
            "http://example.test",
            "file:///tmp",
        ] {
            assert!(workspace_url(url, "/help").is_none());
        }
    }
}
