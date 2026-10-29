use super::*;
use crate::profile_edit::sections::{
    fixture,
    rich::{opening, query},
};

#[test]
fn mounted_profile_long_text_chooses_sdk_wire_without_borrowing_list_category_identity() {
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    fixture::install_interaction(&mut shell);
    let draft = shell.profile_edit.sections.draft.as_ref().unwrap();
    let (visit, serial) = (draft.visit, draft.serial);
    let opening = opening::begin(&mut shell, visit, serial, "long", 1).unwrap();
    assert!(opening::finish(&mut shell, opening));
    let lease = query::begin(&mut shell, visit, serial, 1, 1).unwrap();
    assert!(lease.current(&shell));
    assert!(lease.member_wire() == MemberWire::ProfileLongText);
    assert!(!<query::Lease as Host>::LIST);
}
