use super::*;

fn snapshot(core: &CoreAppState) -> bool {
    let w = &core.workspaces["T1"];
    w.directory_usergroups
        .snapshot(
            "fixture-account",
            &core.session.as_ref().unwrap().workspaces["T1"],
            core.account_epoch,
            &w.usergroups,
        )
        .is_ok()
}

#[test]
fn full_boot_binds_fresh_group_history_and_membership_but_picker_does_not() {
    let mut core = core();
    let mut raw = raw_boot();
    raw["subteams"] = json!({"self":["S1"]});
    assert!(
        begin(&mut core)
            .apply(&mut core, raw.clone(), false)
            .unwrap()
    );
    assert!(!snapshot(&core));
    assert!(full_read(&mut core).apply(&mut core, raw, true).unwrap());
    assert!(snapshot(&core));
    let w = &core.workspaces["T1"];
    assert_eq!(w.usergroups["S1"].users, ["U1"]);
    let observed = w
        .directory_usergroups
        .snapshot(
            "fixture-account",
            &core.session.as_ref().unwrap().workspaces["T1"],
            core.account_epoch,
            &w.usergroups,
        )
        .unwrap();
    assert!(observed.rows().is_empty());
    assert_eq!(observed.membership().unwrap(), ["S1"]);
}

#[test]
fn native_group_drift_refuses_source_binding_without_blocking_ordinary_boot() {
    let mut core = core();
    let read = full_read(&mut core);
    core.workspaces.get_mut("T1").unwrap().usergroups.insert(
        "S1".into(),
        crate::slack::models::UserGroup {
            id: "S1".into(),
            handle: "late".into(),
            ..Default::default()
        },
    );
    assert!(read.apply(&mut core, raw_boot(), true).unwrap());
    assert!(!snapshot(&core));
    assert_eq!(core.workspaces["T1"].usergroups["S1"].handle, "late");
}

#[test]
fn full_boot_cannot_promote_legacy_group_history() {
    let mut core = core();
    core.workspaces.get_mut("T1").unwrap().directory_usergroups = Default::default();
    assert!(
        full_read(&mut core)
            .apply(&mut core, raw_boot(), true)
            .unwrap()
    );
    assert!(!snapshot(&core));
}

#[test]
fn replaced_owner_cannot_seed_group_history() {
    let mut core = core();
    let read = full_read(&mut core);
    core.transport = Some(Arc::new(Transport::disconnected_fixture("fixture-cookie")));
    assert!(!read.apply(&mut core, raw_boot(), true).unwrap());
    assert!(!snapshot(&core));
}
