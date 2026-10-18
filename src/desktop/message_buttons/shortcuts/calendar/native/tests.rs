use super::*;

#[test]
fn disabled_missing_share_option_has_empty_dom_value_but_retains_full_draft_identity() {
    let (mut s, id) = super::super::tests::setup();
    let c = active_mut(&mut s, id).unwrap();
    c.share = false;
    c.share_channel = "C_UNAVAILABLE".into();
    c.share_choices.clear();
    let captured = Snapshot::capture(&s, id).unwrap();
    assert_eq!(captured.fields["calendar-share-channel"].value, "");
    assert_eq!(captured.share_channel, "C_UNAVAILABLE");
    assert!(captured.event.share_channel.is_empty());
    assert_eq!(active(&s, id).unwrap().share_channel, "C_UNAVAILABLE");
    active_mut(&mut s, id).unwrap().share_channel = "C_NEW_UNAVAILABLE".into();
    assert!(Snapshot::capture(&s, id).as_ref() != Some(&captured));
    active_mut(&mut s, id).unwrap().share = true;
    assert!(Snapshot::capture(&s, id).is_none());
    let c = active_mut(&mut s, id).unwrap();
    c.share = false;
    c.share_choices.push(cal::Choice {
        id: c.share_channel.clone(),
        name: "Retained choice".into(),
    });
    let shown = Snapshot::capture(&s, id).unwrap();
    assert_eq!(
        shown.fields["calendar-share-channel"].value,
        "C_NEW_UNAVAILABLE"
    );
}

#[test]
fn final_snapshot_rechecks_actual_draft_owner_and_uncertain_book() {
    let (mut s, id) = super::super::tests::setup();
    let original = Snapshot::capture(&s, id).unwrap();
    active_mut(&mut s, id).unwrap().title = "newer local input".into();
    assert!(Snapshot::capture(&s, id).as_ref() != Some(&original));
    active_mut(&mut s, id).unwrap().title = original.event.title.clone();
    assert!(Snapshot::capture(&s, id).as_ref() == Some(&original));
    active_mut(&mut s, id).unwrap().confirm_close = true;
    assert!(Snapshot::capture(&s, id).is_none());
    active_mut(&mut s, id).unwrap().confirm_close = false;
    active_mut(&mut s, id).unwrap().stage = Stage::Uncertain;
    assert!(Snapshot::capture(&s, id).is_none());
    active_mut(&mut s, id).unwrap().stage = Stage::Editing;
    s.channel_generation += 1;
    assert!(Snapshot::capture(&s, id).is_none());
    assert!(s.message_buttons.shortcuts.calendar_attempts.is_empty());
}

#[test]
fn manual_custom_time_is_not_required_to_be_a_suggested_option() {
    let (mut s, id) = super::super::tests::setup();
    active_mut(&mut s, id).unwrap().slots.clear();
    let snapshot = Snapshot::capture(&s, id).unwrap();
    assert_eq!(snapshot.fields["calendar-custom-slot"].value, "");
    assert_eq!(snapshot.fields["calendar-start"].value, "09:15");
    assert_eq!(snapshot.fields["calendar-start"].kind, "time");
    let original = snapshot.clone();
    active_mut(&mut s, id).unwrap().account = Some(cal::Account::Modern);
    let modern = Snapshot::capture(&s, id).unwrap();
    assert!(modern != original);
    assert!(!modern.fields.contains_key("calendar-share"));
    assert!(!modern.fields.contains_key("calendar-share-channel"));
}
