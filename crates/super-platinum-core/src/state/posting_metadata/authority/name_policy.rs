//! Read-only complete name-policy inputs from the actual bound boot owner.
//! OrdinaryClientV2 is the explicit client constructor/default policy. lahp then
//! sets teamId to the logical store ID and enterpriseId only to configured E;
//! boot.enterprise_id is not a substitute for that selector.
use super::*;
use crate::block_actions::name_policy::{Membership, OrdinaryClientV2};
use std::collections::BTreeMap;
impl Authority {
    pub fn name_policy_membership(
        &self,
        core: &CoreAppState,
        _: OrdinaryClientV2,
    ) -> Result<(Membership, Option<String>)> {
        let metadata = self.metadata(core)?;
        let ids = metadata
            .internal_workspaces
            .as_ref()
            .ok_or(Unknown::Membership)?;
        let teams = ids
            .iter()
            .map(|id| {
                metadata
                    .workspaces
                    .get(id)
                    .cloned()
                    .map(|raw| (id.clone(), raw))
                    .ok_or(Unknown::Membership)
            })
            .collect::<Result<BTreeMap<_, _>>>()?;
        let enterprise = self.scope.session.enterprise_id.clone();
        let membership = Membership::observe(enterprise.as_deref(), ids, &teams)
            .map_err(|_| Unknown::Membership)?;
        Ok((membership, enterprise))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::Session, state::Workspace};
    use serde_json::json;
    fn acquired(configured: Option<&str>) -> (CoreAppState, Authority) {
        let credential = WorkspaceSession {
            team_id: "T1".into(),
            enterprise_id: configured.map(str::to_owned),
            user_id: "U1".into(),
            name: "Synthetic".into(),
            url: "https://fixture.slack.com".into(),
            token: "synthetic".into(),
        };
        let transport = Arc::new(Transport::disconnected_fixture("synthetic"));
        let mut core = CoreAppState::new(Default::default());
        core.active_account = Some("fixture-account".into());
        core.transport = Some(transport.clone());
        core.workspaces
            .insert("T1".into(), Workspace::from_session(&credential));
        core.session = Some(Session {
            d_cookie: "synthetic".into(),
            unified_user_client_enabled: BTreeMap::from([("T1".into(), true)]),
            workspaces: BTreeMap::from([("T1".into(), credential.clone())]),
        });
        let read = BootRead::begin(&mut core, &transport, &credential).unwrap();
        assert!(read.apply(&mut core,json!({"ok":true,"team":{"id":"T1","enterprise_id":"EBOOT","plan":"plus"},"self":{"id":"U1","team_id":"T1"},"workspaces":[{"id":"T1","enterprise_id":"EBOOT"},{"id":"T2","enterprise_id":"EBOOT"},{"id":"T_EXTERNAL","allow_external_users":true}],"read_only_channels":[],"thread_only_channels":[]}),false).unwrap());
        let authority = Authority::capture(&core, "T1").unwrap();
        (core, authority)
    }
    #[test]
    fn complete_internal_observations_use_configured_enterprise_not_boot_fallback() {
        for configured in [None, Some("ECONFIG")] {
            let (core, authority) = acquired(configured);
            let (membership, enterprise) = authority
                .name_policy_membership(&core, OrdinaryClientV2)
                .unwrap();
            assert_eq!(enterprise.as_deref(), configured);
            assert_eq!(
                membership.workspace_ids().collect::<Vec<_>>(),
                vec!["T1", "T2"]
            );
        }
    }
    #[test]
    fn replacement_credentials_or_incomplete_retained_team_map_refuse() {
        let (mut core, authority) = acquired(Some("E1"));
        core.workspaces
            .get_mut("T1")
            .unwrap()
            .posting_metadata
            .workspaces
            .remove("T2");
        assert!(
            authority
                .name_policy_membership(&core, OrdinaryClientV2)
                .is_err()
        );
        let (mut core, authority) = acquired(Some("E1"));
        core.session
            .as_mut()
            .unwrap()
            .workspaces
            .get_mut("T1")
            .unwrap()
            .token = "replacement".into();
        assert!(
            authority
                .name_policy_membership(&core, OrdinaryClientV2)
                .is_err()
        );
    }
}
