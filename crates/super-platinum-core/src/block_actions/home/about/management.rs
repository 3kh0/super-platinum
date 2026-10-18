//! FVzo/geao's workflow Configuration branch, separate from external settings.
use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorkflowEntry {
    Editor { app: String },
    Managed,
}
impl Profile {
    /// There is no loaded X4HU collaborator store in this native surface. geao
    /// consequently uses its actual About/profile owner_ids fallback. The caller
    /// supplies the observed parity assignment, not an inferred app capability.
    pub fn workflow_entry(
        &self,
        info: &Info,
        user: &str,
        top: &str,
        restricted: bool,
        parity: bool,
    ) -> Option<WorkflowEntry> {
        let p = &self.raw()["app_profile"];
        let app = p["id"].as_str()?;
        let home = p["enterprise_id"]
            .as_str()
            .filter(|v| !v.is_empty())
            .or_else(|| p["team_id"].as_str())?;
        if app != info.app
            || !id(user)
            || !id(top)
            || restricted
            || home != top
            || policy::native_flow(app)
            || p["is_installed"] != true
            || p["is_workflow_app"] != true
            || p["sales_home_workflow_app_type"] == 1
        {
            return None;
        }
        let owners = if parity {
            info.raw().get("owner_ids")
        } else {
            p.get("owner_ids")
        };
        let collaborator = match owners {
            None | Some(Value::Null) => false,
            Some(Value::Array(ids)) if ids.iter().all(|v| v.as_str().is_some_and(id)) => {
                ids.iter().any(|v| v.as_str() == Some(user))
            }
            _ => return None,
        };
        Some(if collaborator {
            WorkflowEntry::Editor { app: app.into() }
        } else {
            WorkflowEntry::Managed
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn about_owner_source_tracks_parity_and_keeps_noncollaborator_managed_route() {
        let profile = Profile::decode("A1", json!({"ok":true,"app_profile":{
            "id":"A1","team_id":"T1","is_installed":true,"is_workflow_app":true,"owner_ids":["U_LEGACY"]
        }})).unwrap();
        let info = Info::decode(
            "A1",
            json!({"ok":true,"app_id":"A1","owner_ids":["U_CURRENT"]}),
        )
        .unwrap();
        let editor = Some(WorkflowEntry::Editor { app: "A1".into() });
        assert_eq!(
            profile.workflow_entry(&info, "U_CURRENT", "T1", false, true),
            editor
        );
        assert_eq!(
            profile.workflow_entry(&info, "U_LEGACY", "T1", false, false),
            editor
        );
        assert_eq!(
            profile.workflow_entry(&info, "U_LEGACY", "T1", false, true),
            Some(WorkflowEntry::Managed)
        );
        assert!(
            profile
                .workflow_entry(&info, "U_CURRENT", "T2", false, true)
                .is_none()
        );
        assert!(
            profile
                .workflow_entry(&info, "U_CURRENT", "T1", true, true)
                .is_none()
        );
    }
    #[test]
    fn malformed_owners_never_silently_grant_or_select_a_route() {
        let base = json!({"ok":true,"app_profile":{"id":"A1","team_id":"T1","is_installed":true,"is_workflow_app":true}});
        let p = Profile::decode("A1", base).unwrap();
        for ids in [json!(true), json!("U1"), json!(["U1", false])] {
            let info =
                Info::decode("A1", json!({"ok":true,"app_id":"A1","owner_ids":ids})).unwrap();
            assert!(p.workflow_entry(&info, "U1", "T1", false, true).is_none());
        }
    }

    #[test]
    fn workflow_configuration_requires_matching_installed_native_scope() {
        let base = json!({"ok":true,"app_profile":{"id":"A1","team_id":"T1",
            "is_installed":true,"is_workflow_app":true}});
        let info = Info::decode("A1", json!({"ok":true,"app_id":"A1"})).unwrap();
        for (key, value) in [
            ("is_installed", json!(false)),
            ("is_workflow_app", json!(false)),
            ("sales_home_workflow_app_type", json!(1)),
            ("team_id", json!("T2")),
        ] {
            let mut raw = base.clone();
            raw["app_profile"][key] = value;
            let profile = Profile::decode("A1", raw).unwrap();
            assert!(
                profile
                    .workflow_entry(&info, "U1", "T1", false, true)
                    .is_none()
            );
        }
        let profile = Profile::decode("A1", base.clone()).unwrap();
        let other = Info::decode("A2", json!({"ok":true,"app_id":"A2"})).unwrap();
        assert!(
            profile
                .workflow_entry(&other, "U1", "T1", false, true)
                .is_none()
        );
        let mut enterprise = base.clone();
        enterprise["app_profile"]["enterprise_id"] = json!("E1");
        let profile = Profile::decode("A1", enterprise).unwrap();
        assert_eq!(
            profile.workflow_entry(&info, "U1", "E1", false, true),
            Some(WorkflowEntry::Managed)
        );
        assert!(
            profile
                .workflow_entry(&info, "U1", "T1", false, true)
                .is_none()
        );
        let mut native = base;
        native["app_profile"]["id"] = json!("A01126R8Y2X");
        let profile = Profile::decode("A01126R8Y2X", native).unwrap();
        let info = Info::decode("A01126R8Y2X", json!({"ok":true,"app_id":"A01126R8Y2X"})).unwrap();
        assert!(
            profile
                .workflow_entry(&info, "U1", "T1", false, true)
                .is_none()
        );
    }
}
