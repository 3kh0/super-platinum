//! The shared member read owns source publication; this token owns only the
//! original unopened description and its right to resume after publication.
use super::*;
pub(super) mod lease;
use crate::directory_member_fetches::driver::Client;
use super_platinum_core::block_actions::{
    directory_endpoint::member_info::unknown::PlaceholderAsset,
    workflow_management::member_hydration::RendererReply,
};

pub(super) struct MissingOpening {
    token: Token,
    visit: u64,
    serial: u64,
    opening: u64,
    id: String,
    key: u64,
    before: Vec<Value>,
    ids: Vec<String>,
    groups: Vec<String>,
}
impl MissingOpening {
    pub(super) fn capture(
        shell: &ShellState,
        visit: u64,
        serial: u64,
        id: &str,
        key: u64,
    ) -> Option<Self> {
        if !editable(shell, visit, serial) {
            return None;
        }
        let draft = shell.profile_edit.sections.draft.as_ref()?;
        if draft.rich.is_some() {
            return None;
        }
        let SectionValue::LongText(before) = draft.form.as_ref()?.value(id)? else {
            return None;
        };
        let mut ids = Vec::new();
        let mut groups = Vec::new();
        for raw in collect(before).ok()? {
            if raw["type"] == "user" {
                let id = raw["user_id"].as_str()?.to_owned();
                if !ids.contains(&id) {
                    ids.push(id);
                }
            }
            if raw["type"] == "usergroup" {
                let id = raw["usergroup_id"].as_str()?.to_owned();
                if !groups.contains(&id) {
                    groups.push(id);
                }
            }
        }
        (!ids.is_empty() || !groups.is_empty()).then(|| Self {
            token: draft.token.clone(),
            visit,
            serial,
            opening: draft.rich_opening,
            id: id.into(),
            key,
            before: before.clone(),
            ids,
            groups,
        })
    }
    fn current(&self, shell: &ShellState) -> bool {
        self.token.current(shell)
            && editable(shell, self.visit, self.serial)
            && shell.profile_edit.sections.draft.as_ref().is_some_and(|d| {
                d.token == self.token
                    && d.rich_opening == self.opening
                    && d.rich.is_none()
                    && d.form.as_ref().and_then(|f| f.value(&self.id))
                        == Some(&SectionValue::LongText(self.before.clone()))
            })
    }
    pub(super) fn start(self, mut state: Signal<ShellState>, known: bool, automatic_members: bool) {
        if !self.current(&state.peek()) {
            return;
        }
        let client = if self.ids.is_empty() {
            None
        } else {
            let Ok(client) = Client::begin(state, &self.token.owner.team, &self.ids) else {
                return;
            };
            client
        };
        if client.is_none() && !known && !automatic_members {
            return;
        }
        let (identity, mut cancellation) = {
            let mut shell = state.write();
            if !self.current(&shell) {
                return;
            }
            let d = shell.profile_edit.sections.draft.as_mut().unwrap();
            retire_recovery(d);
            let (lease, identity, receiver) = lease::Lease::new();
            d.rich_recovery = Some(lease);
            d.rich_reading = known || automatic_members;
            (identity, receiver)
        };
        // Cancellation ends only this consumer. Shared acquisition remains owned
        // by its account runtime and may settle for another current subscriber.
        spawn(async move {
            let result = tokio::select! {
                biased;
                _=lease::cancelled(&mut cancellation)=>Err(unavailable()),
                result=self.run(state,client,known,automatic_members)=>result,
            };
            self.resume(state, result.is_ok(), known, &identity);
        });
    }
    async fn run(
        &self,
        state: Signal<ShellState>,
        client: Option<Client>,
        known: bool,
        automatic_members: bool,
    ) -> Result<(), Error> {
        if let Some(client) = client {
            self.absent(state, client).await?;
        }
        if automatic_members && !known {
            crate::directory_member_fetches::automatic::wait(
                state,
                &self.token.owner.team,
                &self.ids,
                |shell| self.current(shell),
            )
            .await?;
        }
        if known {
            if !self.current(&state.peek()) {
                return Err(unavailable());
            }
            let ids = if self.ids.is_empty() {
                Vec::new()
            } else {
                opening::known_recovery_ids(&state.peek(), &self.token, self.opening, &self.ids)?
            };
            if !ids.is_empty() {
                crate::directory_member_fetches::known::recover(
                    state,
                    &self.token.owner.team,
                    &ids,
                    |shell| self.current(shell),
                )
                .await?;
            }
            let ids = self.group_recovery_ids(&state.peek())?;
            if !ids.is_empty() {
                crate::usergroups::recovery::recover(
                    state,
                    &self.token.owner.team,
                    &ids,
                    |shell| self.current(shell),
                )
                .await?;
            }
        }
        Ok(())
    }
    fn group_recovery_ids(&self, shell: &ShellState) -> Result<Vec<String>, Error> {
        if !self.current(shell) {
            return Err(unavailable());
        }
        let workspace = shell
            .core
            .workspaces
            .get(&self.token.owner.team)
            .ok_or_else(unavailable)?;
        Ok(self
            .groups
            .iter()
            .filter(|id| {
                workspace
                    .usergroups
                    .get(*id)
                    .is_none_or(|group| group.id != **id || group.profile_label().is_none())
            })
            .cloned()
            .collect())
    }
    async fn absent(&self, mut state: Signal<ShellState>, mut client: Client) -> Result<(), Error> {
        while let Some(mut election) = client.next(|shell| self.current(shell)).await? {
            let flags = crate::app_home::channel_fetches::member_flags(
                &state.peek(),
                &self.token.owner.team,
            )?;
            let generation = format!("profile-missing-member-{}", self.opening);
            let asset = PlaceholderAsset::observed("https://a.slack-edge.com/bv1-13/")?;
            let mut stage = election.stage(
                &state.peek(),
                flags.flags().clone(),
                generation.clone(),
                &asset,
            )?;
            loop {
                let pending = stage.prepare(&state.peek(), &election)?;
                let Some(pending) = pending else { break };
                let fields = pending.fields().to_vec();
                let lowered = crate::app_home::channel_fetches::member_fields(
                    state,
                    |shell| self.current(shell) && flags.current(shell) && election.current(shell),
                    &generation,
                    &fields,
                )
                .await?;
                let reply = RendererReply {
                    generation: pending.generation().into(),
                    operation: "ordinary-toLowerCase".into(),
                    fields,
                    lowered,
                };
                stage.apply(&state.peek(), &election, pending, reply)?;
            }
            stage.install(&mut state.write(), &mut election, |shell| {
                self.current(shell) && flags.current(shell)
            })?;
        }
        Ok(())
    }
    fn cleanup(&self, shell: &mut ShellState, identity: &Arc<()>) -> bool {
        if !self.token.current(shell) {
            return false;
        }
        let Some(d) = shell.profile_edit.sections.draft.as_mut() else {
            return false;
        };
        if d.token != self.token
            || d.visit != self.visit
            || d.serial != self.serial
            || d.rich_opening != self.opening
            || d.rich.is_some()
            || !d
                .rich_recovery
                .as_ref()
                .is_some_and(|lease| lease.owns(identity))
        {
            return false;
        }
        retire_recovery(d);
        true
    }
    fn resume(self, mut state: Signal<ShellState>, success: bool, known: bool, identity: &Arc<()>) {
        let hits = {
            let mut shell = state.write();
            let may_resume = self.current(&shell);
            if !self.cleanup(&mut shell, identity) || !may_resume || !success {
                return;
            }
            let (opening, hits) = opening::tracked_with_members(
                &mut shell,
                self.visit,
                self.serial,
                &self.id,
                self.key,
                !known,
            );
            if let Ok(opening) = opening {
                finish(&mut shell, opening);
            }
            hits
        };
        for hit in hits {
            hit.schedule(state);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn prepared() -> (ShellState, MissingOpening) {
        let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
        fixture::install_interaction(&mut shell);
        let d = shell.profile_edit.sections.draft.as_ref().unwrap();
        let (visit, serial) = (d.visit, d.serial);
        let raw = vec![
            json!({"type":"rich_text","elements":[{"type":"rich_text_section","elements":[
                {"type":"text","text":"Keep my original description "},
                {"type":"user","user_id":"UMISSING01","opaque":1},
                {"type":"user","user_id":"UMISSING01","opaque":2}
            ]}]}),
        ];
        assert!(change_shell(
            &mut shell,
            visit,
            serial,
            "long",
            SectionValue::LongText(raw)
        ));
        assert!(opening::begin(&mut shell, visit, serial, "long", 7).is_err());
        let captured = MissingOpening::capture(&shell, visit, serial, "long", 7).unwrap();
        (shell, captured)
    }
    #[test]
    fn duplicate_occurrences_share_read_without_changing_original_draft() {
        let (shell, captured) = prepared();
        assert_eq!(captured.ids, ["UMISSING01"]);
        assert_eq!(collect(&captured.before).unwrap().len(), 2);
        assert!(captured.current(&shell));
    }
    #[test]
    fn superseding_opening_or_edit_revokes_auto_resume() {
        let (mut shell, captured) = prepared();
        let _ = opening::begin(&mut shell, captured.visit, captured.serial, "long", 8);
        assert!(!captured.current(&shell));
        let (mut shell, captured) = prepared();
        assert!(change_shell(
            &mut shell,
            captured.visit,
            captured.serial,
            "long",
            SectionValue::LongText(vec![])
        ));
        assert!(!captured.current(&shell));
    }
    #[test]
    fn account_or_section_replacement_revokes_auto_resume() {
        let (mut shell, captured) = prepared();
        shell.core.account_epoch += 1;
        assert!(!captured.current(&shell));
        let (mut shell, captured) = prepared();
        shell.profile_edit.sections.draft = None;
        assert!(!captured.current(&shell));
    }

    #[test]
    fn waiting_does_not_change_original_description_or_authorize_a_mounted_editor() {
        let (mut shell, captured) = prepared();
        let before = captured.before.clone();
        shell
            .profile_edit
            .sections
            .draft
            .as_mut()
            .unwrap()
            .rich_reading = true;
        assert!(captured.current(&shell));
        assert!(
            shell
                .profile_edit
                .sections
                .draft
                .as_ref()
                .unwrap()
                .form
                .as_ref()
                .unwrap()
                .value("long")
                == Some(&SectionValue::LongText(before.clone()))
        );
        // Even an editor for the same field/blocks owns its own native buffer.
        // A completed read for the prior unopened attempt cannot replace it.
        shell.profile_edit.sections.draft.as_mut().unwrap().rich = Some(Session {
            id: captured.id.clone(),
            key: captured.key,
            before,
            catalog: catalog::Catalog::default(),
            labels: "{}".into(),
            notice: None,
            query: None,
        });
        assert!(!captured.current(&shell));
    }

    #[test]
    fn section_write_recovery_retires_automatic_opening_continuation() {
        let (mut shell, captured) = prepared();
        assert!(captured.current(&shell));
        let _attempt =
            crate::profile_edit::sections::begin(&mut shell, captured.visit, captured.serial)
                .unwrap();
        assert!(!captured.current(&shell));
        assert!(
            shell
                .profile_edit
                .sections
                .draft
                .as_ref()
                .unwrap()
                .form
                .as_ref()
                .unwrap()
                .value("long")
                == Some(&SectionValue::LongText(captured.before))
        );
    }

    fn install_lease(shell: &mut ShellState) -> (Arc<()>, tokio::sync::watch::Receiver<bool>) {
        let (lease, identity, receiver) = lease::Lease::new();
        let d = shell.profile_edit.sections.draft.as_mut().unwrap();
        d.rich_recovery = Some(lease);
        d.rich_reading = true;
        (identity, receiver)
    }
    #[tokio::test]
    async fn explicit_retirement_wakes_even_if_a_pane_clone_retains_sender() {
        let (mut shell, captured) = prepared();
        let (identity, mut receiver) = install_lease(&mut shell);
        let retained = shell.profile_edit.sections.draft.clone();
        retire_recovery(shell.profile_edit.sections.draft.as_mut().unwrap());
        tokio::time::timeout(
            std::time::Duration::from_millis(100),
            lease::cancelled(&mut receiver),
        )
        .await
        .unwrap();
        assert!(
            !shell
                .profile_edit
                .sections
                .draft
                .as_ref()
                .unwrap()
                .rich_reading
        );
        assert!(!captured.cleanup(&mut shell, &identity));
        drop(retained);
    }
    #[tokio::test]
    async fn removed_pane_closes_consumer_without_any_member_response() {
        let (mut shell, _) = prepared();
        let (_, mut receiver) = install_lease(&mut shell);
        shell.profile_edit.sections.draft = None;
        tokio::time::timeout(
            std::time::Duration::from_millis(100),
            lease::cancelled(&mut receiver),
        )
        .await
        .unwrap();
    }
    #[test]
    fn old_completion_cannot_clear_new_lease_even_for_same_opening_number() {
        let (mut shell, captured) = prepared();
        let (old, _) = install_lease(&mut shell);
        let (new, _) = install_lease(&mut shell);
        assert!(!captured.cleanup(&mut shell, &old));
        assert!(
            shell
                .profile_edit
                .sections
                .draft
                .as_ref()
                .unwrap()
                .rich_reading
        );
        assert!(captured.cleanup(&mut shell, &new));
    }
    #[test]
    fn own_cleanup_releases_reading_but_changed_blocks_never_resume() {
        let (mut shell, captured) = prepared();
        let (identity, _) = install_lease(&mut shell);
        shell
            .profile_edit
            .sections
            .draft
            .as_mut()
            .unwrap()
            .form
            .as_mut()
            .unwrap()
            .change("long", SectionValue::LongText(vec![]))
            .unwrap();
        assert!(!captured.current(&shell));
        assert!(captured.cleanup(&mut shell, &identity));
        assert!(
            !shell
                .profile_edit
                .sections
                .draft
                .as_ref()
                .unwrap()
                .rich_reading
        );
        assert!(
            shell
                .profile_edit
                .sections
                .draft
                .as_ref()
                .unwrap()
                .rich
                .is_none()
        );
    }
    #[tokio::test]
    async fn actual_field_change_retires_waiter_without_pending_io() {
        let (mut shell, captured) = prepared();
        let (_, mut receiver) = install_lease(&mut shell);
        assert!(change_shell(
            &mut shell,
            captured.visit,
            captured.serial,
            "long",
            SectionValue::LongText(vec![])
        ));
        tokio::time::timeout(
            std::time::Duration::from_millis(100),
            lease::cancelled(&mut receiver),
        )
        .await
        .unwrap();
        assert!(!captured.current(&shell));
        assert!(
            !shell
                .profile_edit
                .sections
                .draft
                .as_ref()
                .unwrap()
                .rich_reading
        );
    }
    #[tokio::test]
    async fn actual_competing_write_retires_consumer_without_replaying_it() {
        let (mut shell, captured) = prepared();
        let (_, mut receiver) = install_lease(&mut shell);
        let attempt =
            crate::profile_edit::sections::begin(&mut shell, captured.visit, captured.serial)
                .unwrap();
        tokio::time::timeout(
            std::time::Duration::from_millis(100),
            lease::cancelled(&mut receiver),
        )
        .await
        .unwrap();
        assert!(!captured.current(&shell));
        assert!(
            !shell
                .profile_edit
                .sections
                .draft
                .as_ref()
                .unwrap()
                .rich_reading
        );
        assert_eq!(
            shell.profile_edit.sections.entries[&attempt.token.owner]
                .pending
                .as_ref()
                .unwrap()
                .id,
            attempt.id
        );
    }
    #[test]
    fn socket_generation_alone_does_not_retire_original_profile_draft() {
        let (mut shell, captured) = prepared();
        let (identity, _) = install_lease(&mut shell);
        shell
            .core
            .workspaces
            .get_mut(&captured.token.owner.team)
            .unwrap()
            .rt_generation += 1;
        assert!(captured.current(&shell));
        assert!(captured.cleanup(&mut shell, &identity));
    }

    fn groups() -> (ShellState, MissingOpening) {
        let (mut shell, previous) = prepared();
        let raw = vec![json!({"type":"rich_text","elements":[
            {"type":"rich_text_section","elements":[
                {"type":"usergroup","usergroup_id":"SABSENT01","opaque":1},
                {"type":"usergroup","usergroup_id":"SABSENT01","opaque":2},
                {"type":"usergroup","usergroup_id":"SLEGACY01"},
                {"type":"usergroup","usergroup_id":"SREADY001"}
            ]}
        ]})];
        assert!(change_shell(
            &mut shell,
            previous.visit,
            previous.serial,
            "long",
            SectionValue::LongText(raw)
        ));
        assert!(opening::begin(&mut shell, previous.visit, previous.serial, "long", 8).is_err());
        let captured =
            MissingOpening::capture(&shell, previous.visit, previous.serial, "long", 8).unwrap();
        let w = shell
            .core
            .workspaces
            .get_mut(&captured.token.owner.team)
            .unwrap();
        w.usergroups.insert(
            "SLEGACY01".into(),
            serde_json::from_value(json!({"id":"SLEGACY01","handle":"cached"})).unwrap(),
        );
        w.usergroups.insert(
            "SREADY001".into(),
            super_platinum_core::slack::models::UserGroup::from_observed(
                &json!({"id":"SREADY001","handle":""}),
            )
            .unwrap(),
        );
        (shell, captured)
    }
    #[test]
    fn group_only_retry_selects_absent_and_legacy_but_keeps_observed_empty_label() {
        let (shell, captured) = groups();
        assert!(captured.ids.is_empty());
        assert_eq!(captured.groups, ["SABSENT01", "SLEGACY01", "SREADY001"]);
        assert_eq!(collect(&captured.before).unwrap().len(), 4);
        assert_eq!(
            captured.group_recovery_ids(&shell).unwrap(),
            ["SABSENT01", "SLEGACY01"]
        );
    }
    #[test]
    fn newly_observed_group_no_longer_needs_retry_without_mutating_description() {
        let (mut shell, captured) = groups();
        let before = captured.before.clone();
        shell
            .core
            .workspaces
            .get_mut(&captured.token.owner.team)
            .unwrap()
            .usergroups
            .insert(
                "SLEGACY01".into(),
                super_platinum_core::slack::models::UserGroup::from_observed(
                    &json!({"id":"SLEGACY01","handle":"fresh"}),
                )
                .unwrap(),
            );
        assert_eq!(captured.group_recovery_ids(&shell).unwrap(), ["SABSENT01"]);
        assert!(captured.current(&shell));
        assert_eq!(captured.before, before);
    }
    #[test]
    fn retired_group_description_cannot_select_another_read() {
        let (mut shell, captured) = groups();
        shell
            .profile_edit
            .sections
            .draft
            .as_mut()
            .unwrap()
            .rich_opening += 1;
        assert!(captured.group_recovery_ids(&shell).is_err());
    }
}
