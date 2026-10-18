use super::*;
use serde_json::json;
fn shell() -> ShellState {
    let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
    fixture::install(&mut s);
    s
}
fn photo(hash: &str, custom: bool) -> protocol::Photo {
    protocol::Photo::from_profile(&json!({"avatar_hash":hash,"image_48":format!("https://fixture.slack.com/{hash}.png"),"image_original":if custom{json!(format!("https://fixture.slack.com/{hash}-original.png"))}else{Value::Null}})).unwrap()
}
fn pending_read(s: &mut ShellState, ack: bool, remove: bool) -> (Token, u64, u64, u64, u64) {
    let d = s.profile_edit.photo.draft.as_ref().unwrap();
    let token = d.token.clone();
    let visit = d.visit;
    let serial = d.serial;
    let id = next_id();
    let read = next_id();
    let a = PhotoAttempt {
        id,
        token: token.clone(),
        baseline: d.cap.as_ref().unwrap().photo.clone(),
        observed: d.observed.clone(),
        remove,
        phase: Phase::Checking,
        ack,
        read: Some(read),
    };
    s.profile_edit
        .photo
        .entries
        .entry(token.owner.clone())
        .or_default()
        .pending = Some(a);
    (token, visit, serial, id, read)
}
#[test]
fn cancel_drops_only_private_preview_preserving_unsaved_text_and_never_creates_a_mutation() {
    let mut s = shell();
    s.profile_edit.draft.as_mut().unwrap().values.0[0] = "Unsaved name".into();
    let visit = s.profile_edit.draft.as_ref().unwrap().visit;
    let id = s
        .profile_edit
        .photo
        .draft
        .as_ref()
        .unwrap()
        .preview
        .as_ref()
        .unwrap()
        .asset
        .id()
        .clone();
    assert!(s.media.ready_bytes(&id).is_some());
    assert!(intercept_close(&mut s, Some(visit)));
    assert!(
        s.media.ready_bytes(&id).is_none(),
        "private bytes leave with the pane"
    );
    assert!(s.profile_edit.photo.draft.is_none());
    assert_eq!(
        s.profile_edit.draft.as_ref().unwrap().values.0[0],
        "Unsaved name"
    );
    assert!(s.profile_edit.photo.entries.is_empty());
    assert!(s.profile_edit.fixture.as_ref().unwrap().attempts.is_empty());
}
#[test]
fn lost_reply_plus_changed_photo_is_not_success_and_exact_read_is_once_only() {
    let mut s = shell();
    let (t, v, serial, id, read) = pending_read(&mut s, false, false);
    let p = photo("changed", true);
    network::finish_check(&mut s, &t, v, serial, id, read, Ok(p.clone()));
    let e = &s.profile_edit.photo.entries[&t.owner];
    assert!(e.pending.is_some());
    assert!(e.checked.as_ref() == Some(&p));
    network::finish_check(
        &mut s,
        &t,
        v,
        serial,
        id,
        read,
        Ok(photo("late-duplicate", true)),
    );
    assert!(s.profile_edit.photo.entries[&t.owner].checked.as_ref() == Some(&p));
    assert!(presentation(&s).unwrap().can_keep);
    assert!(!presentation(&s).unwrap().can_save);
    let newer = photo("after-read-realtime", true);
    newer.adopt(
        s.core
            .workspaces
            .get_mut(&t.owner.team)
            .unwrap()
            .users
            .get_mut(&t.owner.user)
            .unwrap()
            .profile
            .as_mut()
            .unwrap(),
    );
    assert!(
        !presentation(&s).unwrap().can_keep,
        "a later realtime update invalidates the displayed check"
    );
}
#[test]
fn acknowledged_set_still_needs_changed_custom_photo_and_remove_needs_cleared_original() {
    for remove in [false, true] {
        let mut s = shell();
        let (t, v, serial, id, read) = pending_read(&mut s, true, remove);
        let unchanged = s.profile_edit.photo.entries[&t.owner]
            .pending
            .as_ref()
            .unwrap()
            .baseline
            .clone();
        network::finish_check(&mut s, &t, v, serial, id, read, Ok(unchanged));
        assert!(s.profile_edit.photo.entries[&t.owner].pending.is_some());
        let (t, v, serial, id, read) = pending_read(&mut s, true, remove);
        network::finish_check(
            &mut s,
            &t,
            v,
            serial,
            id,
            read,
            Ok(photo("canonical", !remove)),
        );
        assert!(s.profile_edit.photo.entries[&t.owner].pending.is_none());
    }
}
#[test]
fn foreign_owner_and_newer_realtime_photo_never_receive_old_read_projection() {
    let mut s = shell();
    let (t, v, serial, id, read) = pending_read(&mut s, true, false);
    let newer = photo("newer-realtime", true);
    newer.adopt(
        s.core
            .workspaces
            .get_mut(&t.owner.team)
            .unwrap()
            .users
            .get_mut(&t.owner.user)
            .unwrap()
            .profile
            .as_mut()
            .unwrap(),
    );
    network::finish_check(&mut s, &t, v, serial, id, read, Ok(photo("old-read", true)));
    assert!(local_photo(&s, &t.owner) == Some(newer.clone()));
    let entry = &s.profile_edit.photo.entries[&t.owner];
    assert!(
        entry.pending.is_some(),
        "a superseded ACK read is not final evidence"
    );
    assert!(entry.checked.is_none());
    assert!(
        s.profile_edit
            .photo
            .draft
            .as_ref()
            .unwrap()
            .cap
            .as_ref()
            .unwrap()
            .photo
            == newer
    );
    let expected_url = s
        .media
        .register_image(MediaAssetKind::Avatar, newer.url(), "image/png")
        .webview_uri_at(s.media_epoch);
    let shown = presentation(&s).unwrap();
    assert_eq!(shown.url.as_deref(), Some(expected_url.as_str()));
    assert!(!shown.can_keep && !shown.can_save && shown.can_check);
    let mut s = shell();
    let (t, v, serial, id, read) = pending_read(&mut s, true, false);
    let before = local_photo(&s, &t.owner);
    s.core.active_account = Some("another-account".into());
    network::finish_check(&mut s, &t, v, serial, id, read, Ok(photo("foreign", true)));
    assert!(local_photo(&s, &t.owner) == before);
    assert!(s.profile_edit.photo.entries[&t.owner].pending.is_some());
}
#[test]
fn cancel_and_reopen_honor_actual_retained_capability_cooldown_before_load_admission() {
    let mut s = shell();
    let draft = s.profile_edit.photo.draft.as_ref().unwrap();
    let token = draft.token.clone();
    let visit = draft.visit;
    s.profile_edit
        .photo
        .entries
        .entry(token.owner.clone())
        .or_default()
        .retry_at = now() + 30;
    assert!(intercept_close(&mut s, Some(visit)));
    assert!(
        open_shell(&mut s, visit).is_none(),
        "no permission/profile load admitted"
    );
    assert!(!presentation(&s).unwrap().busy);
    assert!(!presentation(&s).unwrap().can_choose);
    assert!(s.profile_edit.photo.draft.as_ref().unwrap().cap.is_none());
    assert!(intercept_close(&mut s, Some(visit)));
    s.profile_edit
        .photo
        .entries
        .get_mut(&token.owner)
        .unwrap()
        .retry_at = now() - 1;
    let (admitted, _) = open_shell(&mut s, visit).expect("explicit reopen after deadline");
    assert!(admitted == token);
    assert!(presentation(&s).unwrap().busy);
}
#[test]
fn pending_photo_blocks_text_and_status_and_failed_read_releases_no_write_authority() {
    let mut s = shell();
    let (t, v, serial, id, read) = pending_read(&mut s, false, false);
    assert!(super::super::blocks_status(&s));
    assert!(!super::super::presentation(&s).unwrap().can_save);
    network::finish_check(
        &mut s,
        &t,
        v,
        serial,
        id,
        read,
        Err(Error::RateLimited {
            retry_after_secs: Some(7),
        }),
    );
    let e = &s.profile_edit.photo.entries[&t.owner];
    assert!(e.pending.is_some());
    assert!(e.retry_at >= now() + 6);
    assert!(!presentation(&s).unwrap().can_check);
}
