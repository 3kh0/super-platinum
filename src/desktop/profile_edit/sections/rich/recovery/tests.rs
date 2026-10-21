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
        preferences: None,
    });
    assert!(!captured.current(&shell));
}

#[test]
fn section_write_recovery_retires_automatic_opening_continuation() {
    let (mut shell, captured) = prepared();
    assert!(captured.current(&shell));
    let _attempt =
        crate::profile_edit::sections::begin(&mut shell, captured.visit, captured.serial).unwrap();
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
        crate::profile_edit::sections::begin(&mut shell, captured.visit, captured.serial).unwrap();
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

fn recapture(shell: &ShellState, previous: &MissingOpening) -> MissingOpening {
    MissingOpening::capture(
        shell,
        previous.visit,
        previous.serial,
        &previous.id,
        previous.key,
    )
    .unwrap()
}
#[test]
fn automatic_groups_are_only_original_absence_and_keep_legacy_retry_distinct() {
    let (shell, previous) = groups();
    let captured = recapture(&shell, &previous);
    assert_eq!(captured.absent_groups, ["SABSENT01"]);
    assert_eq!(
        captured.absent_group_recovery_ids(&shell).unwrap(),
        ["SABSENT01"]
    );
    assert_eq!(
        captured.group_recovery_ids(&shell).unwrap(),
        ["SABSENT01", "SLEGACY01"]
    );
    assert_eq!(collect(&captured.before).unwrap().len(), 4);
    assert_eq!(captured.before, previous.before);
}
#[test]
fn automatic_wait_skips_newly_present_rows_and_does_not_expand_to_new_absence() {
    let (mut shell, previous) = groups();
    let captured = recapture(&shell, &previous);
    let w = shell
        .core
        .workspaces
        .get_mut(&captured.token.owner.team)
        .unwrap();
    // Even an unavailable legacy row arriving mid-wait is a present source,
    // while deletion of an originally present row is not this opening's demand.
    w.usergroups.insert(
        "SABSENT01".into(),
        serde_json::from_value(json!({"id":"SABSENT01","handle":"legacy arrived"})).unwrap(),
    );
    w.usergroups.remove("SLEGACY01");
    assert!(
        captured
            .absent_group_recovery_ids(&shell)
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        captured.group_recovery_ids(&shell).unwrap(),
        ["SABSENT01", "SLEGACY01"]
    );
    assert!(captured.current(&shell));
}
#[test]
fn present_mismatched_or_malformed_group_never_becomes_automatic_absence() {
    let (mut shell, previous) = groups();
    let w = shell
        .core
        .workspaces
        .get_mut(&previous.token.owner.team)
        .unwrap();
    w.usergroups.insert(
        "SABSENT01".into(),
        serde_json::from_value(json!({"id":"SDIFFERENT","handle":"wrong identity"})).unwrap(),
    );
    let captured = recapture(&shell, &previous);
    assert!(captured.absent_groups.is_empty());
    assert!(
        captured
            .absent_group_recovery_ids(&shell)
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        captured.group_recovery_ids(&shell).unwrap(),
        ["SABSENT01", "SLEGACY01"]
    );
}
#[test]
fn mixed_references_keep_original_occurrences_while_deduplicating_absent_groups() {
    let (mut shell, previous) = groups();
    let mut raw = previous.before.clone();
    raw[0]["elements"][0]["elements"].as_array_mut().unwrap().extend([
        json!({"type":"user","user_id":"UMISSING01","opaque":{"keep":1}}),
        json!({"type":"canvas","file_id":"FFILE0001","url":"https://example.test/file","text":"Original file"}),
    ]);
    assert!(change_shell(
        &mut shell,
        previous.visit,
        previous.serial,
        "long",
        SectionValue::LongText(raw.clone())
    ));
    let captured = recapture(&shell, &previous);
    assert_eq!(captured.absent_groups, ["SABSENT01"]);
    assert_eq!(captured.ids, ["UMISSING01"]);
    assert_eq!(captured.files, ["FFILE0001"]);
    assert_eq!(captured.before, raw);
}
#[test]
fn failed_or_partial_group_cleanup_is_consumed_without_installing_an_editor() {
    let (mut shell, previous) = groups();
    let captured = recapture(&shell, &previous);
    let before = captured.before.clone();
    let (identity, _) = install_lease(&mut shell);
    assert!(captured.cleanup(&mut shell, &identity));
    assert!(!captured.cleanup(&mut shell, &identity));
    let d = shell.profile_edit.sections.draft.as_ref().unwrap();
    assert!(!d.rich_reading);
    assert!(d.rich_recovery.is_none());
    assert!(d.rich.is_none());
    assert!(d.form.as_ref().unwrap().value("long") == Some(&SectionValue::LongText(before)));
    // An ordinary opening capture still refuses partial/unavailable labels;
    // cleanup has neither synthesized terminal groups nor mounted an editor.
    assert!(
        opening::begin(
            &mut shell,
            captured.visit,
            captured.serial,
            "long",
            captured.key
        )
        .is_err()
    );
}
fn original_group_context() -> (ShellState, MissingOpening, Context) {
    let (mut shell, previous) = groups();
    let owner = Owner::capture(&shell).unwrap();
    let session = crate::profile_edit::fixture::session(&owner);
    shell.core.session = Some(super_platinum_core::config::Session {
        d_cookie: "synthetic-automatic-groups".into(),
        unified_user_client_enabled: Default::default(),
        workspaces: std::collections::BTreeMap::from([(owner.team.clone(), session)]),
    });
    shell.core.transport = Some(Arc::new(
        super_platinum_core::slack::Transport::disconnected_fixture("synthetic-automatic-groups"),
    ));
    let token = Token::capture(&shell).unwrap();
    shell.profile_edit.draft.as_mut().unwrap().token = token.clone();
    shell.profile_edit.sections.draft.as_mut().unwrap().token = token;
    let captured = recapture(&shell, &previous);
    let context = Context::capture(&shell, &captured.token.owner.team).unwrap();
    (shell, captured, context)
}
#[test]
fn automatic_group_owner_retires_on_socket_or_credentials_without_rebinding_draft() {
    let (mut shell, captured, owner) = original_group_context();
    assert!(captured.automatic_group_current(&shell, &owner));
    shell
        .core
        .workspaces
        .get_mut(&captured.token.owner.team)
        .unwrap()
        .rt_generation += 1;
    assert!(captured.current(&shell)); // existing durable Profile custody
    assert!(!captured.automatic_group_current(&shell, &owner));
    let (mut shell, captured, owner) = original_group_context();
    shell.core.session.as_mut().unwrap().d_cookie = "replaced".into();
    assert!(!captured.automatic_group_current(&shell, &owner));
}
#[test]
fn automatic_group_owner_refuses_changed_description_and_account() {
    let (mut shell, captured, owner) = original_group_context();
    assert!(change_shell(
        &mut shell,
        captured.visit,
        captured.serial,
        "long",
        SectionValue::LongText(vec![])
    ));
    assert!(!captured.automatic_group_current(&shell, &owner));
    assert!(captured.absent_group_recovery_ids(&shell).is_err());
    let (mut shell, captured, owner) = original_group_context();
    shell.core.account_epoch += 1;
    assert!(!captured.automatic_group_current(&shell, &owner));
}

#[test]
fn section_save_cooldown_is_not_group_read_debt_and_group_poll_does_not_clear_it() {
    let (mut shell, captured, _) = original_group_context();
    let deadline = now() + 60;
    shell
        .profile_edit
        .sections
        .entries
        .entry(captured.token.owner.clone())
        .or_default()
        .retry_at = deadline;
    assert!(captured.group_rate(&shell).is_none());
    assert!(!poll_group_rate(&shell));
    assert_eq!(
        shell.profile_edit.sections.entries[&captured.token.owner].retry_at,
        deadline
    );
    assert!(captured.current(&shell));
    assert_eq!(
        captured.before,
        match shell
            .profile_edit
            .sections
            .draft
            .as_ref()
            .unwrap()
            .form
            .as_ref()
            .unwrap()
            .value(&captured.id)
            .unwrap()
        {
            SectionValue::LongText(blocks) => blocks.clone(),
            _ => panic!("original description changed"),
        }
    );
}
