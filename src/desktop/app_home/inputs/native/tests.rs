use super::*;
fn setup() -> (ShellState, FormOwner, InputKey, u64) {
    let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
    fixture::install_inputs(&mut s);
    let owner = owner(&s).unwrap();
    let key = InputKey {
        block_id: "home-day".into(),
        action_id: "value".into(),
    };
    s.app_home
        .forms
        .get_mut(&owner)
        .unwrap()
        .set(&key, InputValue::Date(Some("2028-02-29".into())))
        .unwrap();
    seed(&mut s);
    let mount = s.app_home.native.entries[&(owner.clone(), key.clone())].mount;
    (s, owner, key, mount)
}
#[test]
fn invalid_native_empty_retains_previous_typed_value_and_blocks_action_until_actual_valid_clear() {
    let (mut s, owner, key, mount) = setup();
    let before = s.app_home.forms[&owner].fields[&key].value.clone();
    let revision = super::super::revision(&s);
    let event = Snapshot {
        mount,
        seq: 2,
        kind: "input".into(),
        value: String::new(),
        valid: false,
        bad: true,
    };
    assert!(!receive(&mut s, &owner, &key, mount, &event));
    assert_eq!(s.app_home.forms[&owner].fields[&key].value, before);
    assert!(s.app_home.forms[&owner].state().is_err());
    assert!(!super::super::ready(&s));
    assert_ne!(super::super::revision(&s), revision);
    let event = Snapshot {
        mount,
        seq: 3,
        kind: "input".into(),
        value: String::new(),
        valid: true,
        bad: false,
    };
    assert!(!receive(&mut s, &owner, &key, mount, &event));
    assert_eq!(
        s.app_home.forms[&owner].fields[&key].value,
        InputValue::Date(None)
    );
    assert!(s.app_home.forms[&owner].state().is_ok());
    assert!(super::super::ready(&s));
}
#[test]
fn closed_invalid_control_retains_owner_slot_and_old_mount_cannot_clear_after_reattachment() {
    let (mut s, owner, key, mount) = setup();
    let event = Snapshot {
        mount,
        seq: 2,
        kind: "closed".into(),
        value: String::new(),
        valid: false,
        bad: true,
    };
    assert!(!receive(&mut s, &owner, &key, mount, &event));
    let index = (owner.clone(), key.clone());
    assert!(s.app_home.native.entries[&index].pending);
    let new_mount = next();
    {
        let e = s.app_home.native.entries.get_mut(&index).unwrap();
        e.mount = new_mount;
        e.seq = 0;
    }
    let before = s.app_home.forms[&owner].clone();
    let old = Snapshot {
        mount,
        seq: 3,
        kind: "input".into(),
        value: String::new(),
        valid: true,
        bad: false,
    };
    assert!(!receive(&mut s, &owner, &key, mount, &old));
    assert_eq!(s.app_home.forms[&owner], before);
    let valid = Snapshot {
        mount: new_mount,
        seq: 1,
        kind: "mounted".into(),
        value: "2030-04-05".into(),
        valid: true,
        bad: false,
    };
    assert!(!receive(&mut s, &owner, &key, new_mount, &valid));
    assert_eq!(
        s.app_home.forms[&owner].fields[&key].value,
        InputValue::Date(Some("2030-04-05".into()))
    );
}
#[test]
fn retired_visit_may_store_queued_owned_input_but_foreign_account_cannot_change_it() {
    let (mut s, owner, key, mount) = setup();
    s.app_home.surface = None;
    let event = Snapshot {
        mount,
        seq: 2,
        kind: "closed".into(),
        value: "2030-04-05".into(),
        valid: true,
        bad: false,
    };
    assert!(receive(&mut s, &owner, &key, mount, &event));
    assert_eq!(
        s.app_home.forms[&owner].fields[&key].value,
        InputValue::Date(Some("2030-04-05".into()))
    );
    assert!(
        !s.app_home
            .native
            .entries
            .contains_key(&(owner.clone(), key.clone()))
    );
    let before = s.app_home.forms[&owner].clone();
    s.core.account_epoch = s.core.account_epoch.wrapping_add(1);
    assert!(!receive(&mut s, &owner, &key, mount, &event));
    assert_eq!(s.app_home.forms[&owner], before);
}
