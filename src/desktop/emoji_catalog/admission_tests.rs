use super::*;

fn shell() -> ShellState {
    ShellState::fixture(crate::media::MediaRegistry::default())
}

fn install_ready(shell: &mut ShellState) -> Arc<Loaded> {
    let owner = Owner::capture(shell).unwrap();
    let workspace = &shell.core.workspaces[&owner.team];
    let (catalog, picker) = build(Vec::new()).unwrap();
    let loaded = Arc::new(Loaded {
        owner: owner.clone(),
        revision: workspace.custom_emoji_revision,
        locale: workspace.locale.clone(),
        token: "held-ready".into(),
        catalog,
        picker,
    });
    shell
        .emoji_catalog
        .ready
        .insert(owner.team.clone(), loaded.clone());
    loaded
}

#[test]
fn fallible_capture_cold_and_ready_leave_all_admission_state_untouched() {
    for ready in [false, true] {
        for error in ["injected checked size overflow", "injected reserve failure"] {
            let mut shell = shell();
            let held = ready.then(|| install_ready(&mut shell));
            let before = probe_state(&shell);
            let mut called = 0;
            let result = prepare_local(
                &mut shell,
                |_| {
                    called += 1;
                    Err(error.into())
                },
                true,
            );
            assert_eq!(called, 1);
            let Prepared::Ready(returned) = result else {
                panic!("failure reserved build")
            };
            assert_eq!(probe_state(&shell), before);
            assert!(shell.emoji_catalog.building.is_empty());
            assert!(shell.emoji_catalog.attempted.is_empty());
            match (returned, held) {
                (Some(returned), Some(held)) => assert!(Arc::ptr_eq(&returned, &held)),
                (None, None) => {}
                _ => panic!("ready fallback changed"),
            }
            let next = prepare_local(
                &mut shell,
                |_| Ok(snapshot::Snapshot::Current(Vec::new())),
                true,
            );
            let Prepared::Build { ticket, owner, .. } = next else {
                panic!("failure stranded next attempt")
            };
            assert_eq!(ticket, 1);
            assert_eq!(shell.emoji_catalog.building[&owner.team].ticket, 1);
            assert_eq!(shell.emoji_catalog.attempted.len(), 1);
        }
    }
}

#[test]
fn fast_paths_do_not_invoke_capture_or_increment_tickets() {
    let mut shell = shell();
    let held = install_ready(&mut shell);
    let result = prepare_local(&mut shell, |_| panic!("valid ready captured"), false);
    let Prepared::Ready(Some(ready)) = result else {
        panic!("missing ready")
    };
    assert!(Arc::ptr_eq(&held, &ready));
    assert_eq!(shell.emoji_catalog.serial, 0);
    shell.emoji_catalog.ready.clear();
    let pending = prepare_local(
        &mut shell,
        |_| Ok(snapshot::Snapshot::Current(Vec::new())),
        false,
    );
    assert!(matches!(pending, Prepared::Build { ticket: 1, .. }));
    assert!(matches!(
        prepare_local(&mut shell, |_| panic!("busy captured"), true),
        Prepared::Ready(None)
    ));
    assert_eq!(shell.emoji_catalog.serial, 1);
}

#[test]
fn old_failure_cannot_retire_replacement_and_owner_changes_refuse_publication() {
    for change in 0..4 {
        let mut shell = shell();
        let Prepared::Build {
            owner,
            revision,
            locale,
            ticket,
            ..
        } = prepare_local(
            &mut shell,
            |_| Ok(snapshot::Snapshot::Current(Vec::new())),
            false,
        )
        else {
            panic!("not reserved")
        };
        assert!(building_current(&shell, &owner, revision, &locale, ticket));
        match change {
            0 => shell.core.account_epoch += 1,
            1 => shell.core.active_account = Some("another-account".into()),
            2 => {
                shell
                    .core
                    .workspaces
                    .get_mut(&owner.team)
                    .unwrap()
                    .custom_emoji_revision += 1
            }
            _ => {
                shell.core.workspaces.get_mut(&owner.team).unwrap().locale =
                    Some("another-locale".into())
            }
        }
        assert!(!building_current(&shell, &owner, revision, &locale, ticket));
        // Same retire used by post-await decode/build/join failure clears only its ticket.
        shell.emoji_catalog.retire(&owner.team, ticket);
        assert!(shell.emoji_catalog.building.is_empty());
        let Prepared::Build {
            owner: newer,
            ticket: newer_ticket,
            ..
        } = prepare_local(
            &mut shell,
            |_| Ok(snapshot::Snapshot::Current(Vec::new())),
            false,
        )
        else {
            panic!("replacement refused")
        };
        shell.emoji_catalog.retire(&owner.team, ticket);
        assert_eq!(
            shell.emoji_catalog.building[&newer.team].ticket,
            newer_ticket
        );
        shell.emoji_catalog.retire(&newer.team, newer_ticket);
        assert!(shell.emoji_catalog.building.is_empty());
    }
}
